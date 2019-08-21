# tarfs

A readonly FUSE filesystem that allows to mount tar files

## Usage
```
tarfs 1.0
Gero Posmyk-Leinemann <geroleinemann@gmx.de>
A readonly FUSE filesystem that allows to mount tar files

USAGE:
    tarfs <archive> <mountpoint>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <archive>       The tar file that should be mounted
    <mountpoint>    The path to the directory where the archive should be mounted
```

## Why?

Sometimes it's useful to be able to mount a tar file directly without the need to extract it which takes time and disk space.
Also, it's fun - and a great way to learn more Rust.

## How

It scans the tar archive once, builds up an index and later uses that information to respond to FUSE requests like `get_attrs` or `read`.

## Install
TODO

## Development

```Rust
 cargo test
 cargo build
```

 [![Open in Gitpod](https://gitpod.io/button/open-in-gitpod.svg)](https://gitpod.io/#https://github.com/geropl/tarfs)

### TODOS
 - [] hard links
 - [] optimize reads by using a buffered reader
 - [] try to create static binary by replacing relevant parts in https://github.com/zargony/rust-fuse
 - ...