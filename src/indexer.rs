use std::fs::File;
use std::fmt;
use std::io;
use std::path::{PathBuf};
use std::option;
use std::collections::BTreeMap;

use tar::{Archive};

pub struct TarIndex {
    map: BTreeMap<String, Box<TarIndexEntry>>
}

impl TarIndex {
    fn new() -> TarIndex {
        TarIndex {
            map: BTreeMap::new()
        }
    }

    fn insert(&mut self, new_entry: Box<TarIndexEntry>) {
        let s = String::from(new_entry.path.to_str().unwrap());
        self.map.insert(s, new_entry);
    }
}

impl fmt::Display for TarIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut content = String::new();
        for entry in self.map.iter() {
            content.push_str(&format!("{}", entry.1));
        }
        write!(f, "Index: \n{{{}\n}}", content)
    }
}

pub struct TarIndexEntry {
    pub header_offset: u64,
    pub raw_file_offset: u64,
    pub path: PathBuf,
    pub link_name: option::Option<PathBuf>,
}

impl fmt::Display for TarIndexEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Entry: \n{{ header: {}, file: {}, path: {:?}, link_name: {:?} }}", self.header_offset, self.raw_file_offset, self.path, self.link_name)
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

    pub fn index(&self) -> Result<TarIndex, io::Error> {
        let mut archive = Archive::new(self.file);
        let mut index = TarIndex::new();
        for entry in archive.entries()? {
            let e = entry.unwrap();
            let index_entry = TarIndexer::entry_to_index_entry(&e)?;
            index.insert(index_entry);
        }
        Ok(index)
    }

    fn entry_to_index_entry(entry: &tar::Entry<'_, &std::fs::File>) -> Result<Box<TarIndexEntry>, io::Error> {
        let path = entry.path()?;
        let link_name = match entry.link_name() {
            Err(e) => {
                println!("f.link_name(): {}", e);
                None
            },
            Ok(p) => match p {
                None => None,
                Some(l) => Some(l.to_path_buf())
            },
        };
        Ok(Box::new(TarIndexEntry{
            header_offset: entry.raw_header_position(),
            raw_file_offset: entry.raw_file_position(),
            path: path.to_path_buf(),
            link_name
        }))
    }
}
