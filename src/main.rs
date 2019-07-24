extern crate fuse;

use std::os;
use fuse::Filesystem;

struct TarFilesystem;

impl Filesystem for TarFilesystem {
}

fn main() {
    let mountpoint = match os::args().as_slice() {
        [_, ref path] => Path::new(path),
        _ => {
            println!("Usage: {} <MOUNTPOINT>", os::args()[0]);
            return;
        }
    };
    fuse::mount(TarFilesystem, &mountpoint, &amp;[]);
}