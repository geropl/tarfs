use failure::Fail;

mod tarindex;
mod tarindexer;
mod tarfs;

use failure::Error;

use std::{fs, fs::File};
use std::path::Path;
use std::sync::mpsc;

use tarindexer::{TarIndexer, Options, Permissions};
use tarfs::TarFs;

#[derive(Debug, Fail)]
pub enum TarFsError {
    #[fail(display = "{}", text)]
    MountError {
        text: String,
    },
}

pub fn setup_tar_mount(filepath: &Path, mountpoint: &Path, start_signal: Option<mpsc::SyncSender<()>>) -> Result<(), Error> {
    ensure_mountpoint_dir_exists(mountpoint)?;

    // Make the fs root dir permissions the ones from the mountpoint
    let mountpoint_meta = mountpoint.metadata()?;
    let options = Options {
        root_permissions: permissions_from_mountpoint(&mountpoint_meta),
    };

    // Open archive and index it
    let file = File::open(filepath)?;
    let indexer = TarIndexer{};
    let mut index = indexer.build_index_for(&file, &options)?;

    // And finally: Mount it
    let start_signal = match start_signal {
        Some(s) => s,
        None => mpsc::sync_channel(1).0,
    };
    let tar_fs = TarFs::new(&mut index, start_signal);
    tar_fs.mount(mountpoint)?;

    Ok(())
}

fn ensure_mountpoint_dir_exists(mountpoint: &Path) -> Result<(), TarFsError> {
    if !mountpoint.exists() || !mountpoint.is_dir() {
        return Err(TarFsError::MountError{ text: String::from("mountpoint is not a directory")}.into());
    }
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
