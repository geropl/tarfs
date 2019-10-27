use std::fs::File;
use std::fmt;
use std::io;
use std::io::{Seek, SeekFrom, Read};
use std::{path::Path, path::PathBuf};
use std::collections::BTreeMap;
use std::vec::Vec;
use std::ffi::{OsStr};

use log::{trace, error};

use crate::utils::default_fuse_file_attr;
use crate::arena::{ Arena, ChildrenIterator };

#[derive(Debug, Clone)]
pub struct IndexEntry {
    // Ids start from 1
    // It is equivalent with ino() except if this is a hard link
    pub id: u64,
    pub parent_ino: Option<u64>,

    pub path: PathBuf,
    pub name: PathBuf,
    pub link_name: Option<PathBuf>,
    pub link_count: u64,    // TODO Needed? What for?
    pub link_target_ino: Option<u64>,
    pub attrs: fuse::FileAttr,

    pub file_offsets: Vec<TarEntryPointer>,

    pub children: Vec<u64>,
}

impl IndexEntry {
    pub fn ino(&self) -> u64 {
        match self.link_target_ino {
            Some(lt_ino) => lt_ino, // This is a hard link!
            None => self.id,
        }
    }
}

impl Default for IndexEntry {
    fn default() -> Self {
        IndexEntry {
            id: 0,
            parent_ino: None,

            path: PathBuf::from(""),
            name: PathBuf::from(""),
            link_name: None,
            link_count: 0,
            link_target_ino: None,
            attrs: default_fuse_file_attr(),

            file_offsets: vec!(),
            children: vec!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TarEntryPointer {
    pub raw_file_offset: u64,
    pub filesize: u64,
}

type ChildMap = BTreeMap<PathBuf, u64>;
type INodeMap = BTreeMap<u64, usize>;

/// This is the resulting index struct.
/// It holds a reference to the given archive file as it needs it to be open all time as it uses it not only to build the index but only to resolve content later.
#[derive(Debug)]
pub struct TarIndex<'f> {
    /// The archive file. Used to create the tar::Archive and later used to read content.
    file: &'f File,

    arena: Arena<IndexEntry>,

    /// Maps <ino>/<file_name> to the INode
    child_map: ChildMap,

    /// Maps <ino> to the IndexEntry
    /// TODO Could be replaced by ino_to_arena_index now...
    /// Keep for now, maybe someone has an idea to replace the arena by "real" references
    ino_map: INodeMap,
}

impl<'f> TarIndex<'f> {
    pub fn new(file: &File, initial_capacity: usize) -> TarIndex {
        TarIndex {
            file: file,
            arena: Arena::with_capacity(initial_capacity),
            child_map: BTreeMap::new(),
            ino_map: BTreeMap::new(),
        }
    }

    pub fn get_entry_by_ino(&self, ino: u64) -> Option<&IndexEntry> {
        match self.ino_map.get(&ino) {
            None => None,
            Some(arena_index) => self.arena.get(*arena_index),
        }
    }

    pub fn lookup_child(&self, parent_ino: u64, path: PathBuf) -> Option<&IndexEntry> {
        let key = lookup_key(parent_ino, path.as_os_str());
        match self.child_map.get(&key) {
            None => None,
            Some(ino) => {
                let arena_index = ino_to_arena_index(*ino);
                self.arena.get(arena_index)
            },
        }
    }

    pub fn read(&mut self, entry: &IndexEntry, offset: u64, size: u64) -> Result<Vec<u8>, io::Error> {
        // TODO Support sparse tar files
        let part1 = &entry.file_offsets[0];

        let offset_in_file = part1.raw_file_offset + (offset as u64);
        let file_end = part1.raw_file_offset + part1.filesize;
        let left = file_end - offset_in_file;
        trace!("offset {}, size {}, off_f {}, file_end {}, left {}", offset, size, offset_in_file, file_end, left);

        self.file.seek(SeekFrom::Start(offset_in_file))?;

        if left < size {
            let mut buf = vec![0; left as usize];
            self.file.read_exact(&mut buf)?;
            buf.append(&mut vec![0; (size - left) as usize]);
            Ok(buf)
        } else {
            let mut buf = vec![0; size as usize];
            self.file.read_exact(&mut buf)?;
            Ok(buf)
        }
    }

    pub fn insert(&mut self, new_entry: IndexEntry) {
        let (arena_index, new_entry) = self.arena.insert(new_entry, |e| ino_to_arena_index(e.id));
        let ino = new_entry.id;
        if let Some(parent_id) = new_entry.parent_ino {
            let path = new_entry.path.as_path();
            let filename = match path.file_name() {
                Some(n) => n,
                None => {
                    error!("Unable to get file name from: {}", path.display());
                    return
                }
            };
            let key = lookup_key(parent_id, filename);
            self.child_map.insert(key, ino);
        }
        self.ino_map.insert(ino, arena_index);
    }

    pub fn children_iter<'e>(&'e self, entry: &'e IndexEntry) -> ChildrenIterator<'e, IndexEntry> {
        ChildrenIterator::new(&self.arena, &entry.children)
    }
}

fn lookup_key(id: u64, filename: &OsStr) -> PathBuf {
    let mut key = PathBuf::new();
    key.push(Path::new(&format!("{}/", id)));
    key.push(filename);
    key
}

fn ino_to_arena_index(ino: u64) -> usize {
    (ino - 1) as usize      // Compensate the fact that inos start with 1
}

impl fmt::Display for TarIndex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut content = String::new();
        for (_, node) in self.ino_map.iter() {
            content.push_str(&format!("{:?}", node));
        }
        write!(f, "Index: \n{{{}\n}}", content)
    }
}
