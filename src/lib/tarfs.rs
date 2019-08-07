
use std::path::{Path};
use std::ffi::{OsStr};
use std::{path::PathBuf};
use std::io;
#[allow(unused_imports)]
use std::cell::RefCell;

use time::Timespec;

use libc::{ENOENT};

use fuse;
use fuse::{FileType, Filesystem, Request, ReplyAttr, ReplyEntry, ReplyDirectory};

use tar::EntryType;

use super::tarindex::{TarIndex};

pub struct TarFs<'f> {
    index: &'f TarIndex<'f>
}

impl<'f> TarFs<'f> {
    pub fn new(index: &'f TarIndex<'f>) -> TarFs<'f> {
        TarFs{
            index
        }
    }

    pub fn mount(self, mountpoint: &Path) -> io::Result<()> {
        // TODO Would be cool to use fuse::spawn_mount here..
        // But moving TarFs across thread boundaries seems impossible
        fuse::mount(self, &mountpoint, &[])
    }
}

impl<'f> Filesystem for TarFs<'f> {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let path = PathBuf::from(name);
        println!("lookup(parent={}, name={})", parent, path.to_str().unwrap());

        let node = match self.index.lookup_child(parent, PathBuf::from(name)) {
            Some(a) => a,
            None => {
                reply.error(ENOENT);
                println!("lookup: no parent entry");
                return;
            },
        };
        reply.entry(&ttl(10), &node.attrs(), 0);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        println!("getattr(ino={})", ino);

        let node = match self.index.get_node_by_id(ino) {
            None => {
                reply.error(ENOENT);
                println!("lookup: no entry");
                return
            },
            Some(n) => n,
        };

        reply.attr(&ttl(10), &node.attrs());
    }

    fn readdir(&mut self, _req: &Request, ino: u64, fh: u64, offset: i64, mut reply: ReplyDirectory) {
        println!("readdir(ino={}, fh={}, offset={})", ino, fh, offset);

        let node = match self.index.get_node_by_id(ino) {
            None => {
                reply.error(ENOENT);
                println!("readdir: no entry");
                return
            },
            Some(n) => n,
        };

        if node.entry.ftype != EntryType::Directory {
            println!("readdir: ino {}, index {} is no dir!", ino, node.entry.index);
            return
        }

        let mut out_off: i64 = 1;
        let get = |off: &mut i64| -> i64 {
            let res = *off;
            *off += 1;
            res
        };

        let mut full;
        if offset == 0 {
            let off = get(&mut out_off);
            let kind = FileType::Directory;
            full = reply.add(node.ino(), off, kind, ".");
            println!("reply.add inode {}, offset {}, file_type {:?}, base {} ", ino, off, kind, ".");
            if full {
                reply.ok();
                return
            }
        }

        if offset <= 1 {
            // Handle fs root: same ino as
            let ino = match node.parent_id {
                None => node.ino(),
                Some(ino) => ino,
            };

            let off = get(&mut out_off);
            let kind = FileType::Directory;
            full = reply.add(ino, off, kind, "..");
            println!("reply.add inode {}, offset {}, file_type {:?}, base {} ", ino, off, kind, "..");
            if full {
                reply.ok();
                return
            }
        }

        let c_offset = (offset - 2).max(0) as usize;
        for child in &node.children.borrow()[c_offset..] {
            let ino = child.ino();
            let kind = child.attrs().kind;
            let name = &child.entry.name;
            let off = get(&mut out_off);
            println!("reply.add inode {}, offset {}, file_type {:?}, base {} ", ino, off, kind, name.display());
            full = reply.add(ino, off, kind, name);
            if full {
                break;
            }
        }
        reply.ok();
    }

    // fn read(&mut self, _req: &Request, ino: u64, fh: u64, offset: u64, size: uint, reply: ReplyData) {
    //     println!("read(ino={}, fh={}, offset={}, size={})", ino, fh, offset, size);
    //     for (key, &inode) in self.inodes.iter() {
    //         if inode == ino {
    //             let value = self.tree.get(key).unwrap();
    //             reply.data(value.to_pretty_str().as_bytes());
    //             return;
    //         }
    //     }
    //     reply.error(ENOENT);
    // }
}

fn ttl(secs: i64) -> Timespec {
    Timespec::new(secs, 0)
}
