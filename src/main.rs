extern crate env_logger;

extern crate tarfslib;
use tarfslib as lib;

use std::env;
use std::path::Path;

fn main() -> Result<(), Box<std::error::Error>>  {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let (filename, mountpoint) = match args.as_slice() {
        [_, ref filename, ref mountpoint] => Ok((Path::new(filename), Path::new(mountpoint))),
        _ => Err(format!("Usage: {} <FILENAME> <MOUNTPOINT>", args.as_slice()[0]))
    }?;

    lib::setup_tar_mount(filename, mountpoint)?;

    Ok(())
}