// Copyright (C) 2024-2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::collections::{BTreeMap, HashMap};
use std::fs::{
    create_dir, create_dir_all, hard_link, remove_file, set_permissions, symlink_metadata, File,
    OpenOptions,
};
use std::io::{prelude::*, Error, ErrorKind, Result};
use std::os::unix::fs::{chown, fchown, lchown, symlink};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use glob::Pattern;

use crate::compression::read_magic_header;
use crate::filetype::*;
use crate::header::Header;
use crate::libc::{mknod, set_modified};
use crate::logger::Logger;
use crate::ranges::Ranges;
use crate::seek_forward::SeekForward;
use crate::{filename_matches, seek_to_cpio_end, TRAILER_FILENAME};

// TODO: Document hardlink structure
pub(crate) type SeenFiles = HashMap<u128, String>;

/// Options for extracting cpio archives.
///
/// **Warning**: This struct was designed for the `extract_cpio_archive` function.
/// The API can change between releases and no stability promises are given.
/// Please get in contact to support your use case and make the API for this function stable.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtractOptions {
    make_directories: bool,
    parts: Option<Ranges>,
    patterns: Vec<Pattern>,
    preserve_permissions: bool,
    subdir: Option<String>,
}

impl ExtractOptions {
    /// Create a new extract options structure.
    ///
    /// **Warning**: This function was designed for the `3cpio` command-line application.
    /// The API can change between releases and no stability promises are given.
    /// Please get in contact to support your use case and make the API for this function stable.
    pub fn new(
        make_directories: bool,
        parts: Option<Ranges>,
        patterns: Vec<Pattern>,
        preserve_permissions: bool,
        subdir: Option<String>,
    ) -> Self {
        Self {
            make_directories,
            parts,
            patterns,
            preserve_permissions,
            subdir,
        }
    }
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self::new(false, None, Vec::new(), false, None)
    }
}

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

    fn set_modified_times<W: Write>(&self, logger: &mut Logger<W>) -> Result<()> {
        for (path, mtime) in self.mtimes.iter().rev() {
            debug!(logger, "set mtime {mtime} for '{path}'")?;
            set_modified(path, *mtime)?;
        }
        Ok(())
    }
}

fn absolute_parent_directory<S: AsRef<str>>(path: S, base_dir: &Path) -> Result<PathBuf>
where
    PathBuf: From<S>,
{
    let abspath = if path.as_ref().starts_with("/") {
        PathBuf::from(path)
    } else {
        base_dir.join(path.as_ref())
    };
    match abspath.parent() {
        Some(d) => Ok(d.into()),
        // TODO: Use ErrorKind::InvalidFilename once stable.
        None => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Path {abspath:#?} has no parent directory."),
        )),
    }
}

fn check_path_is_canonical_subdir<S: AsRef<str> + std::fmt::Display>(
    path: S,
    dir: &Path,
    base_dir: &PathBuf,
) -> Result<PathBuf> {
    let canonicalized_path = dir.canonicalize()?;
    if !canonicalized_path.starts_with(base_dir) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "The parent directory of \"{path}\" (resolved to {canonicalized_path:#?}) \
                is not within the directory {base_dir:#?}.",
            ),
        ));
    }
    Ok(canonicalized_path)
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

/// Extract cpio archives.
///
/// **Warning**: This function was designed for the `3cpio` command-line application.
/// The API can change between releases and no stability promises are given.
/// Please get in contact to support your use case and make the API for this function stable.
pub fn extract_cpio_archive<W: Write, LW: Write>(
    mut archive: File,
    mut out: Option<&mut W>,
    options: &ExtractOptions,
    logger: &mut Logger<LW>,
) -> Result<()> {
    let mut count = 0;
    let base_dir = std::env::current_dir()?;
    loop {
        count += 1;
        let compression = match read_magic_header(&mut archive)? {
            None => return Ok(()),
            Some(x) => x,
        };
        if options.parts.as_ref().is_some_and(|f| !f.contains(&count)) {
            if compression.is_uncompressed() && options.parts.as_ref().unwrap().has_more(&count) {
                seek_to_cpio_end(&mut archive)?;
                continue;
            }
            break;
        }
        let mut dir = base_dir.clone();
        if let Some(ref s) = options.subdir {
            dir.push(format!("{s}{count}"));
            create_dir_ignore_existing(&dir)?;
        }
        if compression.is_uncompressed() {
            read_cpio_and_extract(&mut archive, &dir, &mut out, options, logger)?;
        } else {
            let mut decompressed = compression.decompress(archive)?;
            read_cpio_and_extract(&mut decompressed, &dir, &mut out, options, logger)?;
            break;
        }
    }
    Ok(())
}

fn from_mtime(mtime: u32) -> SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime.into())
}

fn extract_to_disk<R: Read + SeekForward, W: Write>(
    archive: &mut R,
    header: &Header,
    extractor: &mut Extractor,
    options: &ExtractOptions,
    logger: &mut Logger<W>,
) -> Result<()> {
    match header.mode & MODE_FILETYPE_MASK {
        FILETYPE_BLOCK_DEVICE | FILETYPE_CHARACTER_DEVICE | FILETYPE_FIFO | FILETYPE_SOCKET => {
            write_special_file(header, options.preserve_permissions, logger)?
        }
        FILETYPE_DIRECTORY => write_directory(
            header,
            options.preserve_permissions,
            logger,
            &mut extractor.mtimes,
        )?,
        FILETYPE_REGULAR_FILE => write_file(
            archive,
            header,
            options.preserve_permissions,
            &mut extractor.seen_files,
            logger,
        )?,
        FILETYPE_SYMLINK => {
            write_symbolic_link(archive, header, options.preserve_permissions, logger)?
        }
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Invalid/unknown file type 0o{:o} for '{}'",
                    header.mode, header.filename
                ),
            ))
        }
    }
    Ok(())
}

fn extract_to_writable<R, W>(archive: &mut R, header: &Header, out: &mut W) -> Result<()>
where
    R: Read + SeekForward,
    W: Write,
{
    if header.filesize == 0 {
        return Ok(());
    }
    if matches!(header.mode & MODE_FILETYPE_MASK, FILETYPE_REGULAR_FILE) {
        write_file_content(archive, out, header)?;
    } else {
        header.skip_file_content(archive)?;
    }
    Ok(())
}

fn read_cpio_and_extract<R: Read + SeekForward, W: Write, LW: Write>(
    archive: &mut R,
    base_dir: &PathBuf,
    out: &mut Option<W>,
    options: &ExtractOptions,
    logger: &mut Logger<LW>,
) -> Result<()> {
    let mut extractor = Extractor::new();
    let mut previous_checked_dir = PathBuf::new();
    if out.is_none() {
        std::env::set_current_dir(base_dir)?;
    }
    loop {
        let header = match Header::read(archive) {
            Ok(header) => {
                if header.filename == TRAILER_FILENAME {
                    break;
                } else {
                    header
                }
            }
            Err(e) => return Err(e),
        };

        debug!(logger, "{header:?}")?;

        if !options.patterns.is_empty() && !filename_matches(&header.filename, &options.patterns) {
            header.skip_file_content(archive)?;
            continue;
        }

        info!(logger, "{}", header.filename)?;

        match out {
            None => {
                if !header.is_root_directory() {
                    // TODO: use dirfd once stable: https://github.com/rust-lang/rust/issues/120426
                    let absdir = absolute_parent_directory(&header.filename, base_dir)?;
                    // canonicalize() is an expensive call. So cache the previously resolved
                    // parent directory. Skip the path traversal check in case the absolute
                    // parent directory has no symlinks and matches the previouly checked directory.
                    if absdir != previous_checked_dir {
                        if options.make_directories {
                            create_dir_all(&absdir)?;
                        }
                        previous_checked_dir =
                            check_path_is_canonical_subdir(&header.filename, &absdir, base_dir)?;
                    }
                }
                extract_to_disk(archive, &header, &mut extractor, options, logger)?;
            }
            Some(out) => extract_to_writable(archive, &header, out)?,
        }
    }
    extractor.set_modified_times(logger)?;
    Ok(())
}

fn write_special_file<W: Write>(
    header: &Header,
    preserve_permissions: bool,
    logger: &mut Logger<W>,
) -> Result<()> {
    if header.filesize != 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid size for {} '{}': {} bytes instead of 0.",
                header.file_type_name(),
                header.filename,
                header.filesize
            ),
        ));
    };
    debug!(
        logger,
        "Creating {} '{}' with mode {:o}",
        header.file_type_name(),
        header.filename,
        header.mode_perm(),
    )?;
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

fn write_directory<W: Write>(
    header: &Header,
    preserve_permissions: bool,
    logger: &mut Logger<W>,
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
    debug!(
        logger,
        "Creating directory '{}' with mode {:o}{}",
        header.filename,
        header.mode_perm(),
        if preserve_permissions {
            format!(" and owner {}:{}", header.uid, header.gid)
        } else {
            String::new()
        },
    )?;
    create_dir_ignore_existing(&header.filename)?;
    if preserve_permissions {
        chown(&header.filename, Some(header.uid), Some(header.gid))?;
    }
    set_permissions(&header.filename, header.permission())?;
    mtimes.insert(header.filename.to_string(), header.mtime.into());
    Ok(())
}

fn write_file<R: Read + SeekForward, W: Write>(
    archive: &mut R,
    header: &Header,
    preserve_permissions: bool,
    seen_files: &mut SeenFiles,
    logger: &mut Logger<W>,
) -> Result<()> {
    let mut file;
    if let Some(target) = header.try_get_hard_link_target(seen_files) {
        debug!(
            logger,
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
        debug!(
            logger,
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
        file = File::create(&header.filename)?
    };
    header.mark_seen(seen_files);
    // TODO: check writing hard-link with length == 0
    // TODO: check overwriting existing files/hardlinks
    write_file_content(archive, &mut file, header)?;
    if preserve_permissions {
        fchown(&file, Some(header.uid), Some(header.gid))?;
    }
    file.set_permissions(header.permission())?;
    file.set_modified(from_mtime(header.mtime))?;
    Ok(())
}

fn write_file_content<R: Read + SeekForward, W: Write>(
    archive: &mut R,
    output_file: &mut W,
    header: &Header,
) -> Result<()> {
    let mut reader = archive.take(header.filesize.into());
    let written = std::io::copy(&mut reader, output_file)?;
    if written != header.filesize.into() {
        return Err(Error::other(format!(
            "Wrong amound of bytes written to '{}': {} != {}.",
            header.filename, written, header.filesize
        )));
    }
    header.skip_file_content_padding(archive)
}

fn write_symbolic_link<R: Read + SeekForward, W: Write>(
    archive: &mut R,
    header: &Header,
    preserve_permissions: bool,
    logger: &mut Logger<W>,
) -> Result<()> {
    let target = header.read_symlink_target(archive)?;
    debug!(
        logger,
        "Creating symlink '{}' -> '{}' with mode {:o}",
        header.filename,
        &target,
        header.mode_perm(),
    )?;
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

#[cfg(test)]
mod tests {
    use std::io::Stdout;
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

    use super::*;
    use crate::libc::{major, minor};
    use crate::logger::Level;
    use crate::temp_dir::TempDir;
    use crate::tests::{tests_path, TEST_LOCK};

    fn getgid() -> u32 {
        unsafe { ::libc::getgid() }
    }

    fn getuid() -> u32 {
        unsafe { ::libc::getuid() }
    }

    #[test]
    fn test_absolute_parent_directory() {
        let base_dir = Path::new("/nonexistent/arthur");
        assert_eq!(
            absolute_parent_directory("usr/bin/true", base_dir).unwrap(),
            PathBuf::from("/nonexistent/arthur/usr/bin")
        );
        assert_eq!(
            absolute_parent_directory("/usr/bin/true", base_dir).unwrap(),
            PathBuf::from("/usr/bin")
        );
        assert_eq!(
            absolute_parent_directory(".", base_dir).unwrap(),
            PathBuf::from("/nonexistent")
        );
    }

    #[test]
    fn test_absolute_parent_directory_error() {
        let got = absolute_parent_directory(".", Path::new("/")).unwrap_err();
        assert_eq!(got.kind(), ErrorKind::InvalidData);
        assert_eq!(got.to_string(), "Path \"/.\" has no parent directory.");
    }

    #[test]
    fn test_extract_cpio_archive_compressed_make_directories_with_pattern() {
        let _lock = TEST_LOCK.lock().unwrap();
        let archive = File::open(tests_path("lz4.cpio")).unwrap();
        let tempdir = TempDir::new_and_set_current_dir().unwrap();
        let patterns = vec![Pattern::new("p?th/f*").unwrap()];
        let options = ExtractOptions::new(true, None, patterns, false, None);
        let mut logger = Logger::new_vec(Level::Info);

        extract_cpio_archive(archive, None::<&mut Stdout>, &options, &mut logger).unwrap();
        assert!(tempdir.path.join("path").is_dir());
        assert!(tempdir.path.join("path/file").exists());
        assert!(!tempdir.path.join("usr").exists());
        assert_eq!(logger.get_logs(), "path/file\n");
    }

    #[test]
    fn test_extract_cpio_archive_compressed_parts_to_stdout() {
        let archive = File::open(tests_path("lzma.cpio")).unwrap();
        let mut output = Vec::new();
        let options = ExtractOptions::new(
            false,
            Some("-1".parse::<Ranges>().unwrap()),
            Vec::new(),
            false,
            None,
        );
        let mut logger = Logger::new_vec(Level::Info);
        extract_cpio_archive(archive, Some(&mut output), &options, &mut logger).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "content\n");
        assert_eq!(logger.get_logs(), ".\npath\npath/file\n");
    }

    #[test]
    fn test_extract_cpio_archive_compressed_to_stdout() {
        let archive = File::open(tests_path("bzip2.cpio")).unwrap();
        let mut output = Vec::new();
        let options = ExtractOptions::default();
        let mut logger = Logger::new_vec(Level::Warning);
        extract_cpio_archive(archive, Some(&mut output), &options, &mut logger).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "content\nThis is a fake busybox binary to simulate a POSIX shell\n"
        );
        assert_eq!(logger.get_logs(), "");
    }

    #[test]
    fn test_extract_cpio_archive_compressed_with_pattern() {
        let _lock = TEST_LOCK.lock().unwrap();
        let archive = File::open(tests_path("zstd.cpio")).unwrap();
        let tempdir = TempDir::new_and_set_current_dir().unwrap();
        let patterns = vec![Pattern::new("p?th").unwrap()];
        let options = ExtractOptions::new(false, None, patterns, false, None);
        let mut logger = Logger::new_vec(Level::Debug);
        extract_cpio_archive(archive, None::<&mut Stdout>, &options, &mut logger).unwrap();
        assert!(tempdir.path.join("path").is_dir());
        assert!(!tempdir.path.join("path/file").exists());
        assert_eq!(
            logger.get_logs(),
            "Header { ino: 0, mode: 16893, uid: 0, gid: 0, nlink: 2, mtime: 1713104326, filesize: 0, \
            major: 0, minor: 0, rmajor: 0, rminor: 0, filename: \".\" }\n\
            Header { ino: 1, mode: 16893, uid: 0, gid: 0, nlink: 2, mtime: 1713104326, filesize: 0, \
            major: 0, minor: 0, rmajor: 0, rminor: 0, filename: \"path\" }\n\
            path\n\
            Creating directory 'path' with mode 775\n\
            Header { ino: 2, mode: 33204, uid: 0, gid: 0, nlink: 1, mtime: 1713104326, filesize: 8, \
            major: 0, minor: 0, rmajor: 0, rminor: 0, filename: \"path/file\" }\n\
            set mtime 1713104326 for 'path'\n\
            Header { ino: 0, mode: 16893, uid: 0, gid: 0, nlink: 2, mtime: 1713104326, filesize: 0, \
            major: 0, minor: 0, rmajor: 0, rminor: 0, filename: \".\" }\n\
            Header { ino: 1, mode: 16893, uid: 0, gid: 0, nlink: 2, mtime: 1713104326, filesize: 0, \
            major: 0, minor: 0, rmajor: 0, rminor: 0, filename: \"usr\" }\n\
            Header { ino: 2, mode: 16893, uid: 0, gid: 0, nlink: 2, mtime: 1713104326, filesize: 0, \
            major: 0, minor: 0, rmajor: 0, rminor: 0, filename: \"usr/bin\" }\n\
            Header { ino: 3, mode: 33204, uid: 0, gid: 0, nlink: 1, mtime: 1713104326, filesize: 56, \
            major: 0, minor: 0, rmajor: 0, rminor: 0, filename: \"usr/bin/sh\" }\n"
        );
    }

    #[test]
    fn test_extract_cpio_archive_compressed_with_pattern_to_stdout() {
        let archive = File::open(tests_path("gzip.cpio")).unwrap();
        let patterns: Vec<Pattern> = vec![Pattern::new("*/b?n/sh").unwrap()];
        let mut output = Vec::new();
        let options = ExtractOptions::new(false, None, patterns, false, None);
        let mut logger = Logger::new_vec(Level::Info);
        extract_cpio_archive(archive, Some(&mut output), &options, &mut logger).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "This is a fake busybox binary to simulate a POSIX shell\n"
        );
        assert_eq!(logger.get_logs(), "usr/bin/sh\n");
    }

    #[test]
    fn test_extract_cpio_archive_uncompressed_with_pattern() {
        let _lock = TEST_LOCK.lock().unwrap();
        let archive = File::open(tests_path("single.cpio")).unwrap();
        let tempdir = TempDir::new_and_set_current_dir().unwrap();
        let patterns = vec![Pattern::new("path").unwrap()];
        let options = ExtractOptions::new(false, None, patterns, false, None);
        let mut logger = Logger::new_vec(Level::Info);
        extract_cpio_archive(archive, None::<&mut Stdout>, &options, &mut logger).unwrap();
        assert!(tempdir.path.join("path").is_dir());
        assert!(!tempdir.path.join("path/file").exists());
        assert_eq!(logger.get_logs(), "path\n");
    }

    #[test]
    fn test_extract_cpio_archive_with_subdir() {
        let _lock = TEST_LOCK.lock().unwrap();
        let archive = File::open(tests_path("single.cpio")).unwrap();
        let tempdir = TempDir::new_and_set_current_dir().unwrap();
        let options = ExtractOptions::new(false, None, Vec::new(), false, Some("cpio".into()));
        let mut logger = Logger::new_vec(Level::Info);
        extract_cpio_archive(archive, None::<&mut Stdout>, &options, &mut logger).unwrap();
        let path = tempdir.path.join("cpio1/path/file");
        assert!(path.exists());
        assert_eq!(logger.get_logs(), ".\npath\npath/file\n");
    }

    #[test]
    fn test_read_cpio_and_extract_fifo() {
        let _lock = TEST_LOCK.lock().unwrap();
        let tempdir = TempDir::new_and_set_current_dir().unwrap();
        let path = tempdir.path.join("fifo.cpio");
        let uid = getuid();
        let gid = getgid();
        let header = Header::new(1, 0o010_600, uid, gid, 1, 1746789067, 0, 0, 0, "initctl");
        let mut archive = File::create(&path).unwrap();
        header.write(&mut archive).unwrap();
        Header::trailer().write(&mut archive).unwrap();

        let mut archive = File::open(&path).unwrap();
        let mut logger = Logger::new_vec(Level::Info);
        read_cpio_and_extract(
            &mut archive,
            &tempdir.path,
            &mut None::<Stdout>,
            &ExtractOptions::default(),
            &mut logger,
        )
        .unwrap();

        let attr = std::fs::metadata("initctl").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.file_type().is_fifo());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        assert_eq!(major(attr.rdev()), header.rmajor);
        assert_eq!(minor(attr.rdev()), header.rminor);
        assert_eq!(logger.get_logs(), "initctl\n");
    }

    #[test]
    fn test_read_cpio_and_extract_invalid_file_type() {
        let _lock = TEST_LOCK.lock().unwrap();
        let tempdir = TempDir::new().unwrap();
        let cwd = std::env::current_dir().unwrap();
        let path = tempdir.path.join("invalid.cpio");
        let mut archive = File::create(&path).unwrap();
        archive
            .write_all(
                b"070701000000010003FFA200000007000000070000000168AEBD2C\
                00000000000000000000000000000000000000000000000800000000\
                invalid\0\0\0\
                070701000000000000000000000000000000000000000100000000\
                00000000000000000000000000000000000000000000000B00000000\
                TRAILER!!!\0\0\0\0",
            )
            .unwrap();
        let mut archive = File::open(&path).unwrap();
        let mut logger = Logger::new_vec(Level::Warning);
        let got = read_cpio_and_extract(
            &mut archive,
            &tempdir.path,
            &mut None::<Stdout>,
            &ExtractOptions::default(),
            &mut logger,
        )
        .unwrap_err();
        std::env::set_current_dir(&cwd).unwrap();

        assert_eq!(got.kind(), ErrorKind::InvalidData);
        assert_eq!(
            got.to_string(),
            "Invalid/unknown file type 0o777642 for 'invalid'",
        );
        assert_eq!(logger.get_logs(), "");
        assert!(!Path::new("invalid").exists());
    }

    // Test detecting path traversal attacks like CVE-2015-1197
    #[test]
    fn test_read_cpio_and_extract_path_traversal() {
        let _lock = TEST_LOCK.lock().unwrap();
        let mut archive = File::open(tests_path("path-traversal.cpio")).unwrap();
        let tempdir = TempDir::new_and_set_current_dir().unwrap();
        let mut logger = Logger::new_vec(Level::Info);
        let got = read_cpio_and_extract(
            &mut archive,
            &tempdir.path,
            &mut None::<Stdout>,
            &ExtractOptions::default(),
            &mut logger,
        )
        .unwrap_err();
        assert_eq!(got.kind(), ErrorKind::InvalidData);
        assert_eq!(got.to_string(), format!(
            "The parent directory of \"tmp/trav.txt\" (resolved to \"/tmp\") is not within the directory {:#?}.",
            &tempdir.path
        ));
        assert_eq!(logger.get_logs(), ".\ntmp\ntmp/trav.txt\n");
    }

    #[test]
    fn test_read_cpio_and_extract_path_traversal_to_stdout() {
        let mut archive = File::open(tests_path("path-traversal.cpio")).unwrap();
        let base_dir = std::env::current_dir().unwrap();
        let mut output = Vec::new();
        let mut logger = Logger::new_vec(Level::Info);
        read_cpio_and_extract(
            &mut archive,
            &base_dir,
            &mut Some(&mut output),
            &ExtractOptions::default(),
            &mut logger,
        )
        .unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "TEST Traversal\n");
        assert_eq!(logger.get_logs(), ".\ntmp\ntmp/trav.txt\n");
    }

    #[test]
    fn test_write_special_file_block_device() {
        if getuid() != 0 {
            // This test needs to run as root.
            return;
        }
        let _lock = TEST_LOCK.lock().unwrap();
        let _tempdir = TempDir::new_and_set_current_dir().unwrap();
        let header = Header::new(1, 0o60_660, 0, 6, 1, 1751300235, 0, 7, 99, "loop99");
        let mut logger = Logger::new_vec(Level::Debug);
        write_special_file(&header, true, &mut logger).unwrap();

        let attr = std::fs::metadata("loop99").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.file_type().is_block_device());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        assert_eq!(major(attr.rdev()), header.rmajor);
        assert_eq!(minor(attr.rdev()), header.rminor);
        assert_eq!(
            logger.get_logs(),
            "Creating block device 'loop99' with mode 660\n"
        );
    }

    #[test]
    fn test_write_special_file_character_device() {
        if getuid() != 0 {
            // This test needs to run as root.
            return;
        }
        let _lock = TEST_LOCK.lock().unwrap();
        let _tempdir = TempDir::new_and_set_current_dir().unwrap();
        let header = Header::new(1, 0o20_644, 0, 0, 0, 1740402179, 0, 1, 3, "./null");
        let mut logger = Logger::new_vec(Level::Debug);
        write_special_file(&header, true, &mut logger).unwrap();

        let attr = std::fs::metadata("null").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.file_type().is_char_device());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        assert_eq!(major(attr.rdev()), header.rmajor);
        assert_eq!(minor(attr.rdev()), header.rminor);
        assert_eq!(
            logger.get_logs(),
            "Creating character device './null' with mode 644\n"
        );
        std::fs::remove_file("null").unwrap();
    }

    #[test]
    fn test_write_special_file_fifo() {
        let _lock = TEST_LOCK.lock().unwrap();
        let _tempdir = TempDir::new_and_set_current_dir().unwrap();
        let uid = getuid();
        let gid = getgid();
        let header = Header::new(1, 0o010_600, uid, gid, 1, 1746789067, 0, 0, 0, "initctl");
        let mut logger = Logger::new_vec(Level::Debug);
        write_special_file(&header, false, &mut logger).unwrap();

        let attr = std::fs::metadata("initctl").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.file_type().is_fifo());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        assert_eq!(major(attr.rdev()), header.rmajor);
        assert_eq!(minor(attr.rdev()), header.rminor);
        assert_eq!(logger.get_logs(), "Creating fifo 'initctl' with mode 600\n");
    }

    #[test]
    fn test_write_special_file_socket() {
        let _lock = TEST_LOCK.lock().unwrap();
        let _tempdir = TempDir::new_and_set_current_dir().unwrap();
        let uid = getuid();
        let gid = getgid();
        let header = Header::new(1, 0o140_777, uid, gid, 1, 1746789058, 0, 0, 0, "notify");
        let mut logger = Logger::new_vec(Level::Debug);
        write_special_file(&header, true, &mut logger).unwrap();

        let attr = std::fs::metadata("notify").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.file_type().is_socket());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        assert_eq!(major(attr.rdev()), header.rmajor);
        assert_eq!(minor(attr.rdev()), header.rminor);
        assert_eq!(
            logger.get_logs(),
            "Creating socket 'notify' with mode 777\n"
        );
    }

    #[test]
    fn test_write_directory_with_setuid() {
        let _lock = TEST_LOCK.lock().unwrap();
        let _tempdir = TempDir::new_and_set_current_dir().unwrap();
        let mut mtimes = BTreeMap::new();
        let header = Header::new(
            1,
            0o43_777,
            getuid(),
            getgid(),
            0,
            1720081471,
            0,
            0,
            0,
            "./directory_with_setuid",
        );
        let mut logger = Logger::new_vec(Level::Debug);
        write_directory(&header, true, &mut logger, &mut mtimes).unwrap();

        let attr = std::fs::metadata("directory_with_setuid").unwrap();
        assert!(attr.is_dir());
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        assert_eq!(
            logger.get_logs(),
            format!(
                "Creating directory './directory_with_setuid' with mode 3777 and owner {}:{}\n",
                getuid(),
                getgid(),
            ),
        );
        std::fs::remove_dir("directory_with_setuid").unwrap();

        let mut expected_mtimes: BTreeMap<String, i64> = BTreeMap::new();
        expected_mtimes.insert("./directory_with_setuid".into(), header.mtime.into());
        assert_eq!(mtimes, expected_mtimes);
    }

    #[test]
    fn test_write_file_with_setuid() {
        let _lock = TEST_LOCK.lock().unwrap();
        let _tempdir = TempDir::new_and_set_current_dir().unwrap();
        let mut seen_files = SeenFiles::new();
        let header = Header::new(
            1,
            0o104_755,
            getuid(),
            getgid(),
            0,
            1720081471,
            9,
            0,
            0,
            "./file_with_setuid",
        );
        let cpio = b"!/bin/sh\n\0\0\0";
        let mut logger = Logger::new_vec(Level::Debug);
        write_file(
            &mut cpio.as_ref(),
            &header,
            true,
            &mut seen_files,
            &mut logger,
        )
        .unwrap();

        let attr = std::fs::metadata("file_with_setuid").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.is_file());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        assert_eq!(
            logger.get_logs(),
            format!(
                "Creating file './file_with_setuid' with permission 4755 and owner {}:{} and 9 bytes\n",
                getuid(),
                getgid(),
            ),
        );
        std::fs::remove_file("file_with_setuid").unwrap();
    }

    #[test]
    fn test_write_symbolic_link() {
        let _lock = TEST_LOCK.lock().unwrap();
        let _tempdir = TempDir::new_and_set_current_dir().unwrap();
        let header = Header::new(
            1,
            0o120_777,
            getuid(),
            getgid(),
            0,
            1721427072,
            12,
            0,
            0,
            "./dead_symlink",
        );
        assert_eq!(header.file_type_name(), "symlink");
        let cpio = b"/nonexistent";
        let mut logger = Logger::new_vec(Level::Debug);
        write_symbolic_link(&mut cpio.as_ref(), &header, true, &mut logger).unwrap();

        let attr = std::fs::symlink_metadata("dead_symlink").unwrap();
        assert_eq!(attr.len(), header.filesize.into());
        assert!(attr.is_symlink());
        assert_eq!(attr.modified().unwrap(), from_mtime(header.mtime));
        assert_eq!(attr.permissions(), PermissionsExt::from_mode(header.mode));
        assert_eq!(attr.uid(), header.uid);
        assert_eq!(attr.gid(), header.gid);
        assert_eq!(
            logger.get_logs(),
            "Creating symlink './dead_symlink' -> '/nonexistent' with mode 777\n"
        );
        std::fs::remove_file("dead_symlink").unwrap();
    }
}
