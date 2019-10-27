use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::collections::BTreeMap;
use std::cell::{RefCell};
use std::rc::Rc;
use std::vec::Vec;
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use std::collections::HashMap;

use time::Timespec;

use tar::EntryType;
use fuse::FileType;

use failure::Error;
use super::TarFsError::IndexError;

use log;
use log::{info};

use crate::tarindex::{TarIndex, IndexEntry, TarEntryPointer};

/// Shorthand type
type Ptr<T> = Rc<RefCell<T>>;
fn ptr<T>(t: T) -> Ptr<T> {
    Rc::new(RefCell::new(t))
}

type PathMap<'e> = BTreeMap<PathBuf, Ptr<IndexEntry>>;

pub struct Options {
    pub root_permissions: Permissions,
}

pub struct Permissions {
    pub mode: u32,
    pub uid: u64,
    pub gid: u64,
}

pub struct TarIndexer {}

impl TarIndexer {
    pub fn build_index_for<'f>(&self, file: &'f File, options: &Options) -> Result<TarIndex<'f>, Error> {
        let now = Instant::now();
        info!("Starting indexing archive...");

        let mut archive: tar::Archive<&File> = tar::Archive::new(file);

        // Use sequential ino numbers
        let mut inode_id = 1;
        let get = |id: &mut u64| -> u64 {
            let res = *id;
            *id += 1;
            res
        };

        // Start with root_entry
        let mut path_map: PathMap = BTreeMap::new();
        let root_entry = self.create_root_entry(get(&mut inode_id), &options.root_permissions);
        let root_path = root_entry.path.to_owned();
        path_map.insert(root_path, ptr(root_entry));

        // Iterate tar entries
        for (idx, entry) in archive.entries()?.enumerate() {
            let tar_entry = self.entry_to_tar_entry(idx as u64, &mut entry?)?;
            //println!("{:?}", &tar_entry);

            // Find parent!
            let parent_path = tar_entry.path.parent().expect("a tar entry without parent component!");
            let (parent_ino, parent) = self.get_or_create_path_entry(&mut path_map, &PathBuf::from(parent_path), || get(&mut inode_id));

            // Entry already present?
            let (ino, index_entry) = self.get_or_create_path_entry(&mut path_map, &tar_entry.path, || get(&mut inode_id));

            // Create IndexEntry
            let is_hard_link = tar_entry.is_hard_link();
            tar_entry.set_to_index_entry(&mut index_entry.borrow_mut(), ino, Some(parent_ino));

            // Add itself to parents children
            parent.borrow_mut().children.push(index_entry.borrow().id);

            // Hard link? Bump nlink count for link_name
            if is_hard_link {
                let target_attrs = {
                    let index_entry_ref = &index_entry.borrow();
                    let link_name = &index_entry_ref.link_name;
                    if link_name.is_none() {
                        let err_msg = format!("Found link without link_name {}, quitting!", index_entry_ref.path.display());
                        return Err(IndexError { msg: err_msg }.into());
                    }
                    let (_, link_target) = self.get_or_create_path_entry(&mut path_map, &link_name.as_ref().unwrap(), || get(&mut inode_id));
                    let mut link_target_mut = link_target.borrow_mut();
                    link_target_mut.link_count += 1;
                    link_target_mut.attrs.nlink += 1;
                    link_target_mut.attrs.clone()
                };
                let mut index_entry_mut = index_entry.borrow_mut();
                index_entry_mut.link_target_ino = Some(target_attrs.ino);
                index_entry_mut.attrs = target_attrs;
            }
        }

        // Actually insert entries into index
        let mut index = TarIndex::new(file, path_map.len());

        // In order to get the IndexEntry out of Rc<RefCell<>> we have to:
        //  - get ownership of the Rc
        //  - to do so we have to remove() it from path_map
        //  - to do so for all entries we need a list of copies of all keys
        let keys: Vec<PathBuf> = path_map.iter()
            .map(|(k, _)| PathBuf::from(k))
            .collect();
        for k in keys {
            let index_entry_rc = path_map.remove(&k).unwrap();  // Impossible to have an entry without value here
            let id = index_entry_rc.borrow().id;
            let index_entry_res = Rc::try_unwrap(index_entry_rc);
            if let Err(_) = index_entry_res {
                return Err(IndexError {
                    msg: format!("Unexpected multiple link to index_entry {}, quitting!", id)
                }.into());
            }
            let index_entry_refc = index_entry_res.unwrap();
            index.insert(index_entry_refc.into_inner());
        }

        info!("Done indexing archive. Took {}s.", now.elapsed().as_secs());
        Ok(index)
    }

    fn get_or_create_path_entry<IdSource>(&self, path_map: &mut PathMap, path: &PathBuf, mut get_id: IdSource) -> (u64, Ptr<IndexEntry>)
        where
            IdSource: FnMut() -> u64 {
        match path_map.get(path) {
            None => {
                let id = get_id();
                let mut entry = IndexEntry::default();
                entry.id = id;
                let entry_ptr = ptr(entry);
                path_map.insert(path.to_owned(), entry_ptr.clone());
                (id, entry_ptr)
            },
            Some(entry) => {
                let id = entry.borrow().id;
                (id, entry.clone())
            },
        }
    }

    fn create_root_entry(&self, ino: u64, root_permissions: &Permissions) -> IndexEntry {
        let now = SystemTime::now();
        let since_epoch = now.duration_since(UNIX_EPOCH).expect("SystemTime error");
        let now = Timespec::new(since_epoch.as_secs() as i64, since_epoch.subsec_nanos() as i32);

        let root_tar_entry = TarEntry {
            index: 0,
            header_offset: 0,
            raw_file_offset: 0,
            name: PathBuf::from("."),
            path: PathBuf::from("./"),
            link_name: None,
            filesize: 0,
            mode: root_permissions.mode,
            uid: root_permissions.uid,
            gid: root_permissions.gid,
            mtime: now,
            atime: now,
            ctime: now,
            ftype: tar::EntryType::Directory,
        };
        let mut root_entry = IndexEntry::default();
        root_tar_entry.set_to_index_entry(&mut root_entry, ino, None);
        root_entry
    }

    fn entry_to_tar_entry(&self, index: u64, entry: &mut tar::Entry<'_, &File>) -> Result<TarEntry, io::Error> {
        let link_name = entry.link_name()?.map(|l| l.to_path_buf());
        let exts = self.collect_pax_extensions(entry)?;
        let header = entry.header();

        let hdr_mtime = Timespec::new(header.mtime()? as i64, 0);
        let mtime = self.get_timespec_for(&exts, "mtime", &hdr_mtime);
        let atime = self.get_timespec_for(&exts, "atime", &mtime);
        let ctime = self.get_timespec_for(&exts, "ctime", &mtime);

        let path = PathBuf::from(entry.path()?);
        let name = PathBuf::from(path.as_path().file_name().expect("entry without name"));

        Ok(TarEntry{
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
            mtime,
            atime,
            ctime,
            ftype: header.entry_type(),
        })
    }

    fn collect_pax_extensions<'a>(&self, entry: &'a mut tar::Entry<'_, &File>) -> Result<HashMap<String, String>, io::Error> {
        let mut result = HashMap::new();
        let exts = match entry.pax_extensions() {
            Err(e) => return Err(e),
            Ok(None) => return Ok(result),
            Ok(Some(exts)) => exts,
        };
        for ext in exts {
            let ext = match ext {
                Err(_) => continue,
                Ok(ext) => ext,
            };
            let key = ext.key();
            if key.is_err() {
                continue;
            }
            let key: &str = key.unwrap();
            let value: &str = ext.value().unwrap_or("");
            result.insert(key.to_owned(), value.to_owned());

            // let r = TarIndexer::debug_print_pax_extension(ext);
            // if let Err(_e) = r {
            //     continue;
            // }
        }
        Ok(result)
    }

    fn get_timespec_for(&self, exts: &HashMap<String, String>, key: &str, fallback: &Timespec) -> Timespec {
        let mtime = self.parse_timespec_from_pax_extension(&exts, key);
        return mtime.unwrap_or(*fallback);
    }

    fn parse_timespec_from_pax_extension(&self, exts: &HashMap<String, String>, key: &str) -> Option<Timespec> {
        let value = exts.get(key);
        if value.is_none() {
            return None;
        }

        use std::num::ParseIntError;
        type ParsedInt = Result<i64, ParseIntError>;

        let splits: Vec<&str> = value.unwrap().split('.').collect();
        let splits_parsed: Vec<ParsedInt> = splits.iter().map(|&s| s.parse::<i64>()).collect();
        let splits_parsed_ref: &[ParsedInt] = &splits_parsed;
        match splits_parsed_ref {
            [Ok(s), Ok(ns)] => {
                let mut ns = *ns as i32;
                // tar seems to eat trailing zeros here.
                // To exactlly mimick the source stats,
                // adjust the exact amount of trailing zeros for nanoseconds
                // Ex1:    27993590
                // Tar1:   2799359
                while ns / 10000000 == 0 {
                    ns = ns * 10;
                }
                Some(Timespec::new(*s, ns))
            },
            [Ok(s)] => Some(Timespec::new(*s, 0)),
            _ => return None,
        }
    }

    // fn debug_print_pax_extension(ext: tar::PaxExtension) -> Result<(), std::str::Utf8Error> {
    //     let k = ext.key()?;
    //     let v = ext.value()?;
    //     println!("key: {} | value: {}", k, v);

    //     Ok(())
    // }
}

#[derive(Debug)]
struct TarEntry {
    index: u64,
    header_offset: u64,
    raw_file_offset: u64,
    name: PathBuf,
    path: PathBuf,
    link_name: Option<PathBuf>,
    filesize: u64,
    mode: u32,
    uid: u64,
    gid: u64,
    mtime: Timespec,
    atime: Timespec,
    ctime: Timespec,
    ftype: tar::EntryType,
}

impl TarEntry {
    fn set_to_index_entry(self, entry: &mut IndexEntry, id: u64, parent_ino: Option<u64>) -> () {
        entry.id = id;
        entry.parent_ino = parent_ino;
        entry.attrs = self.attrs(id);
        entry.path = self.path;
        entry.name = self.name;
        entry.link_name = self.link_name;
        entry.file_offsets.push(TarEntryPointer {
            raw_file_offset: self.raw_file_offset,
            filesize: self.filesize,
        });
    }

    fn is_hard_link(&self) -> bool {
        self.ftype == tar::EntryType::Link
    }

    fn attrs(&self, ino: u64) -> fuse::FileAttr {
        let kind = match self.ftype {
            EntryType::Regular => FileType::RegularFile,
            EntryType::Directory => FileType::Directory,
            EntryType::Symlink => FileType::Symlink,
            EntryType::Link => FileType::RegularFile,
            t => {
                println!("Unsupported EntryType: {:?}", t);
                FileType::RegularFile
            },
        };

        let size = match &self.link_name {
            // For symlinks, fuse/the kernel wants the length of the OsStr...
            Some(ln) => ln.as_os_str().len() as u64,
            None => match self.ftype {
                tar::EntryType::Link => 0,  // hard link
                tar::EntryType::Directory => 4096,    // We're mimicking ext4 here
                _ => self.filesize,       // The default case: Size "on disk" is the same as the size in the tar (uncompressed) archive
            },
        };

        let nlink = match &self.ftype {
            tar::EntryType::Directory => 2,
            _ => 1,
        };

        fuse::FileAttr {
            ino,
            size,
            blocks: 0,
            atime: self.atime,
            mtime: self.mtime,
            ctime: self.ctime,
            crtime: self.ctime, // macOS only
            kind,
            perm: self.mode as u16,
            nlink,
            uid: self.uid as u32,
            gid: self.gid as u32,
            rdev: 0,
            flags: 0,
        }
    }
}
