
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;
use std::fs;
use std::thread;
use std::sync::mpsc::sync_channel;

type TarFsTestResult = Result<(), Box<std::error::Error>>;

pub struct TarFsTest {
    filename: PathBuf,
    mountpoint: PathBuf
}

impl TarFsTest {
    pub fn new(filename: &str, mountpoint: &str) -> TarFsTest {
        TarFsTest {
            filename: PathBuf::from(filename),
            mountpoint: PathBuf::from(mountpoint)
        }
    }

    pub fn perform(&self, test: fn(&Path) -> TarFsTestResult) -> TarFsTestResult {
        self.setup_fs_mnt()?;

        test(&self.mountpoint)?;

        Ok(())
    }

    fn setup_fs_mnt(&self) -> TarFsTestResult {
        let filename = PathBuf::from(self.filename.to_str().unwrap());
        let mountpoint = PathBuf::from(self.mountpoint.to_str().unwrap());

        // Make sure we aren't comparing apples with oranges
        if mountpoint.exists() {
            fs::remove_dir(&mountpoint)?;
        }
        fs::create_dir_all(&mountpoint)?;

        let (tx, rx) = sync_channel(1);
        thread::spawn(move || {
            match tarfslib::setup_tar_mount(&filename, &mountpoint, Some(tx)) {
                Ok(_) => (),
                Err(e) => println!("setup_tar_mount error: {}", e)
            }
        });
        let r = rx.recv();
        if let Err(e) = r {
            eprintln!("error: {}", e);
        }

        Ok(())
    }

    fn teardown_fs_mnt(&self) {
        match Command::new("sudo")
            .args(&["umount", self.mountpoint.to_str().unwrap()])
            .output() {
            Ok(_) => (),
            Err(e) => println!("sudo umount error: {}", e),
        };
        match fs::remove_dir(&self.mountpoint) {
            Ok(_) => (),
            Err(_) => (),   // ignore any errors here, just cleanup
        };
    }
}

impl Drop for TarFsTest {
    fn drop(&mut self) {
        self.teardown_fs_mnt();
    }
}