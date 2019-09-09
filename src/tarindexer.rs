use std::fs::File;
use std::io;
use std::{path::Path, path::PathBuf};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::cell::{RefCell};
use std::vec::Vec;
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use std::collections::HashMap;

use time::Timespec;

use tar::EntryType;
use fuse::FileType;

use log;
use log::{info, error};

use crate::tarindex::{TarIndex, IndexEntry, TarEntryPointer};

/// This is a placeholder struct used by the TarIndexer to be able to create entries for not-yet-read tar entries
/// (in case children are read before their parents, for example)
#[derive(Debug)]
struct PathEntry {
    pub id: u64,
    pub children: Ptr<Vec<Rc<IndexEntry>>>,
    pub index_entry: Option<Rc<IndexEntry>>,
}

/// Shorthand type
type Ptr<T> = Rc<RefCell<T>>;
fn ptr<T>(t: T) -> Ptr<T> {
    Rc::new(RefCell::new(t))
}

type PathMap = BTreeMap<PathBuf, Ptr<PathEntry>>;

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
    pub fn build_index_for<'f>(&self, file: &'f File, options: &Options) -> Result<TarIndex<'f>, io::Error> {
        let now = Instant::now();
        info!("Starting indexing archive...");

        let mut archive: tar::Archive<&File> = tar::Archive::new(file);
        let mut index = TarIndex::new(file);

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
        let root_pe = PathEntry {
            id: root_entry.ino,
            children: root_entry.children.clone(),
            index_entry: Some(Rc::new(root_entry)),
        };
        path_map.insert(root_path, ptr(root_pe));

        // Iterate tar entries
        for (idx, entry) in archive.entries()?.enumerate() {
            let tar_entry = self.entry_to_tar_entry(idx as u64, &mut entry?)?;

            // Find parent!
            let parent_path = tar_entry.path.parent().expect("a tar entry without parent component!");
            let parent_pe = self.get_or_create_path_entry(&mut path_map, parent_path, || {
                get(&mut inode_id)
            });

            // Entry already present?
            let path_entry = self.get_or_create_path_entry(&mut path_map, &tar_entry.path, || {
                get(&mut inode_id)
            });

            let ino = path_entry.borrow().id;
            let children = path_entry.borrow().children.clone();
            let mut pe = path_entry.borrow_mut();
            let pe_index_entry = &mut pe.index_entry;
            if pe_index_entry.is_some() {
                error!("Found double entry for path {}, quitting!", tar_entry.path.display());
                return Ok(index)    // TODO custom error type io::Error | IndexError
            }

            // Create IndexEntry
            let index_entry = tar_entry.to_index_entry(ino, Some(parent_pe.borrow().id), children);
            let rc_index_entry = Rc::new(index_entry);

            // Set index entry
            pe_index_entry.replace(rc_index_entry.clone());

            // Add itself to parents children
            parent_pe.borrow_mut().children.borrow_mut().push(rc_index_entry.clone());
        }

        // Actually insert entries into index
        for (_, path_entry) in path_map {
            let pe = path_entry.borrow();
            let index_entry = pe.index_entry.as_ref().expect(&format!("Found PathEntry without IndexEntry: {:?}", pe));
            index.insert(index_entry.clone());
        }

        info!("Done indexing archive. Took {}s.", now.elapsed().as_secs());
        Ok(index)
    }

    fn get_or_create_path_entry<F>(&self, path_map: &mut PathMap, path: &Path, mut get_ino: F) -> Ptr<PathEntry>
        where
            F: FnMut() -> u64 {
        match path_map.get(path) {
            None => {
                let pe = ptr(PathEntry {
                    id: get_ino(),
                    children: ptr(vec!()),
                    index_entry: None,
                });
                path_map.insert(path.to_owned(), pe.clone());
                (pe)
            },
            Some(pe) => pe.clone(),
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
        root_tar_entry.to_index_entry(ino, None, ptr(vec!()))
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

    // fn debug_print_pax_extension(ext: tar::PaxExtension) -> Result<(), Utf8Error> {
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
    fn to_index_entry(self, ino: u64, parent_ino: Option<u64>, children: Rc<RefCell<Vec<Rc<IndexEntry>>>>) -> IndexEntry {
        let attrs = self.attrs(ino);
        IndexEntry {
            ino,
            parent_ino,

            path: self.path,
            name: self.name,
            link_name: self.link_name,
            attrs,

            file_offsets: vec!(TarEntryPointer {
                raw_file_offset: self.raw_file_offset,
                filesize: self.filesize,
            }),

            children,
        }
    }

    fn attrs(&self, ino: u64) -> fuse::FileAttr {
        let kind = tar_entrytype_to_filetype(self.ftype);
        let size = match &self.link_name {
            // For symlinks, fuse/the kernel wants the length of the OsStr...
            Some(ln) => ln.as_os_str().len() as u64,
            None => match kind {
                fuse::FileType::Directory => 4096,    // We're mimicking ext4 here
                _ => self.filesize,       // The default case: Size "on disk" is the same as the size in the tar (uncompressed) archive
            },
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
            nlink: 1,
            uid: self.uid as u32,
            gid: self.gid as u32,
            rdev: 0,
            flags: 0,
        }
    }
}

fn tar_entrytype_to_filetype(ftype: tar::EntryType) -> fuse::FileType {
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
