use std::fs::File;
use std::fmt;

use tar::Archive;

pub struct TarIndex {

}

impl fmt::Display for TarIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Index: \n{}", "asd")
    }
}

pub struct TarIndexer<'a> {
    file: &'a File
}

impl<'a> TarIndexer<'a> {
    pub fn new(file: &File) -> TarIndexer {
        TarIndexer{
            file
        }
    }

    pub fn index(&self) -> Result<TarIndex, std::io::Error> {
        let mut archive = Archive::new(self.file);
        let entries = archive.entries()?;
        for file in entries {
            let f = file.unwrap();
            // f.
        }
        Ok(TarIndex{})
    }
}
