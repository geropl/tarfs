use std::fs::File;
use std::fmt;
use std::io;
use std::{path, path::Path, path::PathBuf};
use std::option;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::str::Utf8Error;
use tar::{Archive};

type PathMap = BTreeMap<PathBuf, Rc<TarIndexEntry>>;
type IndexMap = BTreeMap<u64, Rc<TarIndexEntry>>;

pub struct TarIndex<'f> {
    _file: &'f File,
    archive: tar::Archive<&'f File>,
    map: PathMap,
    index_map: IndexMap,
}

impl<'f> TarIndex<'f> {
    pub fn new_from(file: &File) -> Result<TarIndex, io::Error> {
        let mut index = TarIndex {
            _file: file,
            archive: Archive::new(file),
            map: BTreeMap::new(),
            index_map: BTreeMap::new(),
        };
        scan(&mut index.archive, &mut index.map, &mut index.index_map)?;
        Ok(index)
    }

    pub fn get_entry_by_index(&self, index: u64) -> Option<&Rc<TarIndexEntry>> {
        self.index_map.get(&index)
    }

    pub fn get_entry_by_path(&self, path: &PathBuf) -> Option<&Rc<TarIndexEntry>> {
        self.map.get(path)
    }

    pub fn get_entries_by_path_prefix(&self, prefix: &Path) -> Vec<&Rc<TarIndexEntry>> {
        println!("Prefix: {}", prefix.display());
        self.map.values().filter(|val| {
            let p = val.path.as_path();
            let res = match p.strip_prefix(prefix) {
                Err(_) => false,
                Ok(base) => {
                    let s = String::from(base.to_str().unwrap());
                    match s.find(path::MAIN_SEPARATOR) {
                        Some(_) => false,
                        None => true
                    }
                }
            };
            // println!("p: {}, {}", p.display(), res);
            res
        }).collect()
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
    pub index: u64,
    pub header_offset: u64,
    pub raw_file_offset: u64,
    pub path: PathBuf,
    pub link_name: option::Option<PathBuf>,
    pub filesize: u64,
    pub mode: u32,
    pub uid: u64,
    pub gid: u64,
    pub mtime: u64,
    pub entrytype: tar::EntryType,
}

impl TarIndexEntry {
}

impl fmt::Display for TarIndexEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Entry: \n{{ index: {}, header: {}, file: {}, path: {}, link_name: {:?} }}", self.index, self.header_offset, self.raw_file_offset, self.path.display(), self.link_name)
    }
}

fn scan(archive: &mut tar::Archive<&File>, path_map: &mut PathMap, index_map: &mut IndexMap) -> Result<(), io::Error> {
    for (idx, entry) in archive.entries()?.enumerate() {
        let index_entry = entry_to_index_entry(idx as u64, &mut entry?)?;
        insert_into(path_map, index_map, Rc::new(index_entry));
    }
    Ok(())
}

fn insert_into(map: &mut PathMap, index_map: &mut IndexMap, new_entry: Rc<TarIndexEntry>) {
    let path = &new_entry.path;
    let index = new_entry.index;
    map.insert(path.to_path_buf(), new_entry.clone());
    index_map.insert(index, new_entry);
}

fn entry_to_index_entry(index: u64, entry: &mut tar::Entry<'_, &File>) -> Result<TarIndexEntry, io::Error> {
    let link_name = entry.link_name()?.map(|l| l.to_path_buf());

    if let Some(exts) = entry.pax_extensions()? {
        for ext in exts {
            match pax_extension(ext?) {
                Err(_) => Err(io::Error::from(std::io::ErrorKind::InvalidData)),
                Ok(_) => Ok(())
            }?;
        }
    }

    let header = entry.header();

    Ok(TarIndexEntry{
        index,
        header_offset: entry.raw_header_position(),
        raw_file_offset: entry.raw_file_position(),
        path: PathBuf::from(entry.path()?),
        link_name,
        filesize: header.size()?,
        mode: header.mode()?,
        uid: header.uid()?,
        gid: header.gid()?,
        mtime: header.mtime()?,
        entrytype: header.entry_type(),
    })
}

fn pax_extension(ext: tar::PaxExtension) -> Result<(), Utf8Error> {
    let k = ext.key()?;
    let v = ext.value()?;
    println!("{}: {}", k, v);

    Ok(())
}
