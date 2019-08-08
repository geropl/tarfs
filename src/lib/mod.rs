// fs
extern crate fuse;
extern crate libc;
extern crate time;

// index
extern crate tar;

// both
extern crate log;
extern crate env_logger;

mod tarindex;
mod tarfs;

use std::{fs, fs::File};
use std::path::Path;

use tarindex::TarIndexer;
use tarfs::TarFs;

pub fn setup_tar_mount(filepath: &Path, mountpoint: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if mountpoint.exists() {
        fs::remove_dir(&mountpoint)?;
    }
    fs::create_dir_all(&mountpoint)?;

    let file = File::open(filepath)?;
    let index = TarIndexer::build_index_for(&file)?;

    let tar_fs = TarFs::new(&index);
    tar_fs.mount(mountpoint)?;

    Ok(())
}