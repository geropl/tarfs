use std::fs::File;
use std::io;
use std::{path::Path, path::PathBuf};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::cell::{RefCell};
use std::vec::Vec;
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use  std::collections::HashMap;

use time::Timespec;

use log;
use log::{info, error};

use crate::tarindex::{INode, TarIndex, TarIndexEntry};

/// This is a placeholder struct used by the TarIndexer to be able to create entries for not-yet-read tar entries
/// (in case children are read before their parents, for example)
#[derive(Debug)]
struct PathEntry {
    pub id: u64,
    pub children: Ptr<Vec<Rc<INode>>>,
    pub node: Option<Rc<INode>>,
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
    pub fn build_index_for<'f>(file: &'f File, options: &Options) -> Result<TarIndex<'f>, io::Error> {
        let now = Instant::now();
        info!("Starting indexing archive...");

        let mut archive: tar::Archive<&File> = tar::Archive::new(file);
        let mut index = TarIndex::new(file);

        // Start out with the root node representing the directory we get mounted to
        let mut inode_id = 1;
        let get = |id: &mut u64| -> u64 {
            let res = *id;
            *id += 1;
            res
        };

        let mut path_map: PathMap = BTreeMap::new();
        let root_node = TarIndexer::create_root_node(get(&mut inode_id), &options.root_permissions);
        let root_path = root_node.entry.path.to_owned();
        let root_pe = PathEntry {
            id: root_node.ino,
            children: root_node.children.clone(),
            node: Some(Rc::new(root_node)),
        };
        path_map.insert(root_path, ptr(root_pe));

        for (idx, entry) in archive.entries()?.enumerate() {
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

    fn create_root_node(ino: u64, root_permissions: &Permissions) -> INode {
        let now = SystemTime::now();
        let since_epoch = now.duration_since(UNIX_EPOCH).expect("SystemTime error");
        let now = Timespec::new(since_epoch.as_secs() as i64, since_epoch.subsec_nanos() as i32);
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
                mode: root_permissions.mode,
                uid: root_permissions.uid,
                gid: root_permissions.gid,
                mtime: now,
                atime: now,
                ctime: now,
                ftype: tar::EntryType::Directory,
            },
            parent_id: None,
            children: ptr(vec!()),
        }
    }

    fn entry_to_index_entry(index: u64, entry: &mut tar::Entry<'_, &File>) -> Result<TarIndexEntry, io::Error> {
        let link_name = entry.link_name()?.map(|l| l.to_path_buf());
        let exts = TarIndexer::collect_pax_extensions(entry)?;
        let header = entry.header();

        let hdr_mtime = Timespec::new(header.mtime()? as i64, 0);
        let mtime = TarIndexer::get_timespec_for(&exts, "mtime", &hdr_mtime);
        let atime = TarIndexer::get_timespec_for(&exts, "atime", &mtime);
        let ctime = TarIndexer::get_timespec_for(&exts, "ctime", &mtime);

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
            mtime,
            atime,
            ctime,
            ftype: header.entry_type(),
        })
    }

    fn collect_pax_extensions<'a>(entry: &'a mut tar::Entry<'_, &File>) -> Result<HashMap<String, String>, io::Error> {
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

    fn get_timespec_for(exts: &HashMap<String, String>, key: &str, fallback: &Timespec) -> Timespec {
        let mtime = TarIndexer::parse_timespec_from_pax_extension(&exts, key);
        return mtime.unwrap_or(*fallback);
    }

    fn parse_timespec_from_pax_extension(exts: &HashMap<String, String>, key: &str) -> Option<Timespec> {
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
                while ns / 100000000 == 0 {
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
