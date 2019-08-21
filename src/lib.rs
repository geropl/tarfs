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
mod tarindexer;
mod tarfs;

use std::{fs, fs::File};
use std::path::Path;
use std::sync::mpsc;
use std::fmt;
use std::error::Error;

use tarindexer::{TarIndexer, Options, Permissions};
use tarfs::TarFs;

pub fn setup_tar_mount(filepath: &Path, mountpoint: &Path, start_signal: Option<mpsc::SyncSender<()>>) -> Result<(), Box<dyn std::error::Error>> {
    if !mountpoint.exists() || !mountpoint.is_dir() {
        return Err(Box::new(MountError::new("mountpoint is not a directory")));
    }

    // Make the fs root dir permissions the ones from the mountpoint
    let mountpoint_meta = mountpoint.metadata()?;
    let options = Options {
        root_permissions: permissions_from_mountpoint(&mountpoint_meta),
    };

    // Open archive and index it
    let file = File::open(filepath)?;
    let mut index = TarIndexer::build_index_for(&file, &options)?;

    // And finally: Mount it
    let start_signal = match start_signal {
        Some(s) => s,
        None => mpsc::sync_channel(1).0,
    };
    let tar_fs = TarFs::new(&mut index, start_signal);
    tar_fs.mount(mountpoint)?;

    Ok(())
}

fn permissions_from_mountpoint(meta: &fs::Metadata) -> Permissions {
    use std::os::unix::fs::PermissionsExt;
    use std::os::linux::fs::MetadataExt;
    let p = meta.permissions();
    Permissions {
        mode: p.mode(),
        uid: meta.st_uid() as u64,
        gid: meta.st_gid() as u64,
    }
}

#[derive(Debug, Clone)]
struct MountError {
    text: &'static str,
}

impl MountError {
    fn new(text: &'static str) -> MountError {
        MountError {
            text
        }
    }
}

impl fmt::Display for MountError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl Error for MountError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}
