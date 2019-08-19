use libc::ENODATA;
use std::path::{Path};
use std::ffi::{OsStr};
use std::{path::PathBuf};
use std::io;
#[allow(unused_imports)]
use std::cell::RefCell;
use std::sync::mpsc;

use time::Timespec;

use libc::{ENOENT};

use fuse;
use fuse::{FileAttr, FileType, Filesystem, Request, ReplyAttr, ReplyEntry, ReplyDirectory, ReplyData};

use tar::EntryType;

use log;
use log::{debug, info, error, trace};

use super::tarindex::{TarIndex, INode};

const NAME_OPTIONS: &[&str] = &[
    "fsname=tarfs",
    "subtype=tarfs",
];

const DEFAULT_OPTIONS: &[&str] = &[
    // http://manpages.ubuntu.com/manpages/bionic/en/man8/mount.fuse.8.html#options
    "default_permissions",  // Enable default kernel permission handling
    "allow_other",          // Allow other users to access the files
    "kernel_cache",         // Disable flushing the kernel cache on each "open"
    "use_ino",              // IDK what it could mean to have this disabled...
];

pub struct TarFs<'f> {
    index: &'f mut TarIndex<'f>,
    pub start_signal: mpsc::SyncSender<()>,
}

impl<'f> TarFs<'f> {
    pub fn new(index: &'f mut TarIndex<'f>, start_signal: mpsc::SyncSender<()>) -> TarFs<'f> {
        TarFs{
            index,
            start_signal,
        }
    }

    pub fn mount(self, mountpoint: &Path) -> io::Result<()> {
        let oss = &mut Vec::new();
        oss.extend(NAME_OPTIONS);
        oss.extend(DEFAULT_OPTIONS);
        let options = fuse_optionize(oss);

        info!("tarfs mounted.");
        // TODO Would be cool to use fuse::spawn_mount here..
        // But moving TarFs across thread boundaries seems impossible
        let res = fuse::mount(self, &mountpoint, &options);
        info!("tarfs unmounted.");
        res
    }
}

fn fuse_optionize<'a>(os: &Vec<&'a str>) -> Vec<&'a OsStr> {
    let mut result: Vec<&OsStr> = vec!();
    let opts = os.iter()
            .map(|o| o.to_owned().as_ref())
            .collect::<Vec<&OsStr>>();
    for i in (opts.len() - 1)..0 {
        result.insert(i, "-o".as_ref());
    }
    result
}

impl<'f> Filesystem for TarFs<'f> {
    fn init(&mut self, _req: &Request) -> Result<(), i32> {
        // Signal start
        if let Err(_) = self.start_signal.send(()) {
            // Do nothing
        }
        Ok(())
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let path = PathBuf::from(name);
        debug!("lookup(parent={}, name={})", parent, path.to_str().unwrap());

        let node = match self.index.lookup_child(parent, PathBuf::from(name)) {
            Some(a) => a,
            None => {
                // According to https://github.com/libfuse/libfuse/blob/master/include/fuse_lowlevel.h#L60
                // this enables caching of none-entries (negative caching)
                let attrs = emtpy_attr();
                reply.entry(&ttl_max(), &attrs, 0);
                // reply.error(ENOENT);
                debug!("lookup: no entry");
                return;
            },
        };
        reply.entry(&ttl_max(), &attrs(&node), 0);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        debug!("getattr(ino={})", ino);

        let node = match self.index.get_node_by_ino(ino) {
            None => {
                reply.error(ENOENT);
                error!("lookup: no entry");
                return
            },
            Some(n) => n,
        };

        reply.attr(&ttl_max(), &attrs(&node));
    }

    fn readdir(&mut self, _req: &Request, ino: u64, fh: u64, offset: i64, mut reply: ReplyDirectory) {
        debug!("readdir(ino={}, fh={}, offset={})", ino, fh, offset);

        let node = match self.index.get_node_by_ino(ino) {
            None => {
                reply.error(ENOENT);
                error!("readdir: no entry");
                return
            },
            Some(n) => n,
        };

        if node.entry.ftype != EntryType::Directory {
            error!("readdir: ino {}, index {} is no dir!", ino, node.entry.index);
            return
        }

        let mut full;
        if offset == 0 {
            let off = 1;
            let kind = FileType::Directory;
            full = reply.add(node.ino, off, kind, ".");
            trace!("reply.add inode {}, offset {}, file_type {:?}, base {} ", ino, off, kind, ".");
            if full {
                reply.ok();
                return
            }
        }

        if offset <= 1 {
            // Handle fs root: same ino as
            let ino = match node.parent_id {
                None => node.ino,
                Some(ino) => ino,
            };

            let off = 2;
            let kind = FileType::Directory;
            full = reply.add(ino, off, kind, "..");
            trace!("reply.add inode {}, offset {}, file_type {:?}, base {} ", ino, off, kind, "..");
            if full {
                reply.ok();
                return
            }
        }

        let children_offset = (offset - 2).max(0);
        let mut off: i64 = 2 + children_offset + 1;
        for child in &node.children.borrow()[children_offset as usize..] {
            let ino = child.ino;
            let kind = attrs(child).kind;
            let name = &child.entry.name;
            trace!("reply.add inode {}, offset {}, file_type {:?}, base {} ", ino, off, kind, name.display());
            full = reply.add(ino, off, kind, name);
            off += 1;
            if full {
                break;
            }
        }
        reply.ok();
    }

    fn read(&mut self, _req: &Request, ino: u64, fh: u64, offset: i64, size: u32, reply: ReplyData) {
        debug!("read(ino={}, fh={}, offset={}, size={})", ino, fh, offset, size);

        let node = match self.index.get_node_by_ino(ino) {
            None => {
                reply.error(ENOENT);
                error!("lookup: no entry");
                return
            },
            Some(n) => n.clone(),
        };

        let bytes = match self.index.read(&node, offset as u64, size as u64) {
            Err(e) => {
                error!("Error reading from file {}: {}", node.entry.path.display(), e);
                reply.error(ENODATA);
                return
            },
            Ok(bytes) => bytes,
        };
        reply.data(&bytes);
    }

    fn readlink(&mut self, _req: &Request, ino: u64, reply: ReplyData) {
        debug!("readlink(ino={})", ino);

        let node = match self.index.get_node_by_ino(ino) {
            None => {
                reply.error(ENOENT);
                error!("readlink: no entry");
                return
            },
            Some(n) => n.clone(),
        };

        match &node.entry.link_name {
            Some(path) => {
                use std::os::unix::ffi::OsStrExt;

                let bytes = path.as_os_str().as_bytes();
                reply.data(bytes);
            },
            None => {
                error!("readlink: no link_name");
                return
            }
        }
    }
}


fn attrs(node: &INode) -> fuse::FileAttr {
    let kind = tar_entrytype_to_filetype(node.entry.ftype);
    let mtime = Timespec::new(node.entry.mtime as i64, 0);
    let size = match &node.entry.link_name {
        // For symlinks, fuse wants the length of the OsStr...
        Some(ln) => ln.as_os_str().len() as u64,
        None => match kind {
            fuse::FileType::Directory => 4096,    // We're mimicking ext4 here
            _ => node.entry.filesize,       // The default case: Size "on disk" is the same as the size in the tar (uncompressed) archive
        },
    };
    fuse::FileAttr {
        ino: node.ino,
        size,
        blocks: 0,
        atime: mtime,
        mtime: mtime,
        ctime: mtime,
        crtime: mtime, // macOS only
        kind,
        perm: node.entry.mode as u16,
        nlink: 1,
        uid: node.entry.uid as u32,
        gid: node.entry.gid as u32,
        rdev: 0,
        flags: 0,
    }
}

fn tar_entrytype_to_filetype(ftype: tar::EntryType) -> fuse::FileType {
    match ftype {
        EntryType::Regular => FileType::RegularFile,
        EntryType::Directory => FileType::Directory,
        EntryType::Symlink => FileType::Symlink,
        t => {
            println!("Unsupported EntryType: {:?}", t);
            FileType::RegularFile
        },
        // EntryType::Link => FileType::
    }
}

fn emtpy_attr() -> FileAttr {
    FileAttr {
        ino: 0,
        size: 0,
        blocks: 0,
        atime: Timespec::new(0, 0),
        mtime: Timespec::new(0, 0),
        ctime: Timespec::new(0, 0),
        crtime: Timespec::new(0, 0),
        kind: FileType::RegularFile,
        perm: 0,
        nlink: 0,
        uid: 0,
        gid: 0,
        rdev: 0,
        flags: 0,
    }
}

/// As tarfs is a static file system in which files will never change, we use the highest possible timeout for entries and attributes read by the kernel
/// Reference: Here's the best documentation about timeouts I could find: https://github.com/libfuse/libfuse/blob/master/include/fuse_lowlevel.h#L90
fn ttl_max() -> Timespec {
    Timespec::new(std::i64::MAX, 0)
}
