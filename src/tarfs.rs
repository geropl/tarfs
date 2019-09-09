use std::path::{Path};
use std::ffi::{OsStr};
use std::{path::PathBuf};
use std::io;
#[allow(unused_imports)]
use std::cell::RefCell;
use std::sync::mpsc;

use time::Timespec;

use libc::{ENOENT, ENODATA};

use fuse;
use fuse::{FileAttr, FileType, Filesystem, Request, ReplyAttr, ReplyEntry, ReplyDirectory, ReplyData};

use log;
use log::{debug, info, error, trace};

use super::tarindex::{TarIndex};

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

        let entry = match self.index.lookup_child(parent, PathBuf::from(name)) {
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
        reply.entry(&ttl_max(), &entry.attrs, 0);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        debug!("getattr(ino={})", ino);

        let entry = match self.index.get_entry_by_ino(ino) {
            None => {
                reply.error(ENOENT);
                error!("lookup: no entry");
                return
            },
            Some(e) => e,
        };

        reply.attr(&ttl_max(), &entry.attrs);
    }

    fn readdir(&mut self, _req: &Request, ino: u64, fh: u64, offset: i64, mut reply: ReplyDirectory) {
        debug!("readdir(ino={}, fh={}, offset={})", ino, fh, offset);

        let entry = match self.index.get_entry_by_ino(ino) {
            None => {
                reply.error(ENOENT);
                error!("readdir: no entry");
                return
            },
            Some(e) => e,
        };

        if entry.attrs.kind != fuse::FileType::Directory {
            error!("readdir: ino {} is no dir!", ino);
            return
        }

        let mut full;
        if offset == 0 {
            let off = 1;
            let kind = FileType::Directory;
            full = reply.add(entry.ino, off, kind, ".");
            trace!("reply.add inode {}, offset {}, file_type {:?}, base {} ", ino, off, kind, ".");
            if full {
                reply.ok();
                return
            }
        }

        if offset <= 1 {
            // Handle fs root: same ino as
            let ino = match entry.parent_ino {
                None => entry.ino,
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
        for child in &entry.children.borrow()[children_offset as usize..] {
            let ino = child.ino;
            let kind = child.attrs.kind;
            let name = &child.name;
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

        let entry = match self.index.get_entry_by_ino(ino) {
            None => {
                reply.error(ENOENT);
                error!("lookup: no entry");
                return
            },
            Some(e) => e.clone(),
        };

        let bytes = match self.index.read(&entry, offset as u64, size as u64) {
            Err(e) => {
                error!("Error reading from file {}: {}", entry.path.display(), e);
                reply.error(ENODATA);
                return
            },
            Ok(bytes) => bytes,
        };
        reply.data(&bytes);
    }

    fn readlink(&mut self, _req: &Request, ino: u64, reply: ReplyData) {
        debug!("readlink(ino={})", ino);

        let entry = match self.index.get_entry_by_ino(ino) {
            None => {
                reply.error(ENOENT);
                error!("readlink: no entry");
                return
            },
            Some(e) => e.clone(),
        };

        match &entry.link_name {
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
