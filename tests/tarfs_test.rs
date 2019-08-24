extern crate pretty_assertions;
extern crate walkdir;
extern crate tarfslib;

use std::process::Command;
use std::str;
use std::fs;
use std::path::PathBuf;

#[cfg(test)]
use pretty_assertions::{assert_eq};
use walkdir::WalkDir;

mod common;
use common::TarFsTest;

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
    let test = TarFsTest::new("tests/ar.dir");

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

                    let exp_meta = fs::metadata(expected?.path())?;
                    let act_meta = fs::metadata(act_dir_entry.path())?;
                    assert_eq!(exp_meta.file_type().is_dir(), act_meta.file_type().is_dir());
                    assert_eq!(exp_meta.file_type().is_file(), act_meta.file_type().is_file());
                    assert_eq!(exp_meta.file_type().is_symlink(), act_meta.file_type().is_symlink());

                    if !is_root_dir {
                        // Time values can not be tested on root dir
                        use std::os::unix::fs::MetadataExt; // Use unix time functions
                        assert_eq!(exp_meta.ctime(), act_meta.ctime());
                        assert_eq!(exp_meta.ctime_nsec(), act_meta.ctime_nsec());
                        assert_eq!(exp_meta.mtime(), act_meta.mtime());
                        assert_eq!(exp_meta.mtime_nsec(), act_meta.mtime_nsec());
                    }

                    if exp_meta.file_type().is_dir() {
                        // This is necessary because we cannot guarantee 100% matches here
                        assert!(act_meta.len() > 0);
                    } else {
                        assert_eq!(exp_meta.len(), act_meta.len());
                    }
                    assert_eq!(exp_meta.permissions(), act_meta.permissions());
                },
                (None, None) => break,  // Done
            };
        };

        Ok(())
    })?;

    Ok(())
}