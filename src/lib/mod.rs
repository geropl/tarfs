// fs
extern crate fuse;
extern crate libc;
extern crate time;

// index
extern crate tar;

mod tarindex;
mod tarfs;

use std::{fs, fs::File};
use std::path::Path;

use tarindex::TarIndex;
use tarfs::TarFs;

pub fn setup_tar_mount(filepath: &Path, mountpoint: &Path) -> Result<(), Box<std::error::Error>> {
    if mountpoint.exists() {
        fs::remove_dir(&mountpoint)?;
    }
    fs::create_dir(&mountpoint)?;

    let file = File::open(filepath)?;
    let index = TarIndex::new_from(&file)?;

    let tar_fs = TarFs::new(&index);
    tar_fs.mount(mountpoint);

    Ok(())
}