// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::collections::HashMap;
use std::fs::File;
use std::io::{prelude::*, Result};
use std::time::SystemTime;

use glob::Pattern;

use crate::compression::read_magic_header;
use crate::filetype::*;
use crate::header::{read_filename_from_next_cpio_object, Header};
use crate::libc::strftime_local;
use crate::manifest::Manifest;
use crate::ranges::Ranges;
use crate::seek_forward::SeekForward;

mod compression;
mod extended_error;
pub mod extract;
mod filetype;
mod header;
mod libc;
mod manifest;
pub mod ranges;
mod seek_forward;
pub mod temp_dir;

pub const LOG_LEVEL_WARNING: u32 = 5;
pub const LOG_LEVEL_INFO: u32 = 7;
pub const LOG_LEVEL_DEBUG: u32 = 8;

struct CpioFilenameReader<'a, R: Read + SeekForward> {
    archive: &'a mut R,
}

impl<R: Read + SeekForward> Iterator for CpioFilenameReader<'_, R> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        match read_filename_from_next_cpio_object(self.archive) {
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

fn read_cpio_and_print_filenames<R: Read + SeekForward, W: Write>(
    archive: &mut R,
    out: &mut W,
    patterns: &Vec<Pattern>,
) -> Result<()> {
    let cpio = CpioFilenameReader { archive };
    for f in cpio {
        let filename = f?;
        if patterns.is_empty() || filename_matches(&filename, patterns) {
            writeln!(out, "{filename}")?;
        }
    }
    Ok(())
}

fn read_cpio_and_print_long_format<R: Read + SeekForward, W: Write>(
    archive: &mut R,
    out: &mut W,
    patterns: &Vec<Pattern>,
    now: i64,
    user_group_cache: &mut UserGroupCache,
    print_ino: bool,
) -> Result<()> {
    // Files can have the same mtime (especially when using SOURCE_DATE_EPOCH).
    // Cache the time string of the last mtime.
    let mut last_mtime = 0;
    let mut time_string: String = "".into();
    loop {
        let header = match Header::read(archive) {
            Ok(header) => {
                if header.filename == "TRAILER!!!" {
                    break;
                } else {
                    header
                }
            }
            Err(e) => return Err(e),
        };

        if !patterns.is_empty() && !filename_matches(&header.filename, patterns) {
            header.skip_file_content(archive)?;
            continue;
        }

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

        if print_ino {
            write!(out, "{:>4} ", header.ino)?;
        }
        match header.mode & MODE_FILETYPE_MASK {
            FILETYPE_SYMLINK => {
                let target = header.read_symlink_target(archive)?;
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
                header.skip_file_content(archive)?;
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
                header.skip_file_content(archive)?;
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

// Does the given file name matches one of the globbing patterns?
fn filename_matches(filename: &str, patterns: &Vec<Pattern>) -> bool {
    for pattern in patterns {
        if pattern.matches(filename) {
            return true;
        }
    }
    false
}

fn seek_to_cpio_end(archive: &mut File) -> Result<()> {
    let cpio = CpioFilenameReader { archive };
    for f in cpio {
        f?;
    }
    Ok(())
}

pub fn get_cpio_archive_count(archive: &mut File) -> Result<u32> {
    let mut count = 0;
    loop {
        let compression = match read_magic_header(archive) {
            None => return Ok(count),
            Some(x) => x?,
        };
        count += 1;
        if compression.is_uncompressed() {
            seek_to_cpio_end(archive)?;
        } else {
            break;
        }
    }
    Ok(count)
}

// Parse SOURCE_DATE_EPOCH environment variable (if set and valid integer)
fn get_source_date_epoch() -> Option<u32> {
    match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(value) => match value.parse::<i64>() {
            Ok(source_date_epoch) => {
                if let Ok(x) = source_date_epoch.try_into() {
                    Some(x)
                } else if source_date_epoch < 0 {
                    Some(0)
                } else {
                    Some(u32::MAX)
                }
            }
            Err(_) => None,
        },
        Err(_) => None,
    }
}

pub fn print_cpio_archive_count<W: Write>(mut archive: File, out: &mut W) -> Result<()> {
    let count = get_cpio_archive_count(&mut archive)?;
    writeln!(out, "{count}")?;
    Ok(())
}

// Return the size in bytes of the uncompressed data.
pub fn create_cpio_archive(
    archive: Option<File>,
    alignment: Option<u32>,
    log_level: u32,
) -> Result<u64> {
    let source_date_epoch = get_source_date_epoch();
    let stdin = std::io::stdin();
    let buf_reader = std::io::BufReader::new(stdin);
    if log_level >= LOG_LEVEL_DEBUG {
        eprintln!("Parsing manifest from stdin...");
    }
    let manifest = Manifest::from_input(buf_reader, log_level)?;
    if log_level >= LOG_LEVEL_DEBUG {
        eprintln!("Writing cpio...");
    }
    manifest.write_archive(archive, alignment, source_date_epoch, log_level)
}

pub fn examine_cpio_content<W: Write>(mut archive: File, out: &mut W) -> Result<()> {
    loop {
        let compression = match read_magic_header(&mut archive) {
            None => return Ok(()),
            Some(x) => x?,
        };
        writeln!(
            out,
            "{}\t{}",
            archive.stream_position()?,
            compression.command()
        )?;
        if compression.is_uncompressed() {
            seek_to_cpio_end(&mut archive)?;
        } else {
            break;
        }
    }
    Ok(())
}

pub fn list_cpio_content<W: Write>(
    mut archive: File,
    out: &mut W,
    parts: Option<&Ranges>,
    patterns: &Vec<Pattern>,
    log_level: u32,
) -> Result<()> {
    let mut user_group_cache = UserGroupCache::new();
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .try_into()
        .unwrap();
    let mut count = 0;
    loop {
        count += 1;
        let compression = match read_magic_header(&mut archive) {
            None => return Ok(()),
            Some(x) => x?,
        };
        if parts.is_some_and(|f| !f.contains(&count)) {
            if compression.is_uncompressed() && parts.unwrap().has_more(&count) {
                seek_to_cpio_end(&mut archive)?;
                continue;
            }
            break;
        }
        if compression.is_uncompressed() {
            if log_level >= LOG_LEVEL_INFO {
                read_cpio_and_print_long_format(
                    &mut archive,
                    out,
                    patterns,
                    now,
                    &mut user_group_cache,
                    log_level >= LOG_LEVEL_DEBUG,
                )?;
            } else {
                read_cpio_and_print_filenames(&mut archive, out, patterns)?;
            }
        } else {
            let mut decompressed = compression.decompress(archive)?;
            if log_level >= LOG_LEVEL_INFO {
                read_cpio_and_print_long_format(
                    &mut decompressed,
                    out,
                    patterns,
                    now,
                    &mut user_group_cache,
                    log_level >= LOG_LEVEL_DEBUG,
                )?;
            } else {
                read_cpio_and_print_filenames(&mut decompressed, out, patterns)?;
            }
            break;
        }
    }
    Ok(())
}

/// Returns the amount of padding needed after `offset` to ensure that the
/// following address will be aligned to `alignment`.
const fn padding_needed_for(offset: u64, alignment: u64) -> u64 {
    let misalignment = offset % alignment;
    if misalignment == 0 {
        return 0;
    }
    alignment - misalignment
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::io::SeekFrom;
    use std::path::{Path, PathBuf};

    use super::*;

    // Lock for tests that rely on / change the current directory
    pub static TEST_LOCK: std::sync::Mutex<u32> = std::sync::Mutex::new(0);

    #[test]
    fn test_padding_needed_for() {
        assert_eq!(padding_needed_for(110, 4), 2);
    }

    #[test]
    fn test_padding_needed_for_is_aligned() {
        assert_eq!(padding_needed_for(32, 4), 0);
    }

    pub fn tests_path<P: AsRef<Path>>(path: P) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(path)
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
    fn test_get_cpio_archive_count_single() {
        let mut archive =
            File::open(tests_path("single.cpio")).expect("test cpio should be present");
        let count = get_cpio_archive_count(&mut archive).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_list_cpio_content_compressed_parts() {
        let archive = File::open(tests_path("gzip.cpio")).unwrap();
        let mut output = Vec::new();
        list_cpio_content(
            archive,
            &mut output,
            Some(&"2-".parse::<Ranges>().unwrap()),
            &Vec::new(),
            LOG_LEVEL_WARNING,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            ".\nusr\nusr/bin\nusr/bin/sh\n"
        );
    }

    #[test]
    fn test_list_cpio_content_compressed_with_pattern() {
        let archive = File::open(tests_path("xz.cpio")).unwrap();
        let patterns = vec![Pattern::new("p?th").unwrap()];
        let mut output = Vec::new();
        list_cpio_content(archive, &mut output, None, &patterns, LOG_LEVEL_WARNING).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "path\n");
    }

    #[test]
    fn test_list_cpio_content_uncompressed_with_pattern() {
        let archive = File::open(tests_path("single.cpio")).unwrap();
        let patterns = vec![Pattern::new("*/file").unwrap()];
        let mut output = Vec::new();
        list_cpio_content(archive, &mut output, None, &patterns, LOG_LEVEL_WARNING).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "path/file\n");
    }

    #[test]
    fn test_print_cpio_archive_count() {
        let mut archive = File::open(tests_path("zstd.cpio")).expect("test cpio should be present");
        let mut output = Vec::new();

        let count = get_cpio_archive_count(&mut archive).unwrap();
        assert_eq!(count, 2);

        archive.seek(SeekFrom::Start(0)).unwrap();
        print_cpio_archive_count(archive, &mut output).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "2\n");
    }

    #[test]
    fn test_read_cpio_and_print_long_format_character_device() {
        // Wrapped before mtime and filename
        let archive = b"07070100000003000021A4000000000000\
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
            &mut archive.as_ref(),
            &mut output,
            &Vec::new(),
            1728486311,
            &mut user_group_cache,
            false,
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
        let archive = b"07070100000001000047FF000000000000007B00000002\
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
            &mut archive.as_ref(),
            &mut output,
            &Vec::new(),
            1722389471,
            &mut user_group_cache,
            false,
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
        let archive = b"070701000036E4000081A4000003E8000007D000000001\
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
            &mut archive.as_ref(),
            &mut output,
            &Vec::new(),
            1722645915,
            &mut user_group_cache,
            false,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "-rw-r--r--   1 user     2000           65 Jul 26 04:38 conf/modules\n"
        );
    }

    #[test]
    fn test_read_cpio_and_print_long_format_pattern() {
        // Wrapped before mtime and filename
        let archive = b"070701000036E4000081A4000003E8000007D000000001\
        66A3285300000041000000000000002400000000000000000000000D00000000\
        conf/modules\0\0\
        linear\nmultipath\nraid0\nraid1\nraid456\nraid5\nraid6\nraid10\nefivarfs\0\0\0\0\
        0707010000000D0000A1FF000000000000000000000001\
        6237389400000007000000000000000000000000000000000000000400000000\
        bin\0\0\0usr/bin\0\
        0707010000000000000000000000000000000000000001\
        0000000000000000000000000000000000000000000000000000000B00000000\
        TRAILER!!!\0\0\0\0";
        let mut output = Vec::new();
        let mut user_group_cache = UserGroupCache::new();
        user_group_cache.insert_test_data();
        env::set_var("TZ", "UTC");
        unsafe { tzset() };
        read_cpio_and_print_long_format(
            &mut archive.as_ref(),
            &mut output,
            &vec![Pattern::new("bin").unwrap()],
            1722645915,
            &mut user_group_cache,
            false,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "lrwxrwxrwx   1 root     root            7 Mar 20  2022 bin -> usr/bin\n"
        );
    }

    #[test]
    fn test_read_cpio_and_print_long_format_symlink() {
        // Wrapped before mtime and filename
        let archive = b"0707010000000D0000A1FF000000000000000000000001\
        6237389400000007000000000000000000000000000000000000000400000000\
        bin\0\0\0usr/bin\0\
        0707010000000000000000000000000000000000000001\
        0000000000000000000000000000000000000000000000000000000B00000000\
        TRAILER!!!\0\0\0\0";
        let mut output = Vec::new();
        let mut user_group_cache = UserGroupCache::new();
        user_group_cache.insert_test_data();
        read_cpio_and_print_long_format(
            &mut archive.as_ref(),
            &mut output,
            &Vec::new(),
            1722645915,
            &mut user_group_cache,
            false,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "lrwxrwxrwx   1 root     root            7 Mar 20  2022 bin -> usr/bin\n"
        );
    }

    #[test]
    fn test_read_cpio_and_print_long_format_print_ino() {
        // Wrapped after mtime
        let archive = b"07070100000000000041ED00000000000000000000000265307180\
        00000000000000000000000000000000000000000000000200000000.\0\
        07070100000001000041ED00000000000000000000000265307180\
        00000000000000000000000000000000000000000000000700000000kernel\0\0\0\0\
        070701000000000000000000000000000000000000000100000000\
        00000000000000000000000000000000000000000000000B00000000TRAILER!!!\0\0\0\0";
        let mut output = Vec::new();
        let mut user_group_cache = UserGroupCache::new();
        user_group_cache.insert_test_data();
        env::set_var("TZ", "UTC");
        unsafe { tzset() };
        read_cpio_and_print_long_format(
            &mut archive.as_ref(),
            &mut output,
            &Vec::new(),
            1722645915,
            &mut user_group_cache,
            true,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            concat!(
                "   0 drwxr-xr-x   2 root     root            0 Oct 19  2023 .\n",
                "   1 drwxr-xr-x   2 root     root            0 Oct 19  2023 kernel\n"
            )
        );
    }
}
