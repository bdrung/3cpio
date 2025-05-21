// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::fs::{symlink_metadata, Metadata};
use std::io::{Error, ErrorKind, Result};
use std::os::unix::fs::MetadataExt;

use crate::filetype::*;
use crate::libc::{major, minor};

#[derive(Debug, PartialEq)]
enum Filetype {
    RegularFile { location: String, filesize: u32 },
    EmptyFile,
    Directory,
    BlockDevice { major: u32, minor: u32 },
    CharacterDevice { major: u32, minor: u32 },
    Fifo,
    Socket,
    Symlink { target: String },
}

#[derive(Debug, PartialEq)]
pub struct File {
    filetype: Filetype,
    name: String,
    mode: u16,
    uid: u32,
    gid: u32,
    mtime: u32,
}

#[derive(Debug, PartialEq)]
pub struct Manifest {
    input_path: Option<String>,
    path: String,
}

struct LazyMetadata<'a> {
    location: Option<&'a str>,
    metadata: Option<Metadata>,
}

impl<'a> LazyMetadata<'a> {
    fn new(location: Option<&'a str>) -> Self {
        LazyMetadata {
            location,
            metadata: None,
        }
    }

    fn get_metadata(&mut self, name: &str) -> Result<&Metadata> {
        if self.metadata.is_none() {
            let stat = match self.location {
                None => {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        format!("Neither {name} nor location specified."),
                    ))
                }
                Some(path) => symlink_metadata(path)?,
            };
            self.metadata = Some(stat);
        }
        Ok(self.metadata.as_ref().unwrap())
    }

    fn parse_u32(
        &mut self,
        entry: Option<&str>,
        name: &str,
        f: impl Fn(&Metadata) -> u32,
    ) -> Result<u32> {
        match entry {
            Some("-") | Some("") | None => Ok(f(self.get_metadata(name)?)),
            Some(x) => match x.parse() {
                Ok(y) => Ok(y),
                Err(e) => Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid {name}: {e}"),
                )),
            },
        }
    }

    fn parse_octal(
        &mut self,
        entry: Option<&str>,
        name: &str,
        f: impl Fn(&Metadata) -> u16,
    ) -> Result<u16> {
        match entry {
            Some("-") | Some("") | None => Ok(f(self.get_metadata(name)?)),
            Some(x) => match u16::from_str_radix(x, 8) {
                Ok(y) => Ok(y),
                Err(e) => Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("invalid {name}: {e}"),
                )),
            },
        }
    }

    fn parse_filetype(&mut self, entry: Option<&str>, name: &str) -> Result<u32> {
        let filetype = match entry {
            Some("file") => FILETYPE_REGULAR_FILE,
            Some("dir") => FILETYPE_DIRECTORY,
            Some("block") => FILETYPE_BLOCK_DEVICE,
            Some("char") => FILETYPE_CHARACTER_DEVICE,
            Some("link") => FILETYPE_SYMLINK,
            Some("fifo") => FILETYPE_FIFO,
            Some("sock") => FILETYPE_SOCKET,
            Some("-") | Some("") | None => self.get_metadata(name)?.mode() & MODE_FILETYPE_MASK,
            Some(x) => {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("Unknown filetype '{x}'"),
                ))
            }
        };
        Ok(filetype)
    }
}

fn pathbuf_to_string(path: std::path::PathBuf) -> Result<String> {
    path.into_os_string().into_string().map_err(|e| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("failed to convert path {e:#?} to string"),
        )
    })
}

fn parse_symlink(entry: Option<&str>, location: Option<&str>) -> Result<String> {
    match entry {
        // TODO
        Some("-") | None => match location {
            None => Err(Error::new(
                ErrorKind::InvalidInput,
                "Neither symlink nor location specified.",
            )),
            Some(path) => Ok(pathbuf_to_string(std::fs::read_link(path)?)?),
        },
        Some(x) => Ok(x.into()),
    }
}

fn replace_empty<'a>(entry: Option<&'a str>) -> Option<&'a str> {
    match entry {
        Some("-") | Some("") | None => None,
        Some(x) => Some(x),
    }
}

fn sanitize_path<'a>(path: &'a str) -> &'a str {
    match path.strip_prefix("./") {
        Some(p) => {
            if p.is_empty() {
                ".".into()
            } else {
                p.into()
            }
        }
        None => match path.strip_prefix("/") {
            Some(p) => {
                if p.is_empty() {
                    ".".into()
                } else {
                    p.into()
                }
            }
            None => path.into(),
        },
    }
}

// Return the permission bits from Metadata.mode
fn get_permission(mode: u32) -> u16 {
    (mode & MODE_PERMISSION_MASK) as u16
}

// Return the rdev major from Metadata
fn get_rmajor(metadata: &Metadata) -> u32 {
    major(metadata.rdev())
}

// Return the rdev major from Metadata
fn get_rminor(metadata: &Metadata) -> u32 {
    minor(metadata.rdev())
}

impl File {
    fn new<S: Into<String>>(
        filetype: Filetype,
        name: S,
        mode: u16,
        uid: u32,
        gid: u32,
        mtime: u32,
    ) -> Self {
        Self {
            filetype,
            name: name.into(),
            mode,
            uid,
            gid,
            mtime,
        }
    }

    /*
    Description from the 3cpio man page:

    * Each element in the line is separated by a tab.
    * In case an element is equal to - it is treated as not specified
      and it is derived from the input file.
    * If a line starts with # it is treated as comment and ignored.

    ----
    # a comment
    <location> <name> file <mode> <uid> <gid> <mtime> <filesize>
    <location> <name> dir <mode> <uid> <gid> <mtime>
    <location> <name> block <mode> <uid> <gid> <mtime> <major> <minor>
    <location> <name> char <mode> <uid> <gid> <mtime> <major> <minor>
    <location> <name> link <mode> <uid> <gid> <mtime> <target>
    <location> <name> fifo <mode> <uid> <gid> <mtime>
    <location> <name> sock <mode> <uid> <gid> <mtime>
    ----

    <location> can be left unspecified in case all other needed fields
    are specified (and the file is otherwise empty).
    */
    fn from_line<S: AsRef<str>>(line: S) -> Result<Self> {
        let mut iter = line.as_ref().split('\t');
        let location = replace_empty(iter.next());
        let name = match replace_empty(iter.next()) {
            Some(name) => name,
            None => match location {
                Some(path) => sanitize_path(path),
                None => {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "Neither location nor name were specified.",
                    ))
                }
            },
        };
        let mut lazy_metadata = LazyMetadata::new(location);
        let filetype_value = lazy_metadata.parse_filetype(iter.next(), "filetype")?;
        let mode = lazy_metadata.parse_octal(iter.next(), "mode", |m| get_permission(m.mode()))?;
        let uid = lazy_metadata.parse_u32(iter.next(), "uid", |m| m.uid())?;
        let gid = lazy_metadata.parse_u32(iter.next(), "gid", |m| m.gid())?;
        let mtime =
            lazy_metadata.parse_u32(iter.next(), "mtime", |m| m.mtime().try_into().unwrap())?;

        let filetype = match filetype_value {
            FILETYPE_REGULAR_FILE => {
                let filesize = lazy_metadata
                    .parse_u32(iter.next(), "filesize", |m| m.size().try_into().unwrap())?;
                if filesize == 0 {
                    Filetype::EmptyFile
                } else {
                    Filetype::RegularFile {
                        location: location.unwrap().into(),
                        filesize,
                    }
                }
            }
            FILETYPE_DIRECTORY => Filetype::Directory,
            FILETYPE_BLOCK_DEVICE => Filetype::BlockDevice {
                major: lazy_metadata.parse_u32(iter.next(), "major", get_rmajor)?,
                minor: lazy_metadata.parse_u32(iter.next(), "minor", get_rminor)?,
            },
            FILETYPE_CHARACTER_DEVICE => Filetype::CharacterDevice {
                major: lazy_metadata.parse_u32(iter.next(), "major", get_rmajor)?,
                minor: lazy_metadata.parse_u32(iter.next(), "minor", get_rminor)?,
            },
            FILETYPE_SYMLINK => Filetype::Symlink {
                target: parse_symlink(iter.next(), location)?,
            },
            FILETYPE_FIFO => Filetype::Fifo,
            FILETYPE_SOCKET => Filetype::Socket,
            // TODO
            x => {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("Unknown filetype '{x}'"),
                ))
            }
        };

        Ok(Self::new(filetype, name, mode, uid, gid, mtime))
    }
}

/*
impl Manifest {
    fn new<S>(input_path: Option<S>, path: S) -> Input    where
    S: Into<String> {
        Self {
            input_path: input_path.map(|s| s.into()),
            path: path.into(),
        }
    }
}
    */

/*fn read_input<R: ?Sized>(reader: BufReader<R>) -> Result<Vec<Input>> {
    let mut x = Vec::new();
    x.push(Input { filename: "TODO".into() });
    Ok(x)
}*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path_absolute_path() {
        assert_eq!(sanitize_path("/path/to/file"), "path/to/file");
    }

    #[test]
    fn test_sanitize_path_dot() {
        assert_eq!(sanitize_path("."), ".");
    }

    #[test]
    fn test_sanitize_path_dot_slash() {
        assert_eq!(sanitize_path("./"), ".");
    }

    #[test]
    fn test_sanitize_path_dot_slash_path() {
        assert_eq!(sanitize_path("./path/to/file"), "path/to/file");
    }

    #[test]
    fn test_sanitize_path_relative_path() {
        assert_eq!(sanitize_path("path/to/file"), "path/to/file");
    }

    #[test]
    fn test_sanitize_path_root() {
        assert_eq!(sanitize_path("/"), ".");
    }

    #[test]
    fn test_file_from_line_full_regular_file() {
        let line = "/usr/bin/true\tusr/bin/true\tfile\t755\t0\t0\t1739259005\t35288";
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::RegularFile {
                    location: "/usr/bin/true".into(),
                    filesize: 35288
                },
                "usr/bin/true",
                0o755,
                0,
                0,
                1739259005
            )
        );
    }

    #[test]
    fn test_file_from_line_full_directory() {
        let line = "/usr\tusr\tdir\t755\t0\t0\t1681992796";
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Directory, "usr", 0o755, 0, 0, 1681992796)
        );
    }

    #[test]
    fn test_file_from_line_full_block_device() {
        let line = "/dev/nvme0n1p2\tdev/nvme0n1p2\tblock\t660\t0\t0\t1745246683\t259\t2";
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::BlockDevice {
                    major: 259,
                    minor: 2
                },
                "dev/nvme0n1p2",
                0o660,
                0,
                0,
                1745246683
            )
        );
    }

    #[test]
    fn test_file_from_line_full_character_device() {
        let line = "/dev/console\tdev/console\tchar\t600\t0\t5\t1745246724\t5\t1";
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::CharacterDevice { major: 5, minor: 1 },
                "dev/console",
                0o600,
                0,
                5,
                1745246724
            )
        );
    }

    #[test]
    fn test_file_from_line_full_symlink() {
        let line = "/bin\tbin\tlink\t777\t0\t0\t1647786132\tusr/bin";
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Symlink {
                    target: "usr/bin".into()
                },
                "bin",
                0o777,
                0,
                0,
                1647786132
            )
        );
    }

    #[test]
    fn test_file_from_line_full_fifo() {
        let line = "/run/initctl\trun/initctl\tfifo\t0600\t0\t0\t1746789067";
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Fifo, "run/initctl", 0o600, 0, 0, 1746789067)
        )
    }

    #[test]
    fn test_file_from_line_full_socket() {
        let line = "/run/systemd/notify\trun/systemd/notify\tsock\t777\t0\t0\t1746789058";
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Socket,
                "run/systemd/notify",
                0o777,
                0,
                0,
                1746789058,
            )
        )
    }

    #[test]
    fn test_file_from_line_empty_file() {
        let line = "\tetc/fstab.empty\tfile\t644\t0\t0\t1744705149\t0";
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::EmptyFile,
                "etc/fstab.empty",
                0o644,
                0,
                0,
                1744705149
            )
        );
    }

    #[test]
    fn test_file_from_line_location_regular_file() {
        let line = "/usr/bin/true";
        let stat = symlink_metadata("/usr/bin/true").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::RegularFile {
                    location: "/usr/bin/true".into(),
                    filesize: stat.size().try_into().unwrap(),
                },
                "usr/bin/true",
                0o755,
                0,
                0,
                mtime,
            )
        );
    }

    #[test]
    fn test_file_from_line_location_directory() {
        let line = "/usr";
        let stat = symlink_metadata("/usr").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Directory, "usr", 0o755, 0, 0, mtime)
        );
    }

    #[test]
    fn test_file_from_line_location_block_device() {
        let line = "/dev/loop0";
        let stat = symlink_metadata("/dev/loop0").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::BlockDevice {
                    major: major(stat.rdev()),
                    minor: minor(stat.rdev()),
                },
                "dev/loop0",
                0o660,
                0,
                6,
                mtime,
            )
        );
    }

    #[test]
    fn test_file_from_line_location_character_device() {
        let line = "/dev/console";
        let stat = symlink_metadata("/dev/console").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::CharacterDevice { major: 5, minor: 1 },
                "dev/console",
                0o600,
                0,
                5,
                mtime,
            )
        );
    }

    #[test]
    fn test_file_from_line_location_symlink() {
        let line = "/bin";
        let stat = symlink_metadata("/bin").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Symlink {
                    target: "usr/bin".into()
                },
                "bin",
                0o777,
                0,
                0,
                mtime,
            )
        );
    }

    #[test]
    fn test_file_from_line_location_fifo() {
        let line = "/run/initctl";
        let stat = symlink_metadata("/run/initctl").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Fifo, "run/initctl", 0o600, 0, 0, mtime)
        )
    }

    #[test]
    fn test_file_from_line_location_socket() {
        let line = "/run/systemd/notify";
        let stat = symlink_metadata("/run/systemd/notify").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Socket, "run/systemd/notify", 0o777, 0, 0, mtime)
        )
    }

    #[test]
    fn test_file_from_line_empty_fields() {
        let line = "/run\t\t\t\t\t\t";
        let stat = symlink_metadata("/run").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Directory, "run", 0o755, 0, 0, mtime)
        )
    }

    #[test]
    fn test_file_from_line_fields_with_dash() {
        let line = "/etc\t-\t-\t-\t-\t-\t";
        let stat = symlink_metadata("/etc").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let file = File::from_line(line).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Directory, "etc", 0o755, 0, 0, mtime)
        )
    }
}
