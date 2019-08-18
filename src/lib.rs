// fs
extern crate fuse;
extern crate libc;
extern crate time;

// index
extern crate tar;

// both
extern crate log;
extern crate env_logger;

pub mod tarindex;
pub mod tarfs;

use std::{fs, fs::File};
use std::path::Path;
use std::sync::mpsc;

use tarindex::TarIndexer;
use tarfs::TarFs;

pub fn setup_tar_mount(filepath: &Path, mountpoint: &Path, start_signal: Option<mpsc::SyncSender<()>>) -> Result<(), Box<dyn std::error::Error>> {
    if mountpoint.exists() {
        fs::remove_dir(&mountpoint)?;
    }
    fs::create_dir_all(&mountpoint)?;

    let file = File::open(filepath)?;
    let mut index = TarIndexer::build_index_for(&file)?;

    let start_signal = match start_signal {
        Some(s) => s,
        None => mpsc::sync_channel(1).0,
    };
    let tar_fs = TarFs::new(&mut index, start_signal);
    tar_fs.mount(mountpoint)?;

    Ok(())
}