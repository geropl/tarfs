use std::fs::File;
use std::fmt;
use std::io;
use std::io::{Seek, SeekFrom, Read};
use std::{path::Path, path::PathBuf};
use std::option;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::cell::{RefCell};
use std::vec::Vec;
use std::ffi::{OsStr};

use log::{trace, error};

#[derive(Debug)]
pub struct INode {
    pub ino: u64,
    pub entry: TarIndexEntry,
    pub parent_id: Option<u64>,
    /// TODO Ideally, this would be Vec<Rc<INode>>, but IDK how to achieve this without unsafe (cmp. PathEntry below)
    ///
    pub children: Rc<RefCell<Vec<Rc<INode>>>>,
}

#[derive(Debug)]
pub struct TarIndexEntry {
    pub index: u64,
    pub header_offset: u64,
    pub raw_file_offset: u64,
    pub name: PathBuf,
    pub path: PathBuf,
    pub link_name: option::Option<PathBuf>,
    pub filesize: u64,
    pub mode: u32,
    pub uid: u64,
    pub gid: u64,
    pub mtime: u64,
    pub ftype: tar::EntryType,
}

type ChildMap = BTreeMap<PathBuf, Rc<INode>>;
type INodeMap = BTreeMap<u64, Rc<INode>>;

/// This is the resulting index struct.
/// It holds a reference to the given archive file as it needs it to be open all time as it uses it not only to build the index but only to resolve content later.
pub struct TarIndex<'f> {
    /// The archive file. Used to create the tar::Archive and later used to read content.
    file: &'f File,

    /// Maps <ino>/<file_name> to the INode
    child_map: ChildMap,

    /// Maps ino to the INode
    inode_map: INodeMap,
}

impl<'f> TarIndex<'f> {
    pub fn new(file: &File) -> TarIndex {
        TarIndex {
            file: file,
            child_map: BTreeMap::new(),
            inode_map: BTreeMap::new(),
        }
    }

    pub fn get_node_by_ino(&self, ino: u64) -> Option<&Rc<INode>> {
        self.inode_map.get(&ino)
    }

    pub fn lookup_child(&self, parent_ino: u64, path: PathBuf) -> Option<&Rc<INode>> {
        let key = self.lookup_key(parent_ino, path.as_os_str());
        self.child_map.get(&key)
    }

    pub fn read(&mut self, node: &INode, offset: u64, size: u64) -> Result<Vec<u8>, io::Error> {
        let offset_in_file = node.entry.raw_file_offset + (offset as u64);
        let file_end = node.entry.raw_file_offset + node.entry.filesize;
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

    pub fn insert(&mut self, new_node: Rc<INode>) {
        let ino = new_node.ino;
        if let Some(parent_id) = new_node.parent_id {
            let path = new_node.entry.path.as_path();
            let filename = match path.file_name() {
                Some(n) => n,
                None => {
                    error!("Unable to get file name from: {}", path.display());
                    return
                }
            };
            let key = self.lookup_key(parent_id, filename);
            self.child_map.insert(key, new_node.clone());
        }
        self.inode_map.insert(ino, new_node);
    }

    fn lookup_key(&self, id: u64, filename: &OsStr) -> PathBuf {
        let mut key = PathBuf::new();
        key.push(Path::new(&format!("{}/", id)));
        key.push(filename);
        key
    }
}

impl fmt::Display for TarIndex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut content = String::new();
        for (_, node) in self.inode_map.iter() {
            content.push_str(&format!("{:?}", node));
        }
        write!(f, "Index: \n{{{}\n}}", content)
    }
}
