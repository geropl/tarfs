
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;
use std::fs;
use std::thread;

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

        thread::spawn(move || {
            match tarfslib::setup_tar_mount(&filename, &mountpoint) {
                Ok(_) => (),
                Err(e) => println!("setup_tar_mount error: {}", e)
            }
        });

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