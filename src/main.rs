extern crate fuse;
extern crate libc;
extern crate time;

use std::path::Path;
use std::env;

use std::io::{FileType, USER_FILE, USER_DIR};
use std::mem;
use std::os;
use libc::{ENOENT, ENOSYS};
use time::Timespec;
use fuse::{FileAttr, Filesystem, Request, ReplyAttr, ReplyEntry, ReplyDirectory};

struct TarFilesystem;

impl Filesystem for TarFilesystem {
    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        println!("getattr(ino={})", ino);
        let mut attr: FileAttr = unsafe { mem::zeroed() };
        attr.ino = 1;
        attr.kind = FileType::Directory;
        attr.perm = USER_DIR;
            let ttl = Timespec::new(1, 0);
        if ino == 1 {
            reply.attr(&ttl, &attr);
        } else {
            reply.error(ENOSYS);
        }
    }
    fn readdir(&mut self, _req: &Request, ino: u64, fh: u64, offset: i64, mut reply: ReplyDirectory) {
        println!("readdir(ino={}, fh={}, offset={})", ino, fh, offset);
        reply.error(ENOSYS);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mountpoint = match args.as_slice() {
        [_, ref path] => Path::new(path),
        _ => {
            println!("Usage: {} <MOUNTPOINT>", args.as_slice()[0]);
            return;
        }
    };
    fuse::mount(TarFilesystem, &mountpoint, &[]).unwrap();
}