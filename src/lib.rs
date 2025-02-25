// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::collections::{BTreeMap, HashMap};
use std::fs::{
    create_dir, hard_link, remove_file, set_permissions, symlink_metadata, File, OpenOptions,
};
use std::io::prelude::*;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::io::SeekFrom;
use std::os::unix::fs::{chown, fchown, lchown, symlink};
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::time::SystemTime;

use crate::header::*;
use crate::libc::{mknod, set_modified, strftime_local};
use crate::seek_forward::SeekForward;

mod header;
mod libc;
mod seek_forward;

pub const LOG_LEVEL_WARNING: u32 = 5;
pub const LOG_LEVEL_INFO: u32 = 7;
pub const LOG_LEVEL_DEBUG: u32 = 8;

struct CpioFilenameReader<'a, R: Read + SeekForward> {
    file: &'a mut R,
}

impl<R: Read + SeekForward> Iterator for CpioFilenameReader<'_, R> {
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
        strftime_local(b"%b %e %H:%M\0", timestamp)
    } else {
        strftime_local(b"%b %e  %Y\0", timestamp)
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

/// Read only the file name from the next cpio object.
///
/// Read the next cpio object header, check the magic, skip the file data.
/// Return the file name.
fn read_filename_from_next_cpio_object<R: Read + SeekForward>(file: &mut R) -> Result<String> {
    let (filesize, filename) = Header::read_only_filesize_and_filename(file)?;
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
        [0x5D, _, _, _] => {
            let mut cmd = Command::new("lzma");
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
                    "Failed to determine CPIO or compression magic number: 0x{:02x}{:02x}{:02x}{:02x} (big endian)",
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
    let cmd = command
        .stdin(file)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => Error::other(format!(
                "Program '{}' not found in PATH.",
                command.get_program().to_str().unwrap()
            )),
            _ => e,
        })?;
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
    // Files can have the same mtime (especially when using SOURCE_DATE_EPOCH).
    // Cache the time string of the last mtime.
    let mut last_mtime = 0;
    let mut time_string: String = "".into();
    loop {
        let header = match Header::read(file) {
            Ok(header) => {
                if header.filename == "TRAILER!!!" {
                    break;
                } else {
                    header
                }
            }
            Err(e) => return Err(e),
        };

        let user = match user_group_cache.get_user(header.uid)? {
            Some(name) => name,
            None => header.uid.to_string(),
        };
        let group = match user_group_cache.get_group(header.gid)? {
            Some(name) => name,
            None => header.gid.to_string(),
        };
        let mode_string = header.mode_string();
        if header.mtime != last_mtime || time_string.is_empty() {
            last_mtime = header.mtime;
            time_string = format_time(header.mtime, now)?;
        };

        match header.mode & MODE_FILETYPE_MASK {
            FILETYPE_SYMLINK => {
                let target = header.read_symlink_target(file)?;
                writeln!(
                    out,
                    "{} {:>3} {:<8} {:<8} {:>8} {} {} -> {}",
                    std::str::from_utf8(&mode_string).unwrap(),
                    header.nlink,
                    user,
                    group,
                    header.filesize,
                    time_string,
                    header.filename,
                    target
                )?;
            }
            FILETYPE_BLOCK_DEVICE | FILETYPE_CHARACTER_DEVICE => {
                header.skip_file_content(file)?;
                writeln!(
                    out,
                    "{} {:>3} {:<8} {:<8} {:>3}, {:>3} {} {}",
                    std::str::from_utf8(&mode_string).unwrap(),
                    header.nlink,
                    user,
                    group,
                    header.rmajor,
                    header.rminor,
                    time_string,
                    header.filename
                )?;
            }
            _ => {
                header.skip_file_content(file)?;
                writeln!(
                    out,
                    "{} {:>3} {:<8} {:<8} {:>8} {} {}",
                    std::str::from_utf8(&mode_string).unwrap(),
                    header.nlink,
                    user,
                    group,
                    header.filesize,
                    time_string,
                    header.filename
                )?;
            }
        };
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

fn write_character_device(
    header: &Header,
    preserve_permissions: bool,
    log_level: u32,
) -> Result<()> {
    if header.filesize != 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid size for character device '{}': {} bytes instead of 0.",
                header.filename, header.filesize
            ),
        ));
    };
    if log_level >= LOG_LEVEL_DEBUG {
        writeln!(
            std::io::stderr(),
            "Creating character device '{}' with mode {:o}",
            header.filename,
            header.mode_perm(),
        )?;
    };
    if let Err(e) = mknod(&header.filename, header.mode, header.rmajor, header.rminor) {
        match e.kind() {
            ErrorKind::AlreadyExists => {
                remove_file(&header.filename)?;
                mknod(&header.filename, header.mode, header.rmajor, header.rminor)?;
            }
            _ => {
                return Err(e);
            }
        }
    };
    if preserve_permissions {
        lchown(&header.filename, Some(header.uid), Some(header.gid))?;
    };
    set_permissions(&header.filename, header.permission())?;
    set_modified(&header.filename, header.mtime.into())?;
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

fn write_file<R: Read + SeekForward>(
    cpio_file: &mut R,
    header: &Header,
    preserve_permissions: bool,
    seen_files: &mut SeenFiles,
    log_level: u32,
) -> Result<()> {
    let mut file;
    if let Some(target) = header.try_get_hard_link_target(seen_files) {
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
        return Err(Error::other(format!(
            "Wrong amound of bytes written to '{}': {} != {}.",
            header.filename, written, header.filesize
        )));
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
        let header = match Header::read(file) {
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
            FILETYPE_CHARACTER_DEVICE => {
                write_character_device(&header, preserve_permissions, log_level)?
            }
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
            FILETYPE_FIFO | FILETYPE_BLOCK_DEVICE | FILETYPE_SOCKET => {
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
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

    use super::*;
    use crate::libc::{major, minor};

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
    fn test_decompress_program_not_found() {
        let file = File::open("tests/single.cpio").expect("test cpio should be present");
        let mut cmd = Command::new("non-existing-program");
        let got = decompress(&mut cmd, file).unwrap_err();
        assert_eq!(got.kind(), ErrorKind::Other);
        assert_eq!(
            got.to_string(),
            "Program 'non-existing-program' not found in PATH."
        );
    }

    #[test]
    fn test_read_cpio_and_print_long_format_character_device() {
        // Wrapped before mtime and filename
        let cpio_data = b"07070100000003000021A4000000000000\
        00000000000167055BC800000000000000000000000000000005000000010000\
        000C00000000dev/console\0\0\0\
        0707010000000000000000000000000000000000000001\
        0000000000000000000000000000000000000000000000000000000B00000000\
        TRAILER!!!\0\0\0\0";
        let mut output = Vec::new();
        let mut user_group_cache = UserGroupCache::new();
        env::set_var("TZ", "UTC");
        unsafe { tzset() };
        read_cpio_and_print_long_format(
            &mut cpio_data.as_ref(),
            &mut output,
            1728486311,
            &mut user_group_cache,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "crw-r--r--   1 root     root       5,   1 Oct  8 16:20 dev/console\n"
        );
    }

    #[test]
    fn test_read_cpio_and_print_long_format_directory() {
        // Wrapped before mtime and filename
        let cpio_data = b"07070100000001000047FF000000000000007B00000002\
        66A6E40400000000000000000000000000000000000000000000000B00000000\
        /var/crash\0\0\0\0\
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
            1722389471,
            &mut user_group_cache,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "drwxrwsrwt   2 root     whoopsie        0 Jul 29 00:36 /var/crash\n"
        );
    }

    #[test]
    fn test_read_cpio_and_print_long_format_file() {
        // Wrapped before mtime and filename
        let cpio_data = b"070701000036E4000081A4000003E8000007D000000001\
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
            "-rw-r--r--   1 user     2000           65 Jul 26 04:38 conf/modules\n"
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
    fn test_write_character_device() {
        if getuid() != 0 {
            // This test needs to run as root.
            return;
        }
        let mut header = Header::new(1, 0o20_644, 0, 0, 0, 1740402179, 0, "./null".into());
        header.rmajor = 1;
        header.rminor = 3;
        write_character_device(&header, true, LOG_LEVEL_WARNING).unwrap();

        let attr = std::fs::metadata("null").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.file_type().is_char_device());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        assert_eq!(major(attr.rdev()), header.rmajor);
        assert_eq!(minor(attr.rdev()), header.rminor);
        std::fs::remove_file("null").unwrap();
    }

    #[test]
    fn test_write_directory_with_setuid() {
        let mut mtimes = BTreeMap::new();
        let header = Header::new(
            1,
            0o43_777,
            getuid(),
            getgid(),
            0,
            1720081471,
            0,
            "./directory_with_setuid".into(),
        );
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
        let header = Header::new(
            1,
            0o104_755,
            getuid(),
            getgid(),
            0,
            1720081471,
            9,
            "./file_with_setuid".into(),
        );
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
        let header = Header::new(
            1,
            0o120_777,
            getuid(),
            getgid(),
            0,
            1721427072,
            12,
            "./dead_symlink".into(),
        );
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
