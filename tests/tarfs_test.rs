extern crate pretty_assertions;
extern crate tarfslib;

use std::process::Command;
use std::str;

#[cfg(test)]
use pretty_assertions::{assert_eq};

mod common;
use common::TarFsTest;

fn ls_al(path: &str) -> Result<String, Box<std::error::Error>> {
    let out = Command::new("ls")
            .args(&["-al", path])
            .output()?;
    Ok(str::from_utf8(&out.stdout)?.to_owned())
}

#[test]
#[ignore]
fn tarindex_ls() -> Result<(), Box<std::error::Error>> {
    let test = TarFsTest::new("tests/ar.tar", ".tmp/mnt");

    test.perform(|mountpoint| {
        let actual = ls_al(mountpoint.to_str().unwrap())?;
        let expected = ls_al("tests/ar.dir")?;
        println!("actual: {}, expected: {}", actual, expected);
        assert_eq!(expected, actual);

        Ok(())
    })?;

    Ok(())
}