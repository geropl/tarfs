extern crate env_logger;
extern crate tarfslib as lib;
extern crate clap;

use clap::{App, Arg};

use std::path::PathBuf;

fn main() -> Result<(), Box<std::error::Error>>  {
    let matches = App::new("tarfs")
        .version("1.0")
        .author("Gero Posmyk-Leinemann <geroleinemann@gmx.de>")
        .about("A readonly FUSE filesystem that allows to mount tar files")
        .arg(Arg::with_name("archive")
            .short("a")
            .long("archive")
            .help("The tar file that should be mounted")
            .required(true)
            .takes_value(true)
            .index(1))
        .arg(Arg::with_name("mountpoint")
            .short("m")
            .long("mountpoint")
            .help("The path to the directory where the archive should be mounted")
            .required(true)
            .takes_value(true)
            .index(2))
        .get_matches();

    let filename = PathBuf::from(matches.value_of("archive").unwrap());
    let mountpoint = PathBuf::from(matches.value_of("mountpoint").unwrap());

    env_logger::init();
    lib::setup_tar_mount(&filename, &mountpoint, None)?;

    Ok(())
}
