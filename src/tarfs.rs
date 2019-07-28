
use std::path::{Path};
use std::ffi::{OsStr, OsString};
use std::{path::PathBuf};

use time::Timespec;

use libc::{ENOENT};

use fuse;
use fuse::{FileAttr, FileType, Filesystem, Request, ReplyAttr, ReplyEntry, ReplyDirectory};

use crate::tarindex::{TarIndex, TarIndexEntry};

pub struct TarFs<'f> {
    index: &'f TarIndex<'f>
}

impl<'f> TarFs<'f> {
    pub fn new(index: &'f TarIndex<'f>) -> TarFs<'f> {
        TarFs{
            index
        }
    }

    pub fn mount(self, mountpoint: &Path) {
        fuse::mount(self, &mountpoint, &[]).unwrap();
    }

    fn attr_default(&self, ino: u64) -> FileAttr {
        let ts = time::now().to_timespec();
        FileAttr {
            ino,
            size: 0,
            blocks: 0,
            atime: ts,
            mtime: ts,
            ctime: ts,
            crtime: ts,
            kind: FileType::Directory,
            perm: 0o777,
            nlink: 0,
            uid: 33333,
            gid: 33333,
            rdev: 0,
            flags: 0,
        }
    }

    fn attrs_for_entry(&self, entry: &TarIndexEntry) -> FileAttr {
        let mut attr = self.attr_default(index_to_inode(entry.index));
        attr.kind = match entry.is_dir() {
            true => FileType::Directory,
            false => FileType::RegularFile
        };
        // println!("kind: {:?}", attr.kind);
        attr
    }

    fn add_entries_to_reply(&self, reply: &mut ReplyDirectory, prefix: &Path, entry_offset: usize, count_offset: i64) {
        let entries = self.index.get_entries_by_path_prefix(prefix);
        let len = entries.len();
        if entry_offset >= len {
            return
        }

        let mut c: i64 = 1;
        for entry in &entries[entry_offset..] {
            let index = entry.index;
            let inode = index_to_inode(index);
            let offset = count_offset as i64 + c;
            let file_type = if entry.is_dir() { FileType::Directory } else { FileType::RegularFile };
            let base = match entry.path.strip_prefix(prefix) {
                Err(_) => continue,
                Ok(base) => base
            };
            println!("reply.add inode {}, offset {}, file_type {:?}, base {} ", inode, offset, file_type, base.display());
            reply.add(inode, offset, file_type, &OsString::from(&base));
            c = c + 1;
        }
        println!("readdir: {} entries ", len);
    }

    fn path_for_inode(&self, parent: u64) -> Option<PathBuf> {
        if parent == 1 {
            return Some(PathBuf::from("."));
        }

        match self.index.get_entry_by_index(parent) {
            Some(e) => Some(PathBuf::from(&e.path)),
            None => None
        }
    }
}

impl<'f> Filesystem for TarFs<'f> {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let path = PathBuf::from(name);
        println!("lookup(parent={}, name={})", parent, path.to_str().unwrap());

        let parent_path = match self.path_for_inode(parent) {
            Some(e) => e,
            None => {
                reply.error(ENOENT);
                println!("lookup: no parent entry");
                return;
            }
        };

        let child_path = parent_path.as_path().join(path);
        println!("child_path: {}", child_path.display());
        let entry = match self.index.get_entry_by_path(&child_path) {
            Some(e) => e,
            None => {
                reply.error(ENOENT);
                println!("lookup: no child entry");
                return;
            },
        };

        let attr = self.attrs_for_entry(entry);
        reply.entry(&ttl(10), &attr, 0);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        println!("getattr(ino={})", ino);

        let mut attr = self.attr_default(ino);
        if ino == 1 {
            attr.kind = FileType::Directory;
            attr.perm = 0o777;
            reply.attr(&ttl(10), &attr);
            return
        }

        //  else {
        //     reply.error(ENOSYS);
        // }

        let entry = match self.index.get_entry_by_index(ino) {
            None => {
                reply.error(ENOENT);
                println!("lookup: no entry");
                return
            },
            Some(e) => e
        };

        attr = self.attrs_for_entry(entry);
        reply.attr(&ttl(10), &attr);
    }

    fn readdir(&mut self, _req: &Request, ino: u64, fh: u64, offset: i64, mut reply: ReplyDirectory) {
        println!("readdir(ino={}, fh={}, offset={})", ino, fh, offset);

        if ino == 1 {
            if offset <= 1 {
                reply.add(1, 0, FileType::Directory, ".");
                reply.add(1, 1, FileType::Directory, "..");
                self.add_entries_to_reply(&mut reply, &Path::new("."), 0, 2);
            } else {
                self.add_entries_to_reply(&mut reply, &Path::new("."), offset as usize, offset);
            }

            reply.ok();
            return
        }

        let index = inode_to_index(ino);
        let entry = match self.index.get_entry_by_index(index) {
            None => {
                reply.error(ENOENT);
                println!("readdir: no entry");
                return
            },
            Some(e) => e
        };
        if !entry.is_dir() {
            println!("readdir: ino {}, index {} is no dir!", ino, entry.index);
            return
        }

        self.add_entries_to_reply(&mut reply, &entry.path.as_path(), offset as usize, 0);
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

fn index_to_inode(index: u64) -> u64 {
    index + 2
}

fn inode_to_index(ino: u64) -> u64 {
    ino - 2
}
