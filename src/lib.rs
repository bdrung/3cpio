// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::collections::{BTreeMap, HashMap};
use std::fs::{
    create_dir, hard_link, remove_file, set_permissions, symlink_metadata, File, OpenOptions,
    Permissions,
};
use std::io::prelude::*;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::io::SeekFrom;
use std::os::unix::fs::{chown, fchown, lchown, symlink, PermissionsExt};
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::time::SystemTime;

use crate::libc::{set_modified, strftime_local};

mod libc;

const CPIO_HEADER_LENGTH: u32 = 110;
const CPIO_MAGIC_NUMBER: [u8; 6] = *b"070701";
const PIPE_SIZE: usize = 65536;

const MODE_PERMISSION_MASK: u32 = 0o007_777;
const MODE_FILETYPE_MASK: u32 = 0o770_000;
const FILETYPE_FIFO: u32 = 0o010_000;
const FILETYPE_CHARACTER_DEVICE: u32 = 0o020_000;
const FILETYPE_DIRECTORY: u32 = 0o040_000;
const FILETYPE_BLOCK_DEVICE: u32 = 0o060_000;
const FILETYPE_REGULAR_FILE: u32 = 0o100_000;
const FILETYPE_SYMLINK: u32 = 0o120_000;
const FILETYPE_SOCKET: u32 = 0o140_000;

pub const LOG_LEVEL_WARNING: u32 = 5;
pub const LOG_LEVEL_INFO: u32 = 7;
pub const LOG_LEVEL_DEBUG: u32 = 8;

pub trait SeekForward {
    /// Seek forward to an offset, in bytes, in a stream.
    ///
    /// A seek beyond the end of a stream is allowed, but behavior is defined
    /// by the implementation.
    ///
    /// # Errors
    ///
    /// Seeking can fail, for example because it might involve flushing a buffer.
    fn seek_forward(&mut self, offset: u64) -> Result<()>;
}

impl SeekForward for File {
    fn seek_forward(&mut self, offset: u64) -> Result<()> {
        self.seek(SeekFrom::Current(offset.try_into().unwrap()))?;
        Ok(())
    }
}

impl SeekForward for ChildStdout {
    fn seek_forward(&mut self, offset: u64) -> Result<()> {
        let mut seek_reader = self.take(offset);
        let mut remaining: usize = offset.try_into().unwrap();
        let mut buffer = [0; PIPE_SIZE];
        while remaining > 0 {
            let read = seek_reader.read(&mut buffer)?;
            remaining -= read;
        }
        Ok(())
    }
}

impl SeekForward for &[u8] {
    fn seek_forward(&mut self, offset: u64) -> Result<()> {
        let mut seek_reader = std::io::Read::take(self, offset);
        let mut buffer = Vec::new();
        let read = seek_reader.read_to_end(&mut buffer)?;
        if read < offset.try_into().unwrap() {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                format!("read only {} bytes, but {} wanted", read, offset),
            ));
        }
        Ok(())
    }
}

struct CpioFilenameReader<'a, R: Read + SeekForward> {
    file: &'a mut R,
}

impl<'a, R: Read + SeekForward> Iterator for CpioFilenameReader<'a, R> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        match read_filename_from_next_cpio_object(self.file) {
            Ok(filename) => {
                if filename == "TRAILER!!!" {
                    None
                } else {
                    Some(Ok(filename))
                }
            }
            x => Some(x),
        }
    }
}

#[derive(Debug, PartialEq)]
struct Header {
    ino: u32,
    mode: u32,
    uid: u32,
    gid: u32,
    nlink: u32,
    mtime: u32,
    filesize: u32,
    major: u32,
    minor: u32,
    // unused
    //rmajor: u32,
    //rminor: u32,
    filename: String,
}

impl Header {
    // Return major and minor combined as u64
    fn dev(&self) -> u64 {
        u64::from(self.major) << 32 | u64::from(self.minor)
    }

    fn mode_perm(&self) -> u32 {
        self.mode & MODE_PERMISSION_MASK
    }

    // ls-style ASCII representation of the mode
    fn mode_string(&self) -> [u8; 10] {
        [
            match self.mode & MODE_FILETYPE_MASK {
                FILETYPE_FIFO => b'p',
                FILETYPE_CHARACTER_DEVICE => b'c',
                FILETYPE_DIRECTORY => b'd',
                FILETYPE_BLOCK_DEVICE => b'b',
                FILETYPE_REGULAR_FILE => b'-',
                FILETYPE_SYMLINK => b'l',
                FILETYPE_SOCKET => b's',
                _ => b'?',
            },
            if self.mode & 0o400 != 0 { b'r' } else { b'-' },
            if self.mode & 0o200 != 0 { b'w' } else { b'-' },
            match self.mode & 0o4100 {
                0o4100 => b's', // set-uid and executable by owner
                0o4000 => b'S', // set-uid but not executable by owner
                0o0100 => b'x',
                _ => b'-',
            },
            if self.mode & 0o040 != 0 { b'r' } else { b'-' },
            if self.mode & 0o020 != 0 { b'w' } else { b'-' },
            match self.mode & 0o2010 {
                0o2010 => b's', // set-gid and executable by group
                0o2000 => b'S', // set-gid but not executable by group
                0o0010 => b'x',
                _ => b'-',
            },
            if self.mode & 0o004 != 0 { b'r' } else { b'-' },
            if self.mode & 0o002 != 0 { b'w' } else { b'-' },
            match self.mode & 0o1001 {
                0o1001 => b't', // sticky and executable by others
                0o1000 => b'T', // sticky but not executable by others
                0o0001 => b'x',
                _ => b'-',
            },
        ]
    }

    fn permission(&self) -> Permissions {
        PermissionsExt::from_mode(self.mode & MODE_PERMISSION_MASK)
    }

    fn ino_and_dev(&self) -> u128 {
        u128::from(self.ino) << 64 | u128::from(self.dev())
    }

    fn mark_seen(&self, seen_files: &mut SeenFiles) {
        seen_files.insert(self.ino_and_dev(), self.filename.clone());
    }

    fn long_format(&self, now: i64, user_group_cache: &mut UserGroupCache) -> Result<String> {
        let user = match user_group_cache.get_user(self.uid)? {
            Some(name) => name,
            None => self.uid.to_string(),
        };
        let group = match user_group_cache.get_group(self.gid)? {
            Some(name) => name,
            None => self.gid.to_string(),
        };
        let mode_string = self.mode_string();
        Ok(format!(
            "{} {:>3} {:<8} {:<8} {:>8} {} {}",
            std::str::from_utf8(&mode_string).unwrap(),
            self.nlink,
            user,
            group,
            self.filesize,
            format_time(self.mtime, now)?,
            self.filename
        ))
    }

    fn read_symlink_target<R: Read>(&self, file: &mut R) -> Result<String> {
        let align = align_to_4_bytes(self.filesize);
        let mut target_bytes = vec![0u8; (self.filesize + align).try_into().unwrap()];
        file.read_exact(&mut target_bytes)?;
        target_bytes.truncate(self.filesize.try_into().unwrap());
        // TODO: propper name reading handling
        let target = std::str::from_utf8(&target_bytes).unwrap();
        Ok(target.into())
    }

    fn skip_file_content<R: SeekForward>(&self, file: &mut R) -> Result<()> {
        let skip = self.filesize + align_to_4_bytes(self.filesize);
        file.seek_forward(skip.into())?;
        Ok(())
    }
}

struct UserGroupCache {
    user_cache: HashMap<u32, Option<String>>,
    group_cache: HashMap<u32, Option<String>>,
}

impl UserGroupCache {
    fn new() -> Self {
        Self {
            user_cache: HashMap::new(),
            group_cache: HashMap::new(),
        }
    }

    /// Translate user ID (UID) to user name and cache result.
    fn get_user(&mut self, uid: u32) -> Result<Option<String>> {
        match self.user_cache.get(&uid) {
            Some(name) => Ok(name.clone()),
            None => {
                let name = libc::getpwuid_name(uid)?;
                self.user_cache.insert(uid, name.clone());
                Ok(name)
            }
        }
    }

    /// Translate group ID (GID) to group name and cache result.
    fn get_group(&mut self, gid: u32) -> Result<Option<String>> {
        match self.group_cache.get(&gid) {
            Some(name) => Ok(name.clone()),
            None => {
                let name = libc::getgrgid_name(gid)?;
                self.group_cache.insert(gid, name.clone());
                Ok(name)
            }
        }
    }
}

/// Format the time in a similar way to coreutils' ls command.
fn format_time(timestamp: u32, now: i64) -> Result<String> {
    // Logic from coreutils ls command:
    // Consider a time to be recent if it is within the past six months.
    // A Gregorian year has 365.2425 * 24 * 60 * 60 == 31556952 seconds
    // on the average.
    let recent = now - i64::from(timestamp) <= 15778476;
    if recent {
        strftime_local("%b %e %H:%M", timestamp)
    } else {
        strftime_local("%b %e  %Y", timestamp)
    }
}

// TODO: Document hardlink structure
type SeenFiles = HashMap<u128, String>;

struct Extractor {
    seen_files: SeenFiles,
    mtimes: BTreeMap<String, i64>,
}

impl Extractor {
    fn new() -> Extractor {
        Extractor {
            seen_files: SeenFiles::new(),
            mtimes: BTreeMap::new(),
        }
    }

    fn set_modified_times(&self, log_level: u32) -> Result<()> {
        for (path, mtime) in self.mtimes.iter().rev() {
            if log_level >= LOG_LEVEL_DEBUG {
                writeln!(std::io::stderr(), "set mtime {} for '{}'", mtime, path)?;
            };
            set_modified(path, *mtime)?;
        }
        Ok(())
    }
}

fn align_to_4_bytes(length: u32) -> u32 {
    let unaligned = length % 4;
    if unaligned == 0 {
        0
    } else {
        4 - unaligned
    }
}

fn hex_str_to_u32(bytes: &[u8]) -> Result<u32> {
    let s = match std::str::from_utf8(bytes) {
        Err(_) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid hexadecimal value '{}'", bytes.escape_ascii()),
            ))
        }
        Ok(value) => value,
    };
    match u32::from_str_radix(s, 16) {
        Err(_) => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid hexadecimal value '{}'", s),
        )),
        Ok(value) => Ok(value),
    }
}

fn read_filename<R: Read>(file: &mut R, namesize: u32) -> Result<String> {
    let header_align = align_to_4_bytes(CPIO_HEADER_LENGTH + namesize);
    let mut filename_bytes = vec![0u8; (namesize + header_align).try_into().unwrap()];
    let filename_length: usize = (namesize - 1).try_into().unwrap();
    file.read_exact(&mut filename_bytes)?;
    if filename_bytes[filename_length] != 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Entry name '{:?}' is not NULL-terminated",
                &filename_bytes[0..filename_length]
            ),
        ));
    }
    filename_bytes.truncate(filename_length);
    // TODO: propper name reading handling
    let filename = std::str::from_utf8(&filename_bytes).unwrap();
    Ok(filename.to_string())
}

fn check_begins_with_cpio_magic_header(header: &[u8]) -> std::io::Result<()> {
    if header[0..6] != CPIO_MAGIC_NUMBER {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid CPIO magic number '{}'. Expected {}",
                &header[0..6].escape_ascii(),
                std::str::from_utf8(&CPIO_MAGIC_NUMBER).unwrap(),
            ),
        ));
    }
    Ok(())
}

fn read_cpio_header<R: Read>(file: &mut R) -> std::io::Result<Header> {
    let mut buffer = [0; CPIO_HEADER_LENGTH as usize];
    file.read_exact(&mut buffer)?;
    check_begins_with_cpio_magic_header(&buffer)?;
    let namesize = hex_str_to_u32(&buffer[94..102])?;
    let filename = read_filename(file, namesize)?;
    Ok(Header {
        ino: hex_str_to_u32(&buffer[6..14])?,
        mode: hex_str_to_u32(&buffer[14..22])?,
        uid: hex_str_to_u32(&buffer[22..30])?,
        gid: hex_str_to_u32(&buffer[30..38])?,
        nlink: hex_str_to_u32(&buffer[38..46])?,
        mtime: hex_str_to_u32(&buffer[46..54])?,
        filesize: hex_str_to_u32(&buffer[54..62])?,
        major: hex_str_to_u32(&buffer[62..70])?,
        minor: hex_str_to_u32(&buffer[70..78])?,
        //rmajor: hex_str_to_u32(&buffer[78..86])?,
        //rminor: hex_str_to_u32(&buffer[86..94])?,
        filename,
    })
}

/// Read only the file name from the next cpio object.
///
/// Read the next cpio object header, check the magic, skip the file data.
/// Return the file name.
fn read_filename_from_next_cpio_object<R: Read + SeekForward>(file: &mut R) -> Result<String> {
    let mut header = [0; CPIO_HEADER_LENGTH as usize];
    file.read_exact(&mut header)?;
    check_begins_with_cpio_magic_header(&header)?;
    let filesize = hex_str_to_u32(&header[54..62])?;
    let namesize = hex_str_to_u32(&header[94..102])?;
    let filename = read_filename(file, namesize)?;

    let skip = filesize + align_to_4_bytes(filesize);
    file.seek_forward(skip.into())?;
    Ok(filename)
}

fn read_magic_header<R: Read + Seek>(file: &mut R) -> Option<Result<Command>> {
    let mut buffer = [0; 4];
    while buffer == [0, 0, 0, 0] {
        match file.read_exact(&mut buffer) {
            Ok(()) => {}
            Err(e) => match e.kind() {
                ErrorKind::UnexpectedEof => return None,
                _ => return Some(Err(e)),
            },
        };
    }
    let command = match buffer {
        [0x42, 0x5A, 0x68, _] => {
            let mut cmd = Command::new("bzip2");
            cmd.arg("-cd");
            cmd
        }
        [0x30, 0x37, 0x30, 0x37] => Command::new("cpio"),
        [0x1F, 0x8B, _, _] => {
            let mut cmd = Command::new("gzip");
            cmd.arg("-cd");
            cmd
        }
        // Different magic numbers (little endian) for lz4:
        // v0.1-v0.9: 0x184C2102
        // v1.0-v1.3: 0x184C2103
        // v1.4+: 0x184D2204
        [0x02, 0x21, 0x4C, 0x18] | [0x03, 0x21, 0x4C, 0x18] | [0x04, 0x22, 0x4D, 0x18] => {
            let mut cmd = Command::new("lz4");
            cmd.arg("-cd");
            cmd
        }
        // Full magic number for lzop: [0x89, 0x4C, 0x5A, 0x4F, 0x00, 0x0D, 0x0A, 0x1A, 0x0A]
        [0x89, 0x4C, 0x5A, 0x4F] => {
            let mut cmd = Command::new("lzop");
            cmd.arg("-cd");
            cmd
        }
        // Full magic number for xz: [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]
        [0xFD, 0x37, 0x7A, 0x58] => {
            let mut cmd = Command::new("xz");
            cmd.arg("-cd");
            cmd
        }
        [0x28, 0xB5, 0x2F, 0xFD] => {
            let mut cmd = Command::new("zstd");
            cmd.arg("-cdq");
            cmd
        }
        _ => {
            return Some(Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Failed to determine magic number: 0x{:02x}{:02x}{:02x}{:02x} (big endian)",
                    buffer[0], buffer[1], buffer[2], buffer[3]
                ),
            )));
        }
    };
    match file.seek(SeekFrom::Current(-4)) {
        Ok(_) => {}
        Err(e) => {
            return Some(Err(e));
        }
    };
    Some(Ok(command))
}

fn decompress(command: &mut Command, file: File) -> Result<ChildStdout> {
    // TODO: Propper error message if spawn fails
    let cmd = command.stdin(file).stdout(Stdio::piped()).spawn()?;
    // TODO: Should unwrap be replaced by returning Result?
    Ok(cmd.stdout.unwrap())
}

fn read_cpio_and_print_filenames<R: Read + SeekForward, W: Write>(
    file: &mut R,
    out: &mut W,
) -> Result<()> {
    let cpio = CpioFilenameReader { file };
    for f in cpio {
        let filename = f?;
        writeln!(out, "{}", filename)?;
    }
    Ok(())
}

fn read_cpio_and_print_long_format<R: Read + SeekForward, W: Write>(
    file: &mut R,
    out: &mut W,
    now: i64,
    user_group_cache: &mut UserGroupCache,
) -> Result<()> {
    loop {
        let header = match read_cpio_header(file) {
            Ok(header) => {
                if header.filename == "TRAILER!!!" {
                    break;
                } else {
                    header
                }
            }
            Err(e) => return Err(e),
        };

        let long_format = header.long_format(now, user_group_cache)?;
        if header.mode & MODE_FILETYPE_MASK == FILETYPE_SYMLINK {
            let target = header.read_symlink_target(file)?;
            writeln!(out, "{} -> {}", long_format, target)?;
        } else {
            header.skip_file_content(file)?;
            writeln!(out, "{}", long_format)?;
        }
    }
    Ok(())
}

fn create_dir_ignore_existing<P: AsRef<std::path::Path>>(path: P) -> Result<()> {
    if let Err(e) = create_dir(&path) {
        if e.kind() != ErrorKind::AlreadyExists {
            return Err(e);
        }
        let stat = symlink_metadata(&path)?;
        if !stat.is_dir() {
            remove_file(&path)?;
            create_dir(&path)?;
        }
    };
    Ok(())
}

fn write_directory(
    header: &Header,
    preserve_permissions: bool,
    log_level: u32,
    mtimes: &mut BTreeMap<String, i64>,
) -> Result<()> {
    if header.filesize != 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid size for directory '{}': {} bytes instead of 0.",
                header.filename, header.filesize
            ),
        ));
    };
    if log_level >= LOG_LEVEL_DEBUG {
        writeln!(
            std::io::stderr(),
            "Creating directory '{}' with mode {:o}{}",
            header.filename,
            header.mode_perm(),
            if preserve_permissions {
                format!(" and owner {}:{}", header.uid, header.gid)
            } else {
                String::new()
            },
        )?;
    };
    create_dir_ignore_existing(&header.filename)?;
    if preserve_permissions {
        chown(&header.filename, Some(header.uid), Some(header.gid))?;
    }
    set_permissions(&header.filename, header.permission())?;
    mtimes.insert(header.filename.to_string(), header.mtime.into());
    Ok(())
}

fn from_mtime(mtime: u32) -> SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime.into())
}

fn try_get_hard_link_target<'a>(header: &Header, seen_files: &'a SeenFiles) -> Option<&'a String> {
    if header.nlink <= 1 {
        return None;
    }
    seen_files.get(&header.ino_and_dev())
}

fn write_file<R: Read + SeekForward>(
    cpio_file: &mut R,
    header: &Header,
    preserve_permissions: bool,
    seen_files: &mut SeenFiles,
    log_level: u32,
) -> Result<()> {
    let mut file;
    if let Some(target) = try_get_hard_link_target(header, seen_files) {
        if log_level >= LOG_LEVEL_DEBUG {
            writeln!(
                std::io::stderr(),
                "Creating hard-link '{}' -> '{}' with permission {:o}{} and {} bytes",
                header.filename,
                target,
                header.mode_perm(),
                if preserve_permissions {
                    format!(" and owner {}:{}", header.uid, header.gid)
                } else {
                    String::new()
                },
                header.filesize,
            )?;
        };
        if let Err(e) = hard_link(target, &header.filename) {
            match e.kind() {
                ErrorKind::AlreadyExists => {
                    remove_file(&header.filename)?;
                    hard_link(target, &header.filename)?;
                }
                _ => {
                    return Err(e);
                }
            }
        }
        file = OpenOptions::new().write(true).open(&header.filename)?
    } else {
        if log_level >= LOG_LEVEL_DEBUG {
            writeln!(
                std::io::stderr(),
                "Creating file '{}' with permission {:o}{} and {} bytes",
                header.filename,
                header.mode_perm(),
                if preserve_permissions {
                    format!(" and owner {}:{}", header.uid, header.gid)
                } else {
                    String::new()
                },
                header.filesize,
            )?;
        };
        file = File::create(&header.filename)?
    };
    header.mark_seen(seen_files);
    let mut reader = cpio_file.take(header.filesize.into());
    // TODO: check writing hard-link with length == 0
    // TODO: check overwriting existing files/hardlinks
    let written = std::io::copy(&mut reader, &mut file)?;
    if written != header.filesize.into() {
        return Err(Error::new(
            ErrorKind::Other,
            format!(
                "Wrong amound of bytes written to '{}': {} != {}.",
                header.filename, written, header.filesize
            ),
        ));
    }
    let skip = align_to_4_bytes(header.filesize);
    cpio_file.seek_forward(skip.into())?;
    if preserve_permissions {
        fchown(&file, Some(header.uid), Some(header.gid))?;
    }
    file.set_permissions(header.permission())?;
    file.set_modified(from_mtime(header.mtime))?;
    Ok(())
}

fn write_symbolic_link<R: Read + SeekForward>(
    cpio_file: &mut R,
    header: &Header,
    preserve_permissions: bool,
    log_level: u32,
) -> Result<()> {
    let target = header.read_symlink_target(cpio_file)?;
    if log_level >= LOG_LEVEL_DEBUG {
        writeln!(
            std::io::stderr(),
            "Creating symlink '{}' -> '{}' with mode {:o}",
            header.filename,
            &target,
            header.mode_perm(),
        )?;
    };
    if let Err(e) = symlink(&target, &header.filename) {
        match e.kind() {
            ErrorKind::AlreadyExists => {
                remove_file(&header.filename)?;
                symlink(&target, &header.filename)?;
            }
            _ => {
                return Err(e);
            }
        }
    }
    if preserve_permissions {
        lchown(&header.filename, Some(header.uid), Some(header.gid))?;
    }
    if header.mode_perm() != 0o777 {
        return Err(Error::new(
            ErrorKind::Unsupported,
            format!(
                "Symlink '{}' has mode {:o}, but only mode 777 is supported.",
                header.filename,
                header.mode_perm()
            ),
        ));
    };
    set_modified(&header.filename, header.mtime.into())?;
    Ok(())
}

fn read_cpio_and_extract<R: Read + SeekForward>(
    file: &mut R,
    preserve_permissions: bool,
    log_level: u32,
) -> Result<()> {
    let mut extractor = Extractor::new();
    loop {
        let header = match read_cpio_header(file) {
            Ok(header) => {
                if header.filename == "TRAILER!!!" {
                    break;
                } else {
                    header
                }
            }
            Err(e) => return Err(e),
        };

        if log_level >= LOG_LEVEL_DEBUG {
            writeln!(std::io::stderr(), "{:?}", header)?;
        } else if log_level >= LOG_LEVEL_INFO {
            writeln!(std::io::stderr(), "{}", header.filename)?;
        }

        match header.mode & MODE_FILETYPE_MASK {
            FILETYPE_DIRECTORY => write_directory(
                &header,
                preserve_permissions,
                log_level,
                &mut extractor.mtimes,
            )?,
            FILETYPE_REGULAR_FILE => write_file(
                file,
                &header,
                preserve_permissions,
                &mut extractor.seen_files,
                log_level,
            )?,
            FILETYPE_SYMLINK => {
                write_symbolic_link(file, &header, preserve_permissions, log_level)?
            }
            FILETYPE_FIFO | FILETYPE_CHARACTER_DEVICE | FILETYPE_BLOCK_DEVICE | FILETYPE_SOCKET => {
                unimplemented!(
                    "Mode {:o} (file {}) not implemented. Please open a bug report requesting support for this type.",
                    header.mode, header.filename
                )
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Invalid/unknown filetype {:o}: {}",
                        header.mode, header.filename
                    ),
                ))
            }
        };
    }
    extractor.set_modified_times(log_level)?;
    Ok(())
}

fn seek_to_cpio_end(file: &mut File) -> Result<()> {
    let cpio = CpioFilenameReader { file };
    for f in cpio {
        f?;
    }
    Ok(())
}

pub fn examine_cpio_content<W: Write>(mut file: File, out: &mut W) -> Result<()> {
    loop {
        let command = match read_magic_header(&mut file) {
            None => return Ok(()),
            Some(x) => x?,
        };
        writeln!(
            out,
            "{}\t{}",
            file.stream_position()?,
            command.get_program().to_str().unwrap()
        )?;
        if command.get_program() == "cpio" {
            seek_to_cpio_end(&mut file)?;
        } else {
            break;
        }
    }
    Ok(())
}

pub fn extract_cpio_archive(
    mut file: File,
    preserve_permissions: bool,
    subdir: Option<String>,
    log_level: u32,
) -> Result<()> {
    let mut count = 1;
    let base_dir = std::env::current_dir()?;
    loop {
        if let Some(ref s) = subdir {
            let mut dir = base_dir.clone();
            dir.push(format!("{s}{count}"));
            create_dir_ignore_existing(&dir)?;
            std::env::set_current_dir(&dir)?;
        }
        let mut command = match read_magic_header(&mut file) {
            None => return Ok(()),
            Some(x) => x?,
        };
        if command.get_program() == "cpio" {
            read_cpio_and_extract(&mut file, preserve_permissions, log_level)?;
        } else {
            let mut decompressed = decompress(&mut command, file)?;
            read_cpio_and_extract(&mut decompressed, preserve_permissions, log_level)?;
            break;
        }
        count += 1;
    }
    Ok(())
}

pub fn list_cpio_content<W: Write>(mut file: File, out: &mut W, log_level: u32) -> Result<()> {
    let mut user_group_cache = UserGroupCache::new();
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .try_into()
        .unwrap();
    loop {
        let mut command = match read_magic_header(&mut file) {
            None => return Ok(()),
            Some(x) => x?,
        };
        if command.get_program() == "cpio" {
            if log_level >= LOG_LEVEL_INFO {
                read_cpio_and_print_long_format(&mut file, out, now, &mut user_group_cache)?;
            } else {
                read_cpio_and_print_filenames(&mut file, out)?;
            }
        } else {
            let mut decompressed = decompress(&mut command, file)?;
            if log_level >= LOG_LEVEL_INFO {
                read_cpio_and_print_long_format(
                    &mut decompressed,
                    out,
                    now,
                    &mut user_group_cache,
                )?;
            } else {
                read_cpio_and_print_filenames(&mut decompressed, out)?;
            }
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn getgid() -> u32 {
        unsafe { ::libc::getgid() }
    }

    fn getuid() -> u32 {
        unsafe { ::libc::getuid() }
    }

    extern "C" {
        fn tzset();
    }

    impl UserGroupCache {
        fn insert_test_data(&mut self) {
            self.user_cache.insert(1000, Some("user".into()));
            self.group_cache.insert(123, Some("whoopsie".into()));
            self.group_cache.insert(2000, None);
        }
    }

    #[test]
    fn test_align_to_4_bytes() {
        assert_eq!(align_to_4_bytes(110), 2);
    }

    #[test]
    fn test_align_to_4_bytes_is_aligned() {
        assert_eq!(align_to_4_bytes(32), 0);
    }

    #[test]
    fn test_header_long_format_file() {
        let mut user_group_cache = UserGroupCache::new();
        user_group_cache.insert_test_data();
        let header = Header {
            ino: 2,
            mode: 0o100644,
            uid: 1000,
            gid: 2000,
            nlink: 1,
            mtime: 1678200175,
            filesize: 8,
            major: 0,
            minor: 0,
            filename: "path/file".into(),
        };
        let long_format = header
            .long_format(1720737584, &mut user_group_cache)
            .unwrap();
        assert_eq!(
            long_format,
            "-rw-r--r--   1 user     2000            8 Mar  7  2023 path/file"
        );
    }

    #[test]
    fn test_header_long_format_directory() {
        let mut user_group_cache = UserGroupCache::new();
        user_group_cache.insert_test_data();
        let header = Header {
            ino: 1,
            mode: 0o43777,
            uid: 0,
            gid: 123,
            nlink: 2,
            mtime: 1722213380,
            filesize: 0,
            major: 0,
            minor: 0,
            filename: "/var/crash".into(),
        };
        env::set_var("TZ", "UTC");
        unsafe { tzset() };
        let long_format = header
            .long_format(1722389471, &mut user_group_cache)
            .unwrap();
        assert_eq!(
            long_format,
            "drwxrwsrwt   2 root     whoopsie        0 Jul 29 00:36 /var/crash"
        );
    }

    #[test]
    fn test_hex_str_to_u32() {
        let value = hex_str_to_u32(b"000003E8").unwrap();
        assert_eq!(value, 1000);
    }

    #[test]
    fn test_hex_str_to_u32_invalid_hex() {
        let got = hex_str_to_u32(b"something").unwrap_err();
        assert_eq!(got.kind(), ErrorKind::InvalidData);
        assert_eq!(got.to_string(), "Invalid hexadecimal value 'something'");
    }

    #[test]
    fn test_hex_str_to_u32_invalid_utf8() {
        let got = hex_str_to_u32(b"no\xc3\x28utf8").unwrap_err();
        assert_eq!(got.kind(), ErrorKind::InvalidData);
        assert_eq!(got.to_string(), "Invalid hexadecimal value 'no\\xc3(utf8'");
    }

    #[test]
    fn test_read_cpio_and_print_long_format_file() {
        // Wrapped before mtime and filename
        let cpio_data = b"070701000036E4000081A4000000000000000000000001\
        66A3285300000041000000000000002400000000000000000000000D00000000\
        conf/modules\0\0\
        linear\nmultipath\nraid0\nraid1\nraid456\nraid5\nraid6\nraid10\nefivarfs\0\0\0\0\
        0707010000000000000000000000000000000000000001\
        0000000000000000000000000000000000000000000000000000000B00000000\
        TRAILER!!!\0\0\0\0";
        let mut output = Vec::new();
        let mut user_group_cache = UserGroupCache::new();
        user_group_cache.insert_test_data();
        env::set_var("TZ", "UTC");
        unsafe { tzset() };
        read_cpio_and_print_long_format(
            &mut cpio_data.as_ref(),
            &mut output,
            1722645915,
            &mut user_group_cache,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "-rw-r--r--   1 root     root           65 Jul 26 04:38 conf/modules\n"
        );
    }

    #[test]
    fn test_read_cpio_and_print_long_format_symlink() {
        // Wrapped before mtime and filename
        let cpio_data = b"0707010000000D0000A1FF000000000000000000000001\
        6237389400000007000000000000000000000000000000000000000400000000\
        bin\0\0\0usr/bin\0\
        0707010000000000000000000000000000000000000001\
        0000000000000000000000000000000000000000000000000000000B00000000\
        TRAILER!!!\0\0\0\0";
        let mut output = Vec::new();
        let mut user_group_cache = UserGroupCache::new();
        user_group_cache.insert_test_data();
        read_cpio_and_print_long_format(
            &mut cpio_data.as_ref(),
            &mut output,
            1722645915,
            &mut user_group_cache,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "lrwxrwxrwx   1 root     root            7 Mar 20  2022 bin -> usr/bin\n"
        );
    }

    #[test]
    fn test_read_cpio_header() {
        // Wrapped before mtime and filename
        let cpio_data = b"07070100000002000081B4000003E8000007D000000001\
            661BE5C600000008000000000000000000000000000000000000000A00000000\
            path/file\0content\0";
        let header = read_cpio_header(&mut cpio_data.as_ref()).unwrap();
        assert_eq!(
            header,
            Header {
                ino: 2,
                mode: 0o100664,
                uid: 1000,
                gid: 2000,
                nlink: 1,
                mtime: 1713104326,
                filesize: 8,
                major: 0,
                minor: 0,
                filename: "path/file".into()
            }
        )
    }

    #[test]
    fn test_read_cpio_header_invalid_magic_number() {
        let invalid_data = b"abc\tefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\
            abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let got = read_cpio_header(&mut invalid_data.as_ref()).unwrap_err();
        assert_eq!(got.kind(), ErrorKind::InvalidData);
        assert_eq!(
            got.to_string(),
            "Invalid CPIO magic number 'abc\\tef'. Expected 070701"
        );
    }

    #[test]
    fn test_write_directory_with_setuid() {
        let mut mtimes = BTreeMap::new();
        let header = Header {
            ino: 1,
            mode: 0o43_777,
            uid: getuid(),
            gid: getgid(),
            nlink: 0,
            mtime: 1720081471,
            filesize: 0,
            major: 0,
            minor: 0,
            filename: "./directory_with_setuid".into(),
        };
        write_directory(&header, true, LOG_LEVEL_WARNING, &mut mtimes).unwrap();

        let attr = std::fs::metadata("directory_with_setuid").unwrap();
        assert!(attr.is_dir());
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        std::fs::remove_dir("directory_with_setuid").unwrap();

        let mut expected_mtimes: BTreeMap<String, i64> = BTreeMap::new();
        expected_mtimes.insert("./directory_with_setuid".into(), header.mtime.into());
        assert_eq!(mtimes, expected_mtimes);
    }

    #[test]
    fn test_write_file_with_setuid() {
        let mut seen_files = SeenFiles::new();
        let header = Header {
            ino: 1,
            mode: 0o104_755,
            uid: getuid(),
            gid: getgid(),
            nlink: 0,
            mtime: 1720081471,
            filesize: 9,
            major: 0,
            minor: 0,
            filename: "./file_with_setuid".into(),
        };
        let cpio = b"!/bin/sh\n\0\0\0";
        write_file(
            &mut cpio.as_ref(),
            &header,
            true,
            &mut seen_files,
            LOG_LEVEL_WARNING,
        )
        .unwrap();

        let attr = std::fs::metadata("file_with_setuid").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.is_file());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        std::fs::remove_file("file_with_setuid").unwrap();
    }

    #[test]
    fn test_write_symbolic_link() {
        let header = Header {
            ino: 1,
            mode: 0o120_777,
            uid: getuid(),
            gid: getgid(),
            nlink: 0,
            mtime: 1721427072,
            filesize: 12,
            major: 0,
            minor: 0,
            filename: "./dead_symlink".into(),
        };
        let cpio = b"/nonexistent";
        write_symbolic_link(&mut cpio.as_ref(), &header, true, LOG_LEVEL_WARNING).unwrap();

        let attr = std::fs::symlink_metadata("dead_symlink").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.is_symlink());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        std::fs::remove_file("dead_symlink").unwrap();
    }
}
