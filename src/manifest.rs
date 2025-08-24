// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::collections::HashMap;
use std::fs::{symlink_metadata, Metadata};
use std::io::{BufRead, BufWriter, Error, ErrorKind, Result, Write};
use std::num::NonZeroU32;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use crate::compression::Compression;
use crate::extended_error::ExtendedError;
use crate::filetype::*;
use crate::header::{calculate_size, padding_needed_for, Header, TRAILER_SIZE};
use crate::libc::{major, minor};
use crate::logger::Logger;
use crate::CPIO_ALIGNMENT;

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
pub(crate) struct Archive {
    compression: Compression,
    files: Vec<File>,
    hardlinks: HashMap<u128, Hardlink>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Manifest {
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
                Some(path) => symlink_metadata(path).map_err(|e| e.add_prefix(path))?,
            };
            self.metadata = Some(stat);
        }
        Ok(self.metadata.as_ref().unwrap())
    }

    fn parse_u32(
        &mut self,
        entry: Option<&str>,
        name: &str,
        f: impl Fn(&Metadata) -> Result<u32>,
    ) -> Result<u32> {
        match entry {
            Some("-") | Some("") | None => Ok(f(self.get_metadata(name)?)?),
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
        Some("-") | Some("") | None => match location {
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
fn get_rmajor(metadata: &Metadata) -> Result<u32> {
    Ok(major(metadata.rdev()))
}

// Return the rdev major from Metadata
fn get_rminor(metadata: &Metadata) -> Result<u32> {
    Ok(minor(metadata.rdev()))
}

fn get_mtime(metadata: &Metadata) -> Result<u32> {
    metadata.mtime().try_into().map_err(|_| {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "mtime {} outside of supported range from 0 to 4,294,967,295.",
                metadata.mtime()
            ),
        )
    })
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

    The manifest is a text format that is parsed line by line.

    If the line starts with _#cpio_ it is interpreted as section marker to start
    a new cpio. A compression may be specified by adding a colon followed by the
    compression format and an optional compression level.
    Example for a Zstandard-compressed cpio with compression level 9:

    ----
    #cpio: zstd -9
    ----

    All lines starting with _#_ excluding _#cpio_ (see above) will be
    treated as comments and will be ignored.

    Each element in the line is separated by a tab and is expected to be one
    of the following file types:

    ----
    <location> <name> file <mode> <uid> <gid> <mtime> <filesize>
    <location> <name> dir <mode> <uid> <gid> <mtime>
    <location> <name> block <mode> <uid> <gid> <mtime> <major> <minor>
    <location> <name> char <mode> <uid> <gid> <mtime> <major> <minor>
    <location> <name> link <mode> <uid> <gid> <mtime> <target>
    <location> <name> fifo <mode> <uid> <gid> <mtime>
    <location> <name> sock <mode> <uid> <gid> <mtime>
    ----

    fifo is also known as named pipe (see fifo(7)).

    In case an element is empty or equal to - it is treated as not specified
    and it is derived from the input file.

    <location>::
      Path of the input file. It can be left unspecified in case all other
      needed fields are specified (and the file is otherwise empty).
      *Limitation*: The path must not start with #, be equal to -,
      or contain tabs.

    <name>::
      Path of the file inside the cpio. If the name is left unspecified it
      will be derived from <location>. *Limitation*: The path must not be
      equal to - or contain tabs.

    <mode>::
      File mode specified in octal.

    <uid>::
      User ID (owner) of the file specified in decimal.

    <gid>::
      Group ID of the file specified in decimal.

    <mtime>::
      Modification time of the file specified as seconds since the Epoch
      (1970-01-01 00:00 UTC). The specified time might be clamped by the
      time set in the SOURCE_DATE_EPOCH environment variable.

    <filesize>::
      Size of the input file in bytes. 3cpio will fail in case the input
      file is smaller than the provided file size.

    <major>::
      Major block/character device number in decimal.

    <minor>::
      Minor block/character device number in decimal.

    <target>::
      Target of the symbolic link. *Limitation*: The target path must not be
      equal to - or contain tabs.

    *Limitations*: Files cannot start with # (will be treated as comment),
    be equal to - (will be treated as not specified), or contain tabs (will
    be split by tabs). These limitations of the manifest file are not
    expected to cause problems in practice.
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
        let uid = lazy_metadata.parse_u32(iter.next(), "uid", |m| Ok(m.uid()))?;
        let gid = lazy_metadata.parse_u32(iter.next(), "gid", |m| Ok(m.gid()))?;
        let mtime = lazy_metadata.parse_u32(iter.next(), "mtime", get_mtime)?;

        let filetype = match filetype_value {
            FILETYPE_REGULAR_FILE => {
                let filesize = lazy_metadata.parse_u32(iter.next(), "filesize", |m| {
                    m.size().try_into().map_err(|_| {
                        Error::new(
                            ErrorKind::InvalidData,
                            format!(
                                "File '{}' exceeds file size limit of 4 GiB.",
                                location.unwrap()
                            ),
                        )
                    })
                })?;
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
            unknown => {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("Unknown filetype '{unknown}'"),
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

    /// Calculate the size of the cpio archive (when using the standard 4-byte padding)
    fn size(&self) -> u64 {
        let mut size = 0;
        for file in &self.files {
            let filesize = match &file.filetype {
                // Filesize of hardlinks are calculated later
                Filetype::Hardlink { key: _, index: _ } => 0,
                Filetype::Symlink { target } => u32::try_from(target.len()).unwrap(),
                Filetype::EmptyFile
                | Filetype::Directory
                | Filetype::BlockDevice { major: _, minor: _ }
                | Filetype::CharacterDevice { major: _, minor: _ }
                | Filetype::Fifo
                | Filetype::Socket => 0,
            };
            size += calculate_size(&file.name, filesize.into());
        }
        for hardlink in self.hardlinks.values() {
            debug_assert!(hardlink.references > 0);
            let filesize = hardlink.filesize.into();
            size += filesize + padding_needed_for(filesize, CPIO_ALIGNMENT);
        }
        size + TRAILER_SIZE
    }

    fn write<W: Write, LW: Write>(
        &self,
        output_file: &mut W,
        alignment: Option<NonZeroU32>,
        source_date_epoch: Option<u32>,
        mut size: u64,
        logger: &mut Logger<LW>,
    ) -> Result<u64> {
        let mut next_ino = 0;
        let mut hardlink_ino = HashMap::new();
        let mut header;
        for file in &self.files {
            info!(logger, "{}", file.name)?;
            (header, next_ino) = file.generate_header(next_ino, &self.hardlinks, &mut hardlink_ino);
            if let Some(epoch) = source_date_epoch {
                if header.mtime > epoch {
                    header.mtime = epoch;
                }
            }
            debug!(logger, "{header:?}")?;
            size += header.write_with_alignment(output_file, alignment, size)?;
            match &file.filetype {
                Filetype::Hardlink { key, index: _ } => {
                    if header.filesize > 0 {
                        let hardlink = self.hardlinks.get(key).unwrap();
                        size += copy_file(&hardlink.location, hardlink.filesize, output_file)?;
                        size += header.write_file_data_padding(output_file)?;
                    }
                }
                Filetype::Symlink { target } => {
                    output_file.write_all(target.as_bytes())?;
                    size += u64::try_from(target.len()).unwrap();
                    size += header.write_file_data_padding(output_file)?;
                }
                Filetype::EmptyFile
                | Filetype::Directory
                | Filetype::BlockDevice { major: _, minor: _ }
                | Filetype::CharacterDevice { major: _, minor: _ }
                | Filetype::Fifo
                | Filetype::Socket => {}
            }
        }
        size += Header::trailer().write(output_file)?;
        Ok(size)
    }
}

impl Manifest {
    fn new(archives: Vec<Archive>, umask: u32) -> Self {
        Self { archives, umask }
    }

    pub(crate) fn from_input<R: BufRead, W: Write>(
        reader: R,
        logger: &mut Logger<W>,
    ) -> Result<Self> {
        let mut archives = vec![Archive::new()];
        let mut current_archive = archives.last_mut().unwrap();
        let mut umask = 0;
        for (line_number, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| e.add_line(line_number + 1))?;
            let line = line.trim();
            if line.starts_with("#") || line.is_empty() {
                if line.starts_with("#cpio") {
                    debug!(logger, "Parsing line {}: {line}", line_number + 1)?;
                    if !current_archive.is_empty() {
                        archives.push(Archive::new());
                        current_archive = archives.last_mut().unwrap();
                    };
                    match line.strip_prefix("#cpio:") {
                        Some(compression_str) => {
                            let compression = Compression::from_command_line(compression_str)
                                .map_err(|e| e.add_line(line_number + 1))?;
                            current_archive.set_compression(compression);
                        }
                        None => {
                            if line != "#cpio" {
                                return Err(Error::new(
                                    ErrorKind::InvalidInput,
                                    format!(
                                        "line {}: Unknown cpio archive directive: {line}",
                                        line_number + 1,
                                    ),
                                ));
                            }
                        }
                    }
                }
                continue;
            }
            debug!(logger, "Parsing line {}: {line}", line_number + 1)?;
            let file_mask = current_archive
                .add_line(line)
                .map_err(|e| e.add_line(line_number + 1))?;
            umask |= file_mask;
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

    // Return the size in bytes of the uncompressed data.
    pub(crate) fn write_archive<W: Write>(
        self,
        mut file: Option<std::fs::File>,
        alignment: Option<NonZeroU32>,
        source_date_epoch: Option<u32>,
        logger: &mut Logger<W>,
    ) -> Result<u64> {
        let mut size = 0;
        if let Some(file) = file.as_ref() {
            self.apply_umask(file)?;
        }
        for archive in self.archives {
            if archive.compression.is_uncompressed() {
                if let Some(file) = file.as_mut() {
                    let mut writer = BufWriter::new(file);
                    size =
                        archive.write(&mut writer, alignment, source_date_epoch, size, logger)?;
                    writer.flush()?;
                } else {
                    let mut stdout = std::io::stdout().lock();
                    size =
                        archive.write(&mut stdout, alignment, source_date_epoch, size, logger)?;
                    stdout.flush()?;
                }
            } else {
                let mut compressor =
                    archive
                        .compression
                        .compress(file, source_date_epoch, || archive.size())?;
                let mut writer = BufWriter::new(compressor.stdin.as_ref().unwrap());
                size = archive.write(&mut writer, None, source_date_epoch, size, logger)?;
                writer.flush()?;
                drop(writer);
                let exit_status = compressor.wait()?;
                if !exit_status.success() {
                    return Err(Error::other(format!(
                        "{} failed: {exit_status}",
                        archive.compression.command()
                    )));
                }
                // TODO: Check that the compressed cpio is the last
                break;
            }
        }
        Ok(size)
    }
}

fn copy_file<W: Write>(path: &str, filesize: u32, writer: &mut W) -> Result<u64> {
    let file = std::fs::File::open(path).map_err(|e| e.add_prefix(path))?;
    let mut reader = std::io::BufReader::new(file);
    let copied_bytes = std::io::copy(&mut reader, writer)?;
    if copied_bytes != filesize.into() {
        return Err(Error::new(
            ErrorKind::UnexpectedEof,
            format!("Copied {copied_bytes} bytes from {path} but expected {filesize} bytes."),
        ));
    }
    Ok(copied_bytes)
}

#[cfg(test)]
mod tests {
    use std::fs::{canonicalize, hard_link};
    use std::io::Read;
    use std::path::Path;

    use super::*;
    use crate::logger::LOG_LEVEL_WARNING;
    use crate::temp_dir::TempDir;
    use crate::tests::TEST_LOCK;

    fn create_text_file_in_tmpdir<P: AsRef<Path>>(
        tempdir: &TempDir,
        filename: P,
        content: &[u8],
    ) -> String {
        let path = tempdir.path.join(filename);
        let mut text_file = std::fs::File::create(&path).unwrap();
        text_file.write_all(content).unwrap();
        path.into_os_string().into_string().unwrap()
    }

    pub(crate) fn make_temp_dir_with_hardlinks() -> Result<TempDir> {
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path.join("a");
        let mut file = std::fs::File::create(&path)?;
        file.set_permissions(PermissionsExt::from_mode(0o755))?;
        file.write_all(b"content")?;
        hard_link(&path, temp_dir.path.join("b"))?;
        hard_link(&path, temp_dir.path.join("c"))?;
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
        let line = "/usr/bin/gzip\tusr/bin/gzip\tfile\t755\t0\t0\t1739259005\t35288";
        let stat = symlink_metadata("/usr/bin/gzip").unwrap();
        let key = get_hardlink_key(&stat);
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 1 },
                "usr/bin/gzip",
                0o755,
                0,
                0,
                1739259005
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(
            hardlinks,
            HashMap::from([(key, Hardlink::new("/usr/bin/gzip", 35288))])
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
        let line = "/usr/bin/gzip";
        let stat = symlink_metadata("/usr/bin/gzip").unwrap();
        let key = get_hardlink_key(&stat);
        let mtime = stat.mtime().try_into().unwrap();
        let size = stat.size().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 1 },
                "usr/bin/gzip",
                0o755,
                0,
                0,
                mtime,
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(
            hardlinks,
            HashMap::from([(key, Hardlink::new("/usr/bin/gzip", size))])
        );
    }

    #[test]
    fn test_file_from_line_location_duplicate_file() {
        let line = "/usr/bin/gzip\tgzip\t\t\t\t\t1745485084";
        let stat = symlink_metadata("/usr/bin/gzip").unwrap();
        let key = get_hardlink_key(&stat);
        let size = stat.size().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 1 },
                "gzip",
                0o755,
                0,
                0,
                1745485084,
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(
            hardlinks,
            HashMap::from([(key, Hardlink::new("/usr/bin/gzip", size))])
        );

        let (file, umask) = File::from_line(line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::Hardlink { key, index: 2 },
                "gzip",
                0o755,
                0,
                0,
                1745485084,
            )
        );
        assert_eq!(umask, 0o022);
        assert_eq!(
            hardlinks,
            HashMap::from([(key, Hardlink::with_references("/usr/bin/gzip", size, 2))])
        );
    }

    #[test]
    fn test_file_from_line_location_hardlink() {
        let temp_dir = make_temp_dir_with_hardlinks().unwrap();
        let path = temp_dir.path.join("a").to_str().unwrap().to_owned();
        let line = format!("{path}\ta\t\t644\t1\t2");
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
            temp_dir.path.to_str().unwrap()
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
        let _lock = TEST_LOCK.lock().unwrap();
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
        let stat = match symlink_metadata("/dev/loop0") {
            Ok(s) => s,
            // This test expects a block device like /dev/loop0 being present.
            Err(_) => return,
        };
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
        let path = canonicalize("/dev/console").unwrap();
        let line = path.clone().into_os_string().into_string().unwrap();
        let stat = path.symlink_metadata().unwrap();
        let rdev = stat.rdev();
        let mode = (stat.mode() & MODE_PERMISSION_MASK).try_into().unwrap();
        let mtime = stat.mtime().try_into().unwrap();
        let mut hardlinks = HashMap::new();
        let (file, umask) = File::from_line(&line, &mut hardlinks).unwrap();
        assert_eq!(
            file,
            File::new(
                Filetype::CharacterDevice {
                    major: major(rdev),
                    minor: minor(rdev)
                },
                line.strip_prefix("/").unwrap(),
                mode,
                stat.uid(),
                stat.gid(),
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
        let stat = match symlink_metadata("/run/initctl") {
            Ok(s) => s,
            // This test expects a fifo like /run/initctl being present.
            Err(_) => return,
        };
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
        let stat = match symlink_metadata("/run/systemd/notify") {
            Ok(s) => s,
            // This test expects a socket like /run/systemd/notify being present.
            Err(_) => return,
        };
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
        /usr/bin/gzip\tbin/gzip\tfile\t755\t0\t0\t1739259005\t35288\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
        let stat = symlink_metadata("/usr/bin/gzip").unwrap();
        let key = get_hardlink_key(&stat);
        let expected_archive = Archive::with_files_and_hardlinks(
            vec![
                File::new(Filetype::Directory, "bin", 0o755, 0, 0, 1681992796),
                File::new(
                    Filetype::Hardlink { key, index: 1 },
                    "bin/gzip",
                    0o755,
                    0,
                    0,
                    1739259005,
                ),
            ],
            HashMap::from([(key, Hardlink::new("/usr/bin/gzip", 35288))]),
        );
        assert_eq!(manifest, Manifest::new(vec![expected_archive], 0o022));
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_from_input_compressed() {
        let input = b"\
        #cpio: zstd -1\n\
        /bin\tbin\tdir\t755\t0\t0\t1681992796\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
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
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_from_input_multiple_uncompressed() {
        let input = b"\
        # This is a comment\n\n\
        #cpio\n\
        /bin\tbin\tdir\t755\t0\t0\t1681992796\n\
        #cpio\n\
        /\t.\tdir\t755\t0\t0\t1732230747\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
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
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_from_input_file_not_found() {
        let input = b"/nonexistent\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let got = Manifest::from_input(input.as_ref(), &mut logger).unwrap_err();
        assert_eq!(got.kind(), ErrorKind::NotFound);
        assert_eq!(
            got.to_string(),
            "line 1: /nonexistent: No such file or directory (os error 2)"
        );
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_from_input_invalid_cpio_directive() {
        let input = b" #cpio \n #cpio:  zstd  \n #cpio something -42  ";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let got = Manifest::from_input(input.as_ref(), &mut logger).unwrap_err();
        assert_eq!(got.kind(), ErrorKind::InvalidInput);
        assert_eq!(
            got.to_string(),
            "line 3: Unknown cpio archive directive: #cpio something -42"
        );
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_from_input_unknown_compressor() {
        let input = b"#cpio: brotli\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let got = Manifest::from_input(input.as_ref(), &mut logger).unwrap_err();
        assert_eq!(got.kind(), ErrorKind::InvalidData);
        assert_eq!(
            got.to_string(),
            "line 1: Unknown compression format: brotli"
        );
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_write_archive_empty_bzip2() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path.join("initrd.img");
        let input = b"#cpio: bzip2 -3\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
        let file = std::fs::File::create(&path).unwrap();
        let size = manifest
            .write_archive(Some(file), None, Some(1754439117), &mut logger)
            .unwrap();
        assert_eq!(size, 124);
        let mut written_file = std::fs::File::open(&path).unwrap();
        let mut output = Vec::new();
        let read = written_file.read_to_end(&mut output).unwrap();
        assert_eq!(
            output,
            b"BZh31AY&SY\x12<\x9e\xb3\0\0\
            \x0c^\0D\0(\0h\x802$\x14\0 \x001\
            L\0\0\xd3(\x0d\x0fH\x88\x17A\xa8\x8eh!$\
            \xe5l\xc6e\xf5\xba\xaf\x8b\xb9\"\x9c(H\x09\x1eOY\x80"
        );
        assert_eq!(read, 66);
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_write_archive_empty_gzip() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path.join("initrd.img");
        let input = b"#cpio: gzip -7\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
        let file = std::fs::File::create(&path).unwrap();
        let size = manifest
            .write_archive(Some(file), None, Some(1754439117), &mut logger)
            .unwrap();
        assert_eq!(size, 124);
        let mut written_file = std::fs::File::open(&path).unwrap();
        let mut output = Vec::new();
        let read = written_file.read_to_end(&mut output).unwrap();
        assert_eq!(
            output,
            b"\x1f\x8b\x08\0\0\0\0\0\0\x03307070\
            4 \x0e\x10\xab\x0e\x1d8\xc1\x18!A\x8e\x9e>\xae\
            A\x8a\x8a\x8a\x0c@\0\0N\xe5\x097|\0\0\0"
        );
        assert_eq!(read, 48);
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_write_archive_empty_lz4() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path.join("initrd.img");
        let input = b"#cpio: lz4 -4\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
        let file = std::fs::File::create(&path).unwrap();
        let size = manifest
            .write_archive(Some(file), None, Some(1754439117), &mut logger)
            .unwrap();
        assert_eq!(size, 124);
        let mut written_file = std::fs::File::open(&path).unwrap();
        let mut output = Vec::new();
        let read = written_file.read_to_end(&mut output).unwrap();
        assert_eq!(
            output,
            b"\x02!L\x18$\0\0\0\x7f0707010\
            \x01\0\x13/10\x01\0#\x14B\x09\0\xe0TR\
            AILER!!!\0\0\0\0"
        );
        assert_eq!(read, 44);
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_write_archive_empty_lzma() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path.join("initrd.img");
        let input = b"#cpio: lzma -1\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
        let file = std::fs::File::create(&path).unwrap();
        let size = manifest
            .write_archive(Some(file), None, Some(1754439117), &mut logger)
            .unwrap();
        assert_eq!(size, 124);
        let mut written_file = std::fs::File::open(&path).unwrap();
        let mut output = Vec::new();
        let read = written_file.read_to_end(&mut output).unwrap();
        assert_eq!(
            output,
            b"]\0\0\x10\0\xff\xff\xff\xff\xff\xff\xff\xff\0\x18\x0d\
            \xdd\x04b3\x02;A\xe5P\x06\xe8\xc4\xa0\xd8\x89Z\
            pL\xa1]\xb0mv\xe7&\xc4o\xff\xfe$\x90\0"
        );
        assert_eq!(read, 48);
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_write_archive_empty_lzop() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path.join("initrd.img");
        let input = b"#cpio: lzop -9\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
        let file = std::fs::File::create(&path).unwrap();
        let got = manifest.write_archive(Some(file), None, Some(1754439117), &mut logger);
        if got
            .as_ref()
            .is_err_and(|e| e.to_string() == "Program 'lzop' not found in PATH.")
        {
            return;
        }
        let size = got.unwrap();
        assert_eq!(size, 124);
        let mut written_file = std::fs::File::open(&path).unwrap();
        let mut output = Vec::new();
        let read = written_file.read_to_end(&mut output).unwrap();
        // The lzop magic is 9 bytes long. Then follows: 3x 16-bit version fields,
        // 2x 8-bit method and level, 2x 32-bit flags and mode, and 64-bit mtime.
        // Then follows the filename (8-bit size) and then the 32-bit CRC32 checksum.
        output.splice(9..15, b"versio".to_owned());
        output.splice(25..33, b"mtime-42".to_owned());
        output.splice(34..38, b"CRCS".to_owned());
        assert_eq!(
            output,
            b"\x89LZO\0\x0d\x0a\x1a\x0aversio\x03\
            \x09\x03\0\0\x0d\0\0\0\0mtime-4\
            2\0CRCS\0\0\0|\0\0\0%\xbc\x7f\
            \x179\x1307F\x0010 \x05\x02\x0010 \
            \x15\x01\0B\xe0\x01\x08TRAILER!!\
            !\0@\0\x11\0\0\0\0\0\0"
        );
        assert_eq!(read, 91);
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_write_archive_empty_xz() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path.join("initrd.img");
        let input = b"#cpio: xz -6\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
        let file = std::fs::File::create(&path).unwrap();
        let size = manifest
            .write_archive(Some(file), None, Some(1754439117), &mut logger)
            .unwrap();
        assert_eq!(size, 124);
        let mut written_file = std::fs::File::open(&path).unwrap();
        let mut output = Vec::new();
        let read = written_file.read_to_end(&mut output).unwrap();
        assert_eq!(
            output,
            b"\xfd7zXZ\0\0\x01i\"\xde6\x02\0!\x01\
            \x16\0\0\0t/\xe5\xa3\xe0\0{\0\x1c]\0\x18\
            \x0d\xdd\x04c\x9d\x8a@Z1\xe4\xcb{\x1c\xc7\xc9\xc0\
            \xef\x917N\x01]\xbd\xd5q\xc8\0\0N\xe5\x097\
            \0\x014|\xcb{\x1f\xc2\x90B\x99\x0d\x01\0\0\0\0\x01YZ"
        );
        assert_eq!(read, 84);
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_write_archive_empty_zstd() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path.join("initrd.img");
        let input = b"#cpio: zstd -2\n";
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let manifest = Manifest::from_input(input.as_ref(), &mut logger).unwrap();
        let file = std::fs::File::create(&path).unwrap();
        let size = manifest
            .write_archive(Some(file), None, Some(1754439117), &mut logger)
            .unwrap();
        assert_eq!(size, 124);
        let mut written_file = std::fs::File::open(&path).unwrap();
        let mut output = Vec::new();
        let read = written_file.read_to_end(&mut output).unwrap();
        assert_eq!(
            output,
            b"(\xb5/\xfd$|\x15\x01\0\xc8070701\
            010B0TRAILER!!!\0\
            \0\0\0\x03\x10\0\x19\xde\x89?F\x95\xfb\x16m"
        );
        assert_eq!(read, 47);
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_manifest_write_fail_compression() {
        let temp_dir = TempDir::new().unwrap();
        let root_dir = File::new(Filetype::Directory, ".", 0o755, 0x333, 0x42, 0x6841897B);
        let archive = Archive::with_files_compressed(vec![root_dir], Compression::Failing);
        let manifest = Manifest::new(vec![archive], 0o022);
        let file = std::fs::File::create(temp_dir.path.join("initrd.img")).unwrap();
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let got = manifest
            .write_archive(Some(file), None, None, &mut logger)
            .unwrap_err();
        assert!(
            matches!(got.kind(), ErrorKind::Other if got.to_string() == "false failed: exit status: 1")
                || matches!(got.kind(), ErrorKind::BrokenPipe)
        );
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_archive_write() {
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
                    target: "usr/sbin".into(),
                },
                "sbin",
                0o777,
                0x336,
                0x45,
                0x62373894,
            ),
            File::new(Filetype::Fifo, "initctl", 0o600, 0x337, 0x46, 0x6862B88A),
            File::new(Filetype::Socket, "notify", 0o777, 0x338, 0x47, 0x681DE2C2),
            File::new(Filetype::EmptyFile, "fstab", 0o644, 0x339, 0x48, 0x6E44C280),
        ]);
        let mut output = Vec::new();
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let size = archive
            .write(&mut output, None, Some(0x6B49D200), 0, &mut logger)
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
            00000008000000000000000000000000000000000000000500000000\
            sbin\0\0usr/sbin\
            07070100000004000011800000033700000046000000016862B88A\
            00000000000000000000000000000000000000000000000800000000\
            initctl\0\0\0\
            070701000000050000C1FF000003380000004700000001681DE2C2\
            00000000000000000000000000000000000000000000000700000000\
            notify\0\0\0\0\
            07070100000006000081A40000033900000048000000016B49D200\
            00000000000000000000000000000000000000000000000600000000\
            fstab\0\
            070701000000000000000000000000000000000000000100000000\
            00000000000000000000000000000000000000000000000B00000000\
            TRAILER!!!\0\0\0\0",
        );
        assert_eq!(size, 952);
        assert_eq!(archive.size(), 952);
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_archive_write_aligned() {
        let tempdir = TempDir::new().unwrap();
        let example = create_text_file_in_tmpdir(
            &tempdir,
            "example.txt",
            b"This is just an example text file!\n",
        );
        let small = create_text_file_in_tmpdir(&tempdir, "small.txt", b"shorter than alignment\n");
        let mut hardlinks = HashMap::new();
        hardlinks.insert(42, Hardlink::with_references(example, 35, 1));
        hardlinks.insert(99, Hardlink::with_references(small, 23, 1));
        let archive = Archive::with_files_and_hardlinks(
            vec![
                File::new(Filetype::Directory, ".", 0o755, 0x333, 0x42, 0x6841897B),
                File::new(
                    Filetype::Hardlink { key: 42, index: 1 },
                    "example",
                    0o644,
                    0x339,
                    0x48,
                    0x6E44C280,
                ),
                File::new(
                    Filetype::Hardlink { key: 99, index: 1 },
                    "small.txt",
                    0o644,
                    0x339,
                    0x48,
                    0x6E44C280,
                ),
            ],
            hardlinks,
        );
        let mut output = Vec::new();
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let size = archive
            .write(
                &mut output,
                NonZeroU32::new(32),
                Some(0x6B49D200),
                0,
                &mut logger,
            )
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&output).unwrap(),
            "07070100000000000041ED0000033300000042000000026841897B\
            00000000000000000000000000000000000000000000000200000000\
            .\0\
            07070100000001000081A40000033900000048000000016B49D200\
            00000023000000000000000000000000000000000000002200000000\
            example\0\0\0\
            \0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\
            This is just an example text file!\n\0\
            07070100000002000081A40000033900000048000000016B49D200\
            00000017000000000000000000000000000000000000000A00000000\
            small.txt\0\
            shorter than alignment\n\0\
            070701000000000000000000000000000000000000000100000000\
            00000000000000000000000000000000000000000000000B00000000\
            TRAILER!!!\0\0\0\0",
        );
        assert_eq!(size, 560);
        assert_eq!(archive.size(), 536);
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_archive_write_hardlinks() {
        let temp_dir = make_temp_dir_with_hardlinks().unwrap();
        let path = temp_dir.path.join("a").to_str().unwrap().to_owned();
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
        let mut logger = Logger::new_vec(LOG_LEVEL_WARNING);
        let size = archive
            .write(&mut output, None, None, 0, &mut logger)
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
        assert_eq!(size, 356);
        assert_eq!(archive.size(), 356);
        assert_eq!(logger.get_logs(), "");
    }
}
