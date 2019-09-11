use pretty_assertions;
use walkdir;

use std::process::Command;
use std::str;
use std::fs;
use std::path::PathBuf;

#[cfg(test)]
use pretty_assertions::{assert_eq};
use walkdir::WalkDir;

mod common;
use common::TarFsTest;

const HARDLINK_DST: &str = "hardlinkToa";
const HARDLINK_SRC: &str = "a";

fn ls_al(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new("ls")
            .args(&["-al", path])
            .output()?;
    Ok(str::from_utf8(&out.stdout)?.to_owned())
}

#[test]
#[ignore]
fn tarfs_ls() -> Result<(), Box<dyn std::error::Error>> {
    let test = TarFsTest::new("tests/ar.dir");
    // This does not work due to:
    //  1. dir sizes (fixable at all?)
    //  2. root dir permissions (TODO)
    test.perform(|mountpoint| {
        let actual = ls_al(mountpoint.to_str().unwrap())?;
        let expected = ls_al("tests/ar.dir")?;
        println!("actual: {}, expected: {}", actual, expected);
        assert_eq!(expected, actual);

        Ok(())
    })?;

    Ok(())
}

#[test]
fn tarfs_recursive_compare() -> Result<(), Box<dyn std::error::Error>> {
    let src_path = "tests/ar.dir";
    let test = TarFsTest::new(src_path);

    // Create hard link
    let mut src = PathBuf::from(src_path);
    src.push(HARDLINK_SRC);
    let mut dst = PathBuf::from(src_path);
    dst.push(HARDLINK_DST);

    if dst.exists() {
        fs::remove_file(&dst)?;
    }
    fs::hard_link(&src, &dst)?;

    test.perform(|mountpoint| {
        let path_cmp = |e1: &walkdir::DirEntry, e2: &walkdir::DirEntry| {
            e1.path().partial_cmp(e2.path()).unwrap_or(std::cmp::Ordering::Greater)
        };

        // Sort paths, start with root's children
        let mountpoint_str = mountpoint.to_str().unwrap();
        let mut expected_it = WalkDir::new("tests/ar.dir").sort_by(path_cmp).min_depth(1).into_iter();
        let mut actual_it = WalkDir::new(mountpoint_str).sort_by(path_cmp).min_depth(1).into_iter();

        loop {
            match (expected_it.next(), actual_it.next()) {
                (None, Some(actual)) => panic!("Found more entries than expected: {}", actual?.path().display()),
                (Some(expected), None) => panic!("Expected more entries: {}", expected?.path().display()),
                (Some(expected), Some(actual)) => {
                    let act_dir_entry = actual?;
                    let is_root_dir = act_dir_entry.path().to_str().unwrap() == mountpoint_str;
                    println!("{}", PathBuf::from(act_dir_entry.path()).as_path().display());

                    use std::os::unix::fs::MetadataExt; // Use unix specific trait methods
                    let exp_meta = fs::metadata(expected?.path())?;
                    let act_meta = fs::metadata(act_dir_entry.path())?;

                    // File types
                    assert_eq!(exp_meta.file_type().is_dir(), act_meta.file_type().is_dir(), "is dir");
                    assert_eq!(exp_meta.file_type().is_file(), act_meta.file_type().is_file(), "is file");
                    assert_eq!(exp_meta.file_type().is_symlink(), act_meta.file_type().is_symlink(), "is symlink");

                    // TODO hard links
                    // assert_eq!(exp_meta.nlink(), act_meta.nlink(), "nlink");

                    // Times
                    if !is_root_dir {
                        // These values can not be tested on root dir
                        assert_eq!(exp_meta.ctime(), act_meta.ctime(), "ctime");
                        assert_eq!(exp_meta.ctime_nsec(), act_meta.ctime_nsec(), "ctime nsecs");
                        assert_eq!(exp_meta.mtime(), act_meta.mtime(), "mtime secs");
                        assert_eq!(exp_meta.mtime_nsec(), act_meta.mtime_nsec(), "mtime nsecs");
                    }

                    // Size
                    if exp_meta.file_type().is_dir() {
                        // This is necessary because we cannot guarantee 100% matches here
                        assert!(act_meta.len() > 0, "len");
                    } else {
                        assert_eq!(exp_meta.len(), act_meta.len(), "len");
                    }

                    // Permissions
                    assert_eq!(exp_meta.uid(), act_meta.uid(), "uid");
                    assert_eq!(exp_meta.gid(), act_meta.gid(), "gid");
                    assert_eq!(exp_meta.permissions(), act_meta.permissions(), "permission");
                },
                (None, None) => break,  // Done
            };
        };

        Ok(())
    })?;

    Ok(())
}

#[test]
#[ignore]
fn tarfs_hard_link() -> Result<(), Box<dyn std::error::Error>> {
    let test = TarFsTest::new("tests/ar.dir");

    test.perform(|mountpoint| {
        let mut exp_link_path = PathBuf::from(mountpoint);
        exp_link_path.push(HARDLINK_SRC);
        let mut act_link_path = PathBuf::from(mountpoint);
        act_link_path.push(HARDLINK_DST);

        use std::os::unix::fs::MetadataExt;
        let exp_meta = fs::metadata(&exp_link_path)?;
        let act_meta = fs::metadata(&act_link_path)?;

        // hard links should return same ino as target file
        assert_eq!(exp_meta.ino(), act_meta.ino(), "ino");
        Ok(())
    })?;

    Ok(())
}
