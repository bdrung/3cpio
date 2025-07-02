// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::collections::HashMap;
use std::fs::{symlink_metadata, Metadata};
use std::io::{BufRead, Error, ErrorKind, Result, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use crate::align_to_4_bytes;
use crate::compression::Compression;
use crate::filetype::*;
use crate::header::Header;
use crate::libc::{major, minor};
use crate::LOG_LEVEL_DEBUG;

#[derive(Debug, PartialEq)]
struct Hardlink {
    location: String,
    filesize: u32,
    references: u32,
}

fn get_hardlink_key(stat: &Metadata) -> u128 {
    (u128::from(stat.ino()) << 64) | u128::from(stat.dev())
}

impl Hardlink {
    fn new<S: Into<String>>(location: S, filesize: u32) -> Self {
        Self {
            location: location.into(),
            filesize,
            references: 1,
        }
    }

    #[cfg(test)]
    fn with_references<S: Into<String>>(location: S, filesize: u32, references: u32) -> Self {
        Self {
            location: location.into(),
            filesize,
            references,
        }
    }
}

#[derive(Debug, PartialEq)]
enum Filetype {
    Hardlink { key: u128, index: u32 },
    EmptyFile,
    Directory,
    BlockDevice { major: u32, minor: u32 },
    CharacterDevice { major: u32, minor: u32 },
    Fifo,
    Socket,
    Symlink { target: String },
}

#[derive(Debug, PartialEq)]
struct File {
    filetype: Filetype,
    name: String,
    mode: u16,
    uid: u32,
    gid: u32,
    mtime: u32,
}

#[derive(Debug, PartialEq)]
pub struct Archive {
    compression: Compression,
    files: Vec<File>,
    hardlinks: HashMap<u128, Hardlink>,
}

#[derive(Debug, PartialEq)]
pub struct Manifest {
    archives: Vec<Archive>,
    umask: u32,
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

fn replace_empty(entry: Option<&str>) -> Option<&str> {
    match entry {
        Some("-") | Some("") | None => None,
        Some(x) => Some(x),
    }
}

fn sanitize_path(path: &str) -> &str {
    match path.strip_prefix("./") {
        Some(p) => {
            if p.is_empty() {
                "."
            } else {
                p
            }
        }
        None => match path.strip_prefix("/") {
            Some(p) => {
                if p.is_empty() {
                    "."
                } else {
                    p
                }
            }
            None => path,
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

// Determine umask for creating the cpio file based on the given file mode.
// Since the "group" mode of the file can differ from the cpio writer group,
// use the umask from "other" for "group".
fn determine_umask(mode: u32) -> u32 {
    let other_umask = !mode & 0o7;
    (other_umask << 3) | other_umask
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
    fn from_line<S: AsRef<str>>(
        line: S,
        hardlinks: &mut HashMap<u128, Hardlink>,
    ) -> Result<(Self, u32)> {
        let mut umask = 0;
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
        // FIXME: use try_into() from header
        let mtime =
            lazy_metadata.parse_u32(iter.next(), "mtime", |m| m.mtime().try_into().unwrap())?;

        let filetype = match filetype_value {
            FILETYPE_REGULAR_FILE => {
                // FIXME: use try_into() from header
                let filesize = lazy_metadata
                    .parse_u32(iter.next(), "filesize", |m| m.size().try_into().unwrap())?;
                if filesize == 0 {
                    Filetype::EmptyFile
                } else {
                    let stat = lazy_metadata.get_metadata("filetype")?;
                    umask = determine_umask(stat.mode());
                    let key = get_hardlink_key(stat);
                    let index = match hardlinks.get_mut(&key) {
                        Some(hardlink) => {
                            hardlink.references += 1;
                            hardlink.references
                        }
                        None => {
                            // Defer writing the hardlink
                            hardlinks.insert(key, Hardlink::new(location.unwrap(), filesize));
                            1
                        }
                    };
                    Filetype::Hardlink { key, index }
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

        Ok((Self::new(filetype, name, mode, uid, gid, mtime), umask))
    }

    fn generate_header(
        &self,
        next_free_ino: u32,
        hardlinks: &HashMap<u128, Hardlink>,
        hardlinks2ino: &mut HashMap<u128, u32>,
    ) -> (Header, u32) {
        let mut nlink = 1;
        let mut filesize = 0;
        let mut rmajor = 0;
        let mut rminor = 0;
        let mut ino = next_free_ino;
        let mut next_ino = next_free_ino + 1;
        let filetype;
        match &self.filetype {
            Filetype::EmptyFile => filetype = FILETYPE_REGULAR_FILE,
            Filetype::Hardlink { key, index } => {
                filetype = FILETYPE_REGULAR_FILE;
                if let Some(existing_ino) = hardlinks2ino.get(key) {
                    ino = *existing_ino;
                    next_ino = next_free_ino;
                } else {
                    hardlinks2ino.insert(*key, ino);
                }
                let hardlink = hardlinks.get(key).unwrap();
                nlink = hardlink.references;
                // last reference will write the hardlink
                filesize = if *index == nlink {
                    hardlink.filesize
                } else {
                    0
                };
            }
            Filetype::Directory => {
                filetype = FILETYPE_DIRECTORY;
                nlink = 2;
            }
            Filetype::BlockDevice { major, minor } => {
                filetype = FILETYPE_BLOCK_DEVICE;
                rmajor = *major;
                rminor = *minor;
            }
            Filetype::CharacterDevice { major, minor } => {
                filetype = FILETYPE_CHARACTER_DEVICE;
                rmajor = *major;
                rminor = *minor;
            }
            Filetype::Symlink { target } => {
                filetype = FILETYPE_SYMLINK;
                // FIXME: check length
                filesize = target.len().try_into().unwrap();
            }
            Filetype::Fifo => filetype = FILETYPE_FIFO,
            Filetype::Socket => filetype = FILETYPE_SOCKET,
        }
        (
            Header::new(
                ino,
                filetype | u32::from(self.mode),
                self.uid,
                self.gid,
                nlink,
                self.mtime,
                filesize,
                rmajor,
                rminor,
                self.name.clone(),
            ),
            next_ino,
        )
    }
}

impl Archive {
    fn new() -> Self {
        Self {
            compression: Compression::Uncompressed,
            files: Vec::new(),
            hardlinks: HashMap::new(),
        }
    }

    #[cfg(test)]
    fn with_files(files: Vec<File>) -> Self {
        Self {
            compression: Compression::Uncompressed,
            files,
            hardlinks: HashMap::new(),
        }
    }

    #[cfg(test)]
    fn with_files_and_hardlinks(files: Vec<File>, hardlinks: HashMap<u128, Hardlink>) -> Self {
        Self {
            compression: Compression::Uncompressed,
            files,
            hardlinks,
        }
    }

    #[cfg(test)]
    fn with_files_compressed(files: Vec<File>, compression: Compression) -> Self {
        Self {
            compression,
            files,
            hardlinks: HashMap::new(),
        }
    }

    fn add_line<S: AsRef<str>>(&mut self, line: S) -> Result<u32> {
        let (file, umask) = File::from_line(line, &mut self.hardlinks)?;
        self.files.push(file);
        Ok(umask)
    }

    fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn set_compression(&mut self, compression: Compression) {
        self.compression = compression;
    }

    fn write_cpio<W: Write>(
        self,
        output_file: &mut W,
        source_date_epoch: Option<u32>,
        log_level: u32,
    ) -> Result<()> {
        let mut next_ino = 0;
        let mut hardlink_ino = HashMap::new();
        let mut header;
        for file in self.files {
            (header, next_ino) = file.generate_header(next_ino, &self.hardlinks, &mut hardlink_ino);
            if let Some(epoch) = source_date_epoch {
                if header.mtime > epoch {
                    header.mtime = epoch;
                }
            }
            if log_level >= LOG_LEVEL_DEBUG {
                writeln!(std::io::stderr(), "{:?}", header)?;
            };
            header.write(output_file)?;
            match file.filetype {
                Filetype::Hardlink { key, index: _ } => {
                    if header.filesize > 0 {
                        let hardlink = self.hardlinks.get(&key).unwrap();
                        copy_file_with_padding(&hardlink.location, hardlink.filesize, output_file)?;
                    }
                }
                Filetype::Symlink { target } => {
                    output_file.write_all(target.as_bytes())?;
                    write_padding(output_file, header.filesize)?;
                    // TODO: check length + write padding
                }
                Filetype::EmptyFile
                | Filetype::Directory
                | Filetype::BlockDevice { major: _, minor: _ }
                | Filetype::CharacterDevice { major: _, minor: _ }
                | Filetype::Fifo
                | Filetype::Socket => {}
            }
        }
        Header::trailer().write(output_file)?;
        Ok(())
    }
}

impl Manifest {
    fn new(archives: Vec<Archive>, umask: u32) -> Self {
        Self { archives, umask }
    }

    pub fn from_input<R: BufRead>(reader: R, log_level: u32) -> Result<Self> {
        let mut archives = vec![Archive::new()];
        let mut current_archive = archives.last_mut().unwrap();
        let mut line_number = 1;
        let mut umask = 0;
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.starts_with("#") || line.is_empty() {
                if line.starts_with("#cpio") {
                    if !current_archive.is_empty() {
                        archives.push(Archive::new());
                        current_archive = archives.last_mut().unwrap();
                    };
                    match line.strip_prefix("#cpio:") {
                        Some(compression_str) => {
                            let compression = Compression::from_command_line(compression_str)?;
                            current_archive.set_compression(compression);
                        }
                        None => {
                            // TODO: check for non-matching
                        }
                    }
                }
                continue;
            }
            if log_level >= LOG_LEVEL_DEBUG {
                eprintln!("Parsing line {}: {}", line_number, line);
            }
            let file_mask = current_archive.add_line(line)?;
            umask |= file_mask;
            line_number += 1;
        }
        Ok(Self::new(archives, umask))
    }

    fn apply_umask(&self, file: &std::fs::File) -> Result<()> {
        let mode = file.metadata()?.mode();
        let new_mode = mode & !self.umask;
        if mode != new_mode {
            file.set_permissions(PermissionsExt::from_mode(new_mode))?;
        }
        Ok(())
    }

    pub fn write_cpios(
        self,
        mut file: Option<std::fs::File>,
        source_date_epoch: Option<u32>,
        log_level: u32,
    ) -> Result<()> {
        if let Some(file) = file.as_ref() {
            self.apply_umask(file)?;
        }
        for archive in self.archives {
            if archive.compression.is_uncompressed() {
                if let Some(file) = file.as_mut() {
                    archive.write_cpio(file, source_date_epoch, log_level)?;
                } else {
                    let mut stdout = std::io::stdout();
                    archive.write_cpio(&mut stdout, source_date_epoch, log_level)?;
                }
            } else {
                let mut compressor = archive.compression.compress(file, source_date_epoch)?;
                archive.write_cpio(&mut compressor, source_date_epoch, log_level)?;
                // TODO: Check that the compressed cpio is the last
                break;
            }
        }
        Ok(())
    }
}

fn copy_file_with_padding<W: Write>(path: &str, filesize: u32, writer: &mut W) -> Result<()> {
    let mut reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let copied_bytes = std::io::copy(&mut reader, writer)?;
    if copied_bytes != filesize.into() {
        return Err(Error::new(
            ErrorKind::UnexpectedEof,
            format!(
                "Copied {} bytes from {} but expected {} bytes.",
                copied_bytes, path, filesize
            ),
        ));
    }
    write_padding(writer, filesize)?;
    Ok(())
}

pub fn write_padding<W: Write>(file: &mut W, written_bytes: u32) -> Result<()> {
    let padding_len = align_to_4_bytes(written_bytes);
    if padding_len == 0 {
        return Ok(());
    }
    let padding = vec![0u8; padding_len.try_into().unwrap()];
    file.write_all(&padding)
}

#[cfg(test)]
mod tests {
    use std::fs::hard_link;
    use std::path::PathBuf;

    use super::*;
    use crate::libc::tests::make_temp_dir;
    use crate::LOG_LEVEL_WARNING;

    pub fn make_temp_dir_with_hardlinks() -> Result<PathBuf> {
        let temp_dir = make_temp_dir()?;
        let path = temp_dir.join("a");
        let mut file = std::fs::File::create(&path)?;
        file.set_permissions(PermissionsExt::from_mode(0o755))?;
        file.write_all(b"content")?;
        hard_link(&path, temp_dir.join("b"))?;
        hard_link(&path, temp_dir.join("c"))?;
        Ok(temp_dir)
    }

    #[test]
    fn test_determine_umask_all_read() {
        assert_eq!(determine_umask(0o755), 0o022);
    }

    #[test]
    fn test_determine_umask_only_root() {
        assert_eq!(determine_umask(0o640), 0o077);
    }

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
        let stat = symlink_metadata("/usr/bin/true").unwrap();
        let key = get_hardlink_key(&stat);
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 1 },
                "usr/bin/true",
                0o755,
                0,
                0,
                1739259005
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(
            hardlinks,
            HashMap::from([(key, Hardlink::new("/usr/bin/true", 35288))])
        );
    }

    #[test]
    fn test_file_from_line_full_directory() {
        let line = "/usr\tusr\tdir\t755\t0\t0\t1681992796";
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Directory, "usr", 0o755, 0, 0, 1681992796)
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_full_block_device() {
        let line = "/dev/nvme0n1p2\tdev/nvme0n1p2\tblock\t660\t0\t0\t1745246683\t259\t2";
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
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
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_full_character_device() {
        let line = "/dev/console\tdev/console\tchar\t600\t0\t5\t1745246724\t5\t1";
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
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
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_full_symlink() {
        let line = "/bin\tbin\tlink\t777\t0\t0\t1647786132\tusr/bin";
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
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
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_full_fifo() {
        let line = "/run/initctl\trun/initctl\tfifo\t0600\t0\t0\t1746789067";
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Fifo, "run/initctl", 0o600, 0, 0, 1746789067)
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_full_socket() {
        let line = "/run/systemd/notify\trun/systemd/notify\tsock\t777\t0\t0\t1746789058";
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
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
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_empty_file() {
        let line = "\tetc/fstab.empty\tfile\t644\t0\t0\t1744705149\t0";
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
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
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_location_regular_file() {
        let line = "/usr/bin/true";
        let stat = symlink_metadata("/usr/bin/true").unwrap();
        let key = get_hardlink_key(&stat);
        let mtime = stat.mtime().try_into().unwrap();
        let size = stat.size().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 1 },
                "usr/bin/true",
                0o755,
                0,
                0,
                mtime,
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(
            hardlinks,
            HashMap::from([(key, Hardlink::new("/usr/bin/true", size))])
        );
    }

    #[test]
    fn test_file_from_line_location_duplicate_file() {
        let line = "/usr/bin/true\ttrue\t\t\t\t\t1745485084";
        let stat = symlink_metadata("/usr/bin/true").unwrap();
        let key = get_hardlink_key(&stat);
        let size = stat.size().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 1 },
                "true",
                0o755,
                0,
                0,
                1745485084,
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(
            hardlinks,
            HashMap::from([(key, Hardlink::new("/usr/bin/true", size))])
        );

        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 2 },
                "true",
                0o755,
                0,
                0,
                1745485084,
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(
            hardlinks,
            HashMap::from([(key, Hardlink::with_references("/usr/bin/true", size, 2))])
        );
    }

    #[test]
    fn test_file_from_line_location_hardlink() {
        let temp_dir = make_temp_dir_with_hardlinks().unwrap();
        let path = temp_dir.join("a").to_str().unwrap().to_owned();
        let line = format!("{}\ta\t\t644\t1\t2", path);
        let stat = symlink_metadata(&path).unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let key = get_hardlink_key(&stat);
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 1 },
                "a",
                0o644,
                1,
                2,
                mtime,
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(hardlinks, HashMap::from([(key, Hardlink::new(&path, 7))]));

        let line = format!(
            "{}/b\tb\t\t640\t3\t4\t1751413453",
            temp_dir.to_str().unwrap()
        );
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 2 },
                "b",
                0o640,
                3,
                4,
                1751413453,
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(
            hardlinks,
            HashMap::from([(key, Hardlink::with_references(&path, 7, 2))])
        );
    }

    #[test]
    fn test_file_from_line_location_relative_directory() {
        let line = "./tests\t\t\t510\t7\t42";
        let stat = symlink_metadata("tests").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Directory, "tests", 0o510, 7, 42, mtime)
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_location_directory() {
        let line = "/usr";
        let stat = symlink_metadata("/usr").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Directory, "usr", 0o755, 0, 0, mtime)
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_location_block_device() {
        let line = "/dev/loop0";
        let stat = symlink_metadata("/dev/loop0").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
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
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_location_character_device() {
        let line = "/dev/console";
        let stat = symlink_metadata("/dev/console").unwrap();
        let mode = (stat.mode() & MODE_PERMISSION_MASK).try_into().unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::CharacterDevice { major: 5, minor: 1 },
                "dev/console",
                mode,
                0,
                5,
                mtime,
            )
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_location_symlink() {
        let line = "/bin";
        let stat = symlink_metadata("/bin").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
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
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_location_fifo() {
        let line = "/run/initctl";
        let stat = symlink_metadata("/run/initctl").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Fifo, "run/initctl", 0o600, 0, 0, mtime)
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_location_socket() {
        let line = "/run/systemd/notify";
        let stat = symlink_metadata("/run/systemd/notify").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Socket, "run/systemd/notify", 0o777, 0, 0, mtime)
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_empty_fields() {
        let line = "/run\t\t\t\t\t\t";
        let stat = symlink_metadata("/run").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Directory, "run", 0o755, 0, 0, mtime)
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_file_from_line_fields_with_dash() {
        let line = "/etc\t-\t-\t-\t-\t-\t";
        let stat = symlink_metadata("/etc").unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(Filetype::Directory, "etc", 0o755, 0, 0, mtime)
        );
        assert_eq!(umask, 0);
        assert!(hardlinks.is_empty());
    }

    #[test]
    fn test_manifest_from_input() {
        let input = b"\
        # This is a comment\n\n\
        /bin\tbin\tdir\t755\t0\t0\t1681992796\n\
        /bin/true\tbin/true\tfile\t755\t0\t0\t1739259005\t35288\n";
        let manifest = Manifest::from_input(input.as_ref(), LOG_LEVEL_WARNING).unwrap();
        let stat = symlink_metadata("/usr/bin/true").unwrap();
        let key = get_hardlink_key(&stat);
        let expected_archive = Archive::with_files_and_hardlinks(
            vec![
                File::new(Filetype::Directory, "bin", 0o755, 0, 0, 1681992796),
                File::new(
                    Filetype::Hardlink { key, index: 1 },
                    "bin/true",
                    0o755,
                    0,
                    0,
                    1739259005,
                ),
            ],
            HashMap::from([(key, Hardlink::new("/bin/true", 35288))]),
        );
        assert_eq!(manifest, Manifest::new(vec![expected_archive], 0o022));
    }

    #[test]
    fn test_manifest_from_input_compressed() {
        let input = b"\
        #cpio: zstd -1\n\
        /bin\tbin\tdir\t755\t0\t0\t1681992796\n";
        let manifest = Manifest::from_input(input.as_ref(), LOG_LEVEL_WARNING).unwrap();
        let expected_archive = Archive::with_files_compressed(
            vec![File::new(
                Filetype::Directory,
                "bin",
                0o755,
                0,
                0,
                1681992796,
            )],
            Compression::Zstd { level: Some(1) },
        );
        assert_eq!(manifest, Manifest::new(vec![expected_archive], 0));
    }

    #[test]
    fn test_manifest_from_input_multiple_uncompressed() {
        let input = b"\
        # This is a comment\n\n\
        #cpio\n\
        /bin\tbin\tdir\t755\t0\t0\t1681992796\n\
        #cpio\n\
        /\t.\tdir\t755\t0\t0\t1732230747\n";
        let manifest = Manifest::from_input(input.as_ref(), LOG_LEVEL_WARNING).unwrap();
        let expected_manifest = Manifest::new(
            vec![
                Archive::with_files(vec![File::new(
                    Filetype::Directory,
                    "bin",
                    0o755,
                    0,
                    0,
                    1681992796,
                )]),
                Archive::with_files(vec![File::new(
                    Filetype::Directory,
                    ".",
                    0o755,
                    0,
                    0,
                    1732230747,
                )]),
            ],
            0,
        );
        assert_eq!(manifest, expected_manifest);
    }

    #[test]
    fn test_archive_write_cpio() {
        let archive = Archive::with_files(vec![
            File::new(Filetype::Directory, ".", 0o755, 0x333, 0x42, 0x6841897B),
            File::new(
                Filetype::BlockDevice {
                    major: 0x6425,
                    minor: 0x1437,
                },
                "loop0",
                0o660,
                0x334,
                0x43,
                0x6862B88B,
            ),
            File::new(
                Filetype::CharacterDevice {
                    major: 0x2E0E,
                    minor: 0x8C75,
                },
                "console",
                0o600,
                0x335,
                0x44,
                0x6862B8B4,
            ),
            File::new(
                Filetype::Symlink {
                    target: "usr/bin".into(),
                },
                "bin",
                0o777,
                0x336,
                0x45,
                0x62373894,
            ),
            File::new(Filetype::Fifo, "initctl", 0o600, 0x337, 0x46, 0x6862B88A),
            File::new(Filetype::Socket, "notify", 0o777, 0x338, 0x47, 0x681DE2C2),
            File::new(Filetype::EmptyFile, "fstab", 0x644, 0x339, 0x48, 0x6E44C280),
        ]);
        let mut output = Vec::new();
        archive
            .write_cpio(&mut output, Some(0x6B49D200), LOG_LEVEL_WARNING)
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&output).unwrap(),
            "07070100000000000041ED0000033300000042000000026841897B\
            00000000000000000000000000000000000000000000000200000000\
            .\0\
            07070100000001000061B00000033400000043000000016862B88B\
            00000000000000000000000000006425000014370000000600000000\
            loop0\0\
            07070100000002000021800000033500000044000000016862B8B4\
            00000000000000000000000000002E0E00008C750000000800000000\
            console\0\0\0\
            070701000000030000A1FF00000336000000450000000162373894\
            00000007000000000000000000000000000000000000000400000000\
            bin\0\0\0usr/bin\0\
            07070100000004000011800000033700000046000000016862B88A\
            00000000000000000000000000000000000000000000000800000000\
            initctl\0\0\0\
            070701000000050000C1FF000003380000004700000001681DE2C2\
            00000000000000000000000000000000000000000000000700000000\
            notify\0\0\0\0\
            07070100000006000086440000033900000048000000016B49D200\
            00000000000000000000000000000000000000000000000600000000\
            fstab\0\
            070701000000000000000000000000000000000000000100000000\
            00000000000000000000000000000000000000000000000B00000000\
            TRAILER!!!\0\0\0\0",
        );
    }

    #[test]
    fn test_archive_write_cpio_hardlinks() {
        let temp_dir = make_temp_dir_with_hardlinks().unwrap();
        let path = temp_dir.join("a").to_str().unwrap().to_owned();
        // This archive data is the output of test_file_from_line_location_hardlink.
        let archive = Archive::with_files_and_hardlinks(
            vec![
                File::new(
                    Filetype::Hardlink {
                        key: 8921120,
                        index: 1,
                    },
                    "a",
                    0o644,
                    1,
                    2,
                    0x6861C7C5,
                ),
                File::new(
                    Filetype::Hardlink {
                        key: 8921120,
                        index: 2,
                    },
                    "b",
                    0o640,
                    3,
                    4,
                    0x686472CD,
                ),
            ],
            HashMap::from([(8921120, Hardlink::with_references(&path, 7, 2))]),
        );
        let mut output = Vec::new();
        archive
            .write_cpio(&mut output, None, LOG_LEVEL_WARNING)
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&output).unwrap(),
            "07070100000000000081A40000000100000002000000026861C7C5\
            00000000000000000000000000000000000000000000000200000000\
            a\0\
            07070100000000000081A0000000030000000400000002686472CD\
            00000007000000000000000000000000000000000000000200000000\
            b\0content\0\
            070701000000000000000000000000000000000000000100000000\
            00000000000000000000000000000000000000000000000B00000000\
            TRAILER!!!\0\0\0\0",
        );
    }
}
