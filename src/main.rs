// fs
extern crate fuse;
extern crate libc;
extern crate time;

// tar + indexer
extern crate tar;

mod tarindex;
mod tarfs;

use std::env;
use std::fs::File;
use std::path::Path;

use tarindex::TarIndex;
use tarfs::TarFs;

fn main() -> Result<(), Box<std::error::Error>>  {
    let args: Vec<String> = env::args().collect();
    let (filename, mountpoint) = match args.as_slice() {
        [_, ref filename, ref mountpoint] => Ok((Path::new(filename), Path::new(mountpoint))),
        _ => Err(format!("Usage: {} <MOUNTPOINT>", args.as_slice()[0]))
    }?;

    let file = File::open(filename)?;
    let index = TarIndex::new_from(&file)?;

    println!("{}", index);

    let tar_fs = TarFs::new(&index);
    tar_fs.mount(mountpoint);

    Ok(())
}