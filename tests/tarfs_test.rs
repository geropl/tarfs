extern crate tarfslib;

use std::path::Path;
use std::thread;
use std::process::Command;
use std::str;
use std::fs;

#[test]
#[ignore]
fn tarindex_ls() -> Result<(), Box<std::error::Error>> {
    let filename = Path::new("tests/ar.tar");
    let mountpoint = Path::new("tests/mnt");

    thread::spawn(move || {
        tarfslib::setup_tar_mount(filename, mountpoint).unwrap();
    });

    let out = Command::new("ls")
            .args(&["-al", mountpoint.to_str().unwrap()])
            .output()?;
    let ls_str = str::from_utf8(&out.stdout)?;
    let actual = format!("{}", ls_str);
    let expected =
"total 4
drwxrwxrwx 0 gitpod gitpod    0 Jul 29 19:47 .
drwxr-xr-x 3 gitpod gitpod 4096 Jul 29 19:47 ..
-rwxrwxrwx 0 gitpod gitpod    0 Jul 29 19:47 a
-rwxrwxrwx 0 gitpod gitpod    0 Jul 29 19:47 b
drwxrwxrwx 0 gitpod gitpod    0 Jul 29 19:47 dir1
drwxrwxrwx 0 gitpod gitpod    0 Jul 29 19:47 dir2";
    assert_eq!(expected, actual);

    // Stop by unmounting
    match Command::new("sudo")
        .args(&["umount", mountpoint.to_str().unwrap()])
        .output() {
        Ok(_) => (),
        Err(e) => println!("{}", e),
    };
    match fs::remove_dir(&mountpoint) {
        Ok(_) => (),
        Err(e) => println!("{}", e),
    };

    Ok(())
}