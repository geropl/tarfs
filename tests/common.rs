use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;
use std::fs;
use std::thread;
use std::sync::mpsc::sync_channel;

const TEST_ROOT: &str = "/workspace/tarfs/.test";
const TEST_MOUNTPOINT_SUBDIR: &str = "mnt";

type TarFsTestResult = Result<(), Box<dyn std::error::Error>>;

pub struct TarFsTest {
    source_path: PathBuf,
    mountpoint: PathBuf
}

impl TarFsTest {
    pub fn new(source_path: &str) -> TarFsTest {
        let mut mountpoint = PathBuf::from(TEST_ROOT);
        mountpoint.push(TEST_MOUNTPOINT_SUBDIR);
        TarFsTest {
            source_path: PathBuf::from(source_path),
            mountpoint: mountpoint,
        }
    }

    pub fn perform(&self, test: fn(&Path) -> TarFsTestResult) -> TarFsTestResult {
        let archive_path = self.create_test_tar()?;
        self.setup_fs_mnt(&archive_path)?;

        test(&self.mountpoint)?;

        Ok(())
    }

    fn create_test_tar(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let mut archive_path = PathBuf::from(TEST_ROOT);
        let mut archive_filename = self.source_path.file_name().unwrap().to_os_string();
        archive_filename.push(".tar");
        archive_path.push(&archive_filename);

        let archive_parent = archive_path.parent().unwrap();
        if !archive_parent.exists() {
            fs::create_dir_all(&archive_parent)?;
        }

        match Command::new("bash")
            // posix format is needed for nanosecond precision for timestamps
            .args(&["-c", &format!("tar cf {} -H posix ./*", archive_path.to_str().unwrap())])
            .current_dir(&self.source_path)
            .output() {
            Ok(out) => {
                if !out.status.success() {
                    println!("stderr: {}", std::str::from_utf8(&out.stderr).unwrap());
                    println!("stdout: {}", std::str::from_utf8(&out.stdout).unwrap());
                }
                Ok(archive_path)
            },
            Err(e) => {
                println!("bash -c \"tar cf ... \" error: {}", e);
                Err(Box::new(e))
            },
        }
    }

    fn setup_fs_mnt(&self, archive_path: &Path) -> TarFsTestResult {
        let archive_path = PathBuf::from(archive_path);
        let mountpoint = self.mountpoint.clone();

        // Clean state
        if mountpoint.exists() {
            fs::remove_dir(&mountpoint)?;
        }
        fs::create_dir_all(&mountpoint)?;

        let (tx, rx) = sync_channel(1);
        thread::spawn(move || {
            match tarfslib::setup_tar_mount(&archive_path, &mountpoint, Some(tx)) {
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
        match fs::remove_dir_all(TEST_ROOT) {
            Ok(_) => (),
            Err(e) => println!("error during cleanup: {}", e),
        };
    }
}

impl Drop for TarFsTest {
    fn drop(&mut self) {
        self.teardown_fs_mnt();
    }
}