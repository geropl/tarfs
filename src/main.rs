// extern crate fuse;
// extern crate libc;
// extern crate time;

// use std::path::Path;
// use std::env;

// use std::io::{FileType, USER_FILE, USER_DIR};
// use std::mem;
// use std::os;
// use libc::{ENOENT, ENOSYS};
// use time::Timespec;
// use fuse::{FileAttr, Filesystem, Request, ReplyAttr, ReplyEntry, ReplyDirectory};

// struct TarFilesystem;

// impl Filesystem for TarFilesystem {
//     fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
//         println!("getattr(ino={})", ino);
//         let mut attr: FileAttr = unsafe { mem::zeroed() };
//         attr.ino = 1;
//         attr.kind = FileType::Directory;
//         attr.perm = USER_DIR;
//             let ttl = Timespec::new(1, 0);
//         if ino == 1 {
//             reply.attr(&ttl, &attr);
//         } else {
//             reply.error(ENOSYS);
//         }
//     }
//     fn readdir(&mut self, _req: &Request, ino: u64, fh: u64, offset: i64, mut reply: ReplyDirectory) {
//         println!("readdir(ino={}, fh={}, offset={})", ino, fh, offset);
//         reply.error(ENOSYS);
//     }
// }
extern crate tar;

use std::env::args_os;
use std::fs::File;
use std::path::Path;

mod indexer;
use indexer::TarIndexer;

fn main() {
    let first_arg = match args_os().skip(1).next() {
        None => {
            println!("No filename given");
            return
        },
        Some(arg) => arg,
    };
    let filename = Path::new(&first_arg);
    let file = match File::open(filename) {
        Err(v) => {
            println!("Error opening file: {}", v);
            return
        },
        Ok(f) => f,
    };

    let indexer = TarIndexer::new(&file);
    let index = match indexer.index() {
        Err(v) => {
            println!("Error indexing archive: {}", v);
            return
        },
        Ok(idx) => idx,
    };
    println!("{}", index);

    // let args: Vec<String> = env::args().collect();
    // let mountpoint = match args.as_slice() {
    //     [_, ref path] => Path::new(path),
    //     _ => {
    //         println!("Usage: {} <MOUNTPOINT>", args.as_slice()[0]);
    //         return;
    //     }
    // };
    // fuse::mount(TarFilesystem, &mountpoint, &[]).unwrap();
}