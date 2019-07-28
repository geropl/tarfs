use std::fs::File;
use std::fmt;
use std::io;
use std::path::{PathBuf};
use std::option;
use std::collections::BTreeMap;

use tar::{Archive};

pub struct TarIndex<'f> {
    _file: &'f File,
    archive: tar::Archive<&'f File>,
    map: BTreeMap<String, TarIndexEntry>
}

impl<'f> TarIndex<'f> {
    pub fn new_from(file: &File) -> Result<TarIndex, io::Error> {
        let mut index = TarIndex {
            _file: file,
            archive: Archive::new(file),
            map: BTreeMap::new()
        };
        TarIndex::scan(&mut index.archive, &mut index.map)?;
        Ok(index)
    }

    fn scan(archive: &mut tar::Archive<&File>, map: &mut BTreeMap<String, TarIndexEntry>) -> Result<(), io::Error> {
        for entry in archive.entries()? {
            let index_entry = TarIndex::entry_to_index_entry(entry?)?;
            TarIndex::insert_into(map, index_entry);
        }
        Ok(())
    }

    fn insert_into(map: &mut BTreeMap<String, TarIndexEntry>, new_entry: TarIndexEntry) {
        let s = String::from(new_entry.path.to_str().unwrap());
        map.insert(s, new_entry);
    }

    fn entry_to_index_entry(entry: tar::Entry<'_, &File>) -> Result<TarIndexEntry, io::Error> {
        let link_name = entry.link_name()?.map(|l| l.to_path_buf());
        Ok(TarIndexEntry{
            header_offset: entry.raw_header_position(),
            raw_file_offset: entry.raw_file_position(),
            path: PathBuf::from(entry.path()?),
            link_name,
        })
    }
}

impl fmt::Display for TarIndex<'_> {
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
