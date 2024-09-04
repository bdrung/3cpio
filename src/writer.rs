// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::collections::HashMap;
use std::fs::{symlink_metadata, Metadata};
use std::io::{Error, ErrorKind, Result, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;

use crate::header::Header;
use crate::{align_to_4_bytes, LOG_LEVEL_DEBUG};

struct HardlinkState {
    ino: u32,
    seen: u32,
}

impl HardlinkState {
    fn new(ino: u32) -> Self {
        Self { ino: ino, seen: 1 }
    }
}

pub struct CpioWriter<'a, W: Write> {
    file: &'a mut W,
    next_ino: u32,
    hardlinks: HashMap<u64, HardlinkState>,
}

impl<'a, W: Write> CpioWriter<'a, W> {
    pub fn new(file: &'a mut W) -> Self {
        Self {
            file,
            next_ino: 0,
            hardlinks: HashMap::new(),
        }
    }

    pub fn add_path(self: &mut Self, path: String, log_level: u32) -> Result<Metadata> {
        //if log_level >= LOG_LEVEL_DEBUG {
        //    writeln!(std::io::stderr(), "Adding {} to cpio archive...", path)?;
        //};
        let stat = symlink_metadata(&path)?;
        // TODO: propper stripping
        let rel_path = sanitize_path(&path);
        let mut header = Header::from_metadata(&stat, self.next_ino, rel_path)?;
        self.next_ino += 1;
        if log_level >= LOG_LEVEL_DEBUG {
            writeln!(std::io::stderr(), "{:?}", header)?;
        };

        if header.nlink > 1 && stat.is_file() {
            let ino = stat.ino();
            match self.hardlinks.get_mut(&ino) {
                Some(hardlink) => {
                    // TODO
                    hardlink.seen += 1;
                    header.ino = hardlink.ino;
                    self.next_ino -= 1;
                    if hardlink.seen < header.nlink {
                        // Defer writing the hardlink
                        header.filesize = 0;
                        if log_level >= LOG_LEVEL_DEBUG {
                            writeln!(
                                std::io::stderr(),
                                "Defer writing {} (seen: {})",
                                path,
                                hardlink.seen
                            )?;
                        };
                    }
                }
                None => {
                    // Defer writing the hardlink
                    self.hardlinks.insert(ino, HardlinkState::new(header.ino));
                    header.filesize = 0;
                    if log_level >= LOG_LEVEL_DEBUG {
                        writeln!(std::io::stderr(), "Defer writing {} (seen: {})", path, 1)?;
                    };
                }
            }
        }

        header.write(self.file)?;
        if header.filesize == 0 {
            return Ok(stat);
        }

        if stat.is_file() {
            let mut reader = std::io::BufReader::new(std::fs::File::open(&path)?);
            let copied_bytes = std::io::copy(&mut reader, self.file)?;
            if copied_bytes != header.filesize.into() {
                return Err(Error::new(
                    ErrorKind::UnexpectedEof,
                    format!(
                        "Copied {} bytes from {} but expected {} bytes.",
                        copied_bytes, path, header.filesize
                    ),
                ));
            }
        } else if stat.is_symlink() {
            let target = std::fs::read_link(path)?;
            self.file.write_all(target.as_os_str().as_bytes())?;
            // TODO: check length
        } else {
            unimplemented!(
                "Path {} not implemented. Please open a bug report requesting support for this type.",
                path
            )
        }
        write_padding(self.file, header.filesize)?;
        Ok(stat)
    }

    /*
    fn add_path_recursive(self: &mut Self, path: String, log_level: u32) -> Result<()> {
        let stat = self.add_path(path.clone(), log_level)?;
        if stat.is_dir() {
            let mut entries = std::fs::read_dir(path)?
                .map(|res| res.map(|e| e.path()))
                .collect::<Result<Vec<_>>>()?;
            entries.sort();
            for p in entries {
                let p = p.into_os_string().into_string().unwrap();
                self.add_path_recursive(p, log_level)?;
            }
        }
        Ok(())
    }
    */

    pub fn add_trailer(self: &mut Self) -> Result<()> {
        Header::trailer().write(self.file)
    }
}

fn sanitize_path(path: &String) -> String {
    match path.strip_prefix("./") {
        Some(p) => {
            if p.len() == 0 {
                ".".into()
            } else {
                p.into()
            }
        }
        None => match path.strip_prefix("/") {
            Some(p) => {
                if p.len() == 0 {
                    ".".into()
                } else {
                    p.into()
                }
            }
            None => path.clone(),
        },
    }
}

fn write_padding<W: Write>(file: &mut W, written_bytes: u32) -> Result<()> {
    let padding_len = align_to_4_bytes(written_bytes);
    if padding_len == 0 {
        return Ok(());
    }
    let padding = vec![0u8; padding_len.try_into().unwrap()];
    file.write_all(&padding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path_absolute_path() {
        assert_eq!(sanitize_path(&"/path/to/file".into()), "path/to/file");
    }

    #[test]
    fn test_sanitize_path_dot() {
        assert_eq!(sanitize_path(&".".into()), ".");
    }

    #[test]
    fn test_sanitize_path_dot_slash() {
        assert_eq!(sanitize_path(&"./".into()), ".");
    }

    #[test]
    fn test_sanitize_path_dot_slash_path() {
        assert_eq!(sanitize_path(&"./path/to/file".into()), "path/to/file");
    }

    #[test]
    fn test_sanitize_path_relative_path() {
        assert_eq!(sanitize_path(&"path/to/file".into()), "path/to/file");
    }

    #[test]
    fn test_sanitize_path_root() {
        assert_eq!(sanitize_path(&"/".into()), ".");
    }
}
