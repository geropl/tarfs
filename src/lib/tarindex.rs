use std::fs::File;
use std::fmt;
use std::io;
use std::io::{Seek, SeekFrom, Read};
use std::{path::Path, path::PathBuf};
use std::option;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::cell::{RefCell};
use std::str::Utf8Error;
use std::vec::Vec;
use std::ffi::{OsStr};
use std::time::{SystemTime, UNIX_EPOCH, Instant};

use time::Timespec;

use log;
use log::{trace, info, error};

use fuse;

type ChildMap = BTreeMap<PathBuf, Rc<INode>>;
type INodeMap = BTreeMap<u64, Rc<INode>>;

/// This is the resulting index struct.
/// It holds a reference to the given archive file as it needs it to be open all time as it uses it not only to build the index but only to resolve content later.
pub struct TarIndex<'f> {
    /// The archive file. Used to create the tar::Archive and later used to read content.
    file: &'f File,

    /// The parsed tar::Archive. Iterated once to build up the index.
    archive: tar::Archive<&'f File>,

    /// Maps <ino>/<file_name> to the INode
    child_map: ChildMap,

    /// Maps ino to the INode
    inode_map: INodeMap,
}

impl<'f> TarIndex<'f> {
    pub fn new(file: &File) -> TarIndex {
        TarIndex {
            file: file,
            archive: tar::Archive::new(file),
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

    fn insert(&mut self, new_node: Rc<INode>) {
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

#[derive(Debug)]
pub struct INode {
    pub ino: u64,
    pub entry: TarIndexEntry,
    pub parent_id: Option<u64>,
    /// TODO Ideally, this would be Vec<Rc<INode>>, but IDK how to achieve this without unsafe (cmp. PathEntry below)
    ///
    pub children: Ptr<Vec<Rc<INode>>>,
}

impl INode {
    pub fn attrs(&self) -> fuse::FileAttr {
        let kind = tar_entrytype_to_filetype(self.entry.ftype);
        let mtime = Timespec::new(self.entry.mtime as i64, 0);
        let size = match &self.entry.link_name {
            // For symlinks, fuse wants the length of the OsStr...
            Some(ln) => ln.as_os_str().len() as u64,
            None => match kind {
                fuse::FileType::Directory => 4096,    // We're mimicking ext4 here
                _ => self.entry.filesize,       // The default case: Size "on disk" is the same as the size in the tar (uncompressed) archive
            },
        };
        fuse::FileAttr {
            ino: self.ino,
            size,
            blocks: 0,
            atime: mtime,
            mtime: mtime,
            ctime: mtime,
            crtime: mtime, // macOS only
            kind,
            perm: self.entry.mode as u16,
            nlink: 1,
            uid: self.entry.uid as u32,
            gid: self.entry.gid as u32,
            rdev: 0,
            flags: 0,
        }
    }
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

pub struct TarIndexer {}

/// This is a placeholder struct used by the TarIndexer to be able to create entries for not yet read tar entries
/// (in case children are read before their parents, for example)
#[derive(Debug)]
struct PathEntry {
    pub id: u64,
    pub children: Ptr<Vec<Rc<INode>>>,
    pub node: Option<Rc<INode>>,
}

type PathMap = BTreeMap<PathBuf, Ptr<PathEntry>>;

/// Shorthand type
type Ptr<T> = Rc<RefCell<T>>;
fn ptr<T>(t: T) -> Ptr<T> {
    Rc::new(RefCell::new(t))
}

impl TarIndexer {
    pub fn build_index_for(file: &File) -> Result<TarIndex, io::Error> {
        let now = Instant::now();
        info!("Starting indexing archive...");

        let mut index = TarIndex::new(file);

        // Start out with the root node representing the directory we get mounted to
        let mut inode_id = 1;
        let get = |id: &mut u64| -> u64 {
            let res = *id;
            *id += 1;
            res
        };

        let mut path_map: PathMap = BTreeMap::new();
        let root_node = TarIndexer::create_root_node(get(&mut inode_id));
        let root_path = root_node.entry.path.to_owned();
        let root_pe = PathEntry {
            id: root_node.ino,
            children: root_node.children.clone(),
            node: Some(Rc::new(root_node)),
        };
        path_map.insert(root_path, ptr(root_pe));

        for (idx, entry) in index.archive.entries()?.enumerate() {
            let index_entry = TarIndexer::entry_to_index_entry(idx as u64, &mut entry?)?;

            // Find parent!
            let parent_path = index_entry.path.parent().expect("a tar entry without parent component!");
            let parent_pe = TarIndexer::get_or_create_path_entry(&mut path_map, parent_path, || {
                get(&mut inode_id)
            });

            // Entry already present?
            let path_entry = TarIndexer::get_or_create_path_entry(&mut path_map, &index_entry.path, || {
                get(&mut inode_id)
            });

            let ino = path_entry.borrow().id;
            let children = path_entry.borrow().children.clone();
            let mut pe = path_entry.borrow_mut();
            let pe_node = &mut pe.node;
            if pe_node.is_some() {
                error!("Found double entry for path {}, quitting!", index_entry.path.display());
                return Ok(index)    // TODO custom error type io::Error | IndexError
            }

            // Create node
            let node = INode {
                ino,
                entry: index_entry,
                parent_id: Some(parent_pe.borrow().id),
                children,
            };
            let rc_node = Rc::new(node);

            // Set index entry
            pe_node.replace(rc_node.clone());

            // Add itself to parents children
            parent_pe.borrow_mut().children.borrow_mut().push(rc_node.clone());
        }

        // Actually insert entries into index
        for (_, path_entry) in path_map {
            let pe = path_entry.borrow();
            let node = pe.node.as_ref().expect(&format!("Found PathEntry without INode: {:?}", pe));
            index.insert(node.clone());
        }

        info!("Done indexing archive. Took {}s.", now.elapsed().as_secs());
        Ok(index)
    }

    fn get_or_create_path_entry<F>(path_map: &mut PathMap, path: &Path, mut get_ino: F) -> Ptr<PathEntry>
        where
            F: FnMut() -> u64 {
        match path_map.get(path) {
            None => {
                let pe = ptr(PathEntry {
                    id: get_ino(),
                    children: ptr(vec!()),
                    node: None,
                });
                path_map.insert(path.to_owned(), pe.clone());
                (pe)
            },
            Some(pe) => pe.clone(),
        }
    }

    fn create_root_node(ino: u64) -> INode {
        let start = SystemTime::now();
        let since_epoch = start.duration_since(UNIX_EPOCH).expect("SystemTime error");
        INode {
            ino,
            entry: TarIndexEntry {
                index: 0,
                header_offset: 0,
                raw_file_offset: 0,
                name: PathBuf::from("."),
                path: PathBuf::from("./"),
                link_name: None,
                filesize: 0,
                mode: 0o777,
                uid: 33333,
                gid: 33333,
                mtime: since_epoch.as_secs(),
                ftype: tar::EntryType::Directory,
            },
            parent_id: None,
            children: ptr(vec!()),
        }
    }

    fn entry_to_index_entry(index: u64, entry: &mut tar::Entry<'_, &File>) -> Result<TarIndexEntry, io::Error> {
        let link_name = entry.link_name()?.map(|l| l.to_path_buf());

        if let Some(exts) = entry.pax_extensions()? {
            for ext in exts {
                match TarIndexer::pax_extension(ext?) {
                    Err(_) => Err(io::Error::from(std::io::ErrorKind::InvalidData)),
                    Ok(_) => Ok(())
                }?;
            }
        }

        let header = entry.header();
        let path = PathBuf::from(entry.path()?);
        let name = PathBuf::from(path.as_path().file_name().expect("entry without name"));
        Ok(TarIndexEntry{
            index,
            header_offset: entry.raw_header_position(),
            raw_file_offset: entry.raw_file_position(),
            name,
            path,
            link_name,
            filesize: header.size()?,
            mode: header.mode()?,
            uid: header.uid()?,
            gid: header.gid()?,
            mtime: header.mtime()?,
            ftype: header.entry_type(),
        })
    }

    fn pax_extension(ext: tar::PaxExtension) -> Result<(), Utf8Error> {
        let k = ext.key()?;
        let v = ext.value()?;
        info!("{}: {}", k, v);

        Ok(())
    }
}

fn tar_entrytype_to_filetype(ftype: tar::EntryType) -> fuse::FileType {
    use fuse::FileType;
    use tar::EntryType;

    match ftype {
        EntryType::Regular => FileType::RegularFile,
        EntryType::Directory => FileType::Directory,
        EntryType::Symlink => FileType::Symlink,
        t => {
            println!("Unsupported EntryType: {:?}", t);
            FileType::RegularFile
        },
        // EntryType::Link => FileType::
    }
}
