extern crate tarfslib;

use std::process::Command;
use std::str;

mod tarfstest;
use tarfstest::TarFsTest;


#[test]
#[ignore]
fn tarindex_ls() -> Result<(), Box<std::error::Error>> {
    let test = TarFsTest::new("ar.tar", "/tmp/mnt");

    test.perform(|mountpoint| {
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

        Ok(())
    })?;

    Ok(())
}