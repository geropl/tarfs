use fuse;
use time::Timespec;

pub fn default_fuse_file_attr() -> fuse::FileAttr {
    fuse::FileAttr {
        ino: 0,
        size: 0,
        blocks: 0,
        atime: Timespec::new(0, 0),
        mtime: Timespec::new(0, 0),
        ctime: Timespec::new(0, 0),
        crtime: Timespec::new(0, 0),
        kind: fuse::FileType::RegularFile,
        perm: 0,
        nlink: 0,
        uid: 0,
        gid: 0,
        rdev: 0,
        flags: 0,
    }
}