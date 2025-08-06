// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::fs::File;
use std::io::{Error, ErrorKind, Result};
use std::process::{Child, ChildStdout, Command, Stdio};

#[derive(Debug, PartialEq)]
pub enum Compression {
    Uncompressed,
    Bzip2 {
        level: Option<u32>,
    },
    Gzip {
        level: Option<u32>,
    },
    Lz4 {
        level: Option<u32>,
    },
    Lzma {
        level: Option<u32>,
    },
    Lzop {
        level: Option<u32>,
    },
    Xz {
        level: Option<u32>,
    },
    Zstd {
        level: Option<u32>,
    },
    #[cfg(test)]
    NonExistent,
    #[cfg(test)]
    Failing,
}

impl Compression {
    pub fn from_magic_number(magic_number: [u8; 4]) -> Result<Self> {
        let compression = match magic_number {
            [0x42, 0x5A, 0x68, _] => Compression::Bzip2 { level: None },
            [0x30, 0x37, 0x30, 0x37] => Compression::Uncompressed,
            [0x1F, 0x8B, _, _] => Compression::Gzip { level: None },
            // Different magic numbers (little endian) for lz4:
            // v0.1-v0.9: 0x184C2102
            // v1.0-v1.3: 0x184C2103
            // v1.4+: 0x184D2204
            [0x02, 0x21, 0x4C, 0x18] | [0x03, 0x21, 0x4C, 0x18] | [0x04, 0x22, 0x4D, 0x18] => {
                Compression::Lz4 { level: None }
            }
            [0x5D, _, _, _] => Compression::Lzma { level: None },
            // Full magic number for lzop: [0x89, 0x4C, 0x5A, 0x4F, 0x00, 0x0D, 0x0A, 0x1A, 0x0A]
            [0x89, 0x4C, 0x5A, 0x4F] => Compression::Lzop { level: None },
            // Full magic number for xz: [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]
            [0xFD, 0x37, 0x7A, 0x58] => Compression::Xz { level: None },
            [0x28, 0xB5, 0x2F, 0xFD] => Compression::Zstd { level: None },
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Failed to determine CPIO or compression magic number: 0x{:02x}{:02x}{:02x}{:02x} (big endian)",
                        magic_number[0], magic_number[1], magic_number[2], magic_number[3]
                    ),
                ));
            }
        };
        Ok(compression)
    }

    fn from_str(name: &str) -> Result<Self> {
        let compression = match name {
            "" => Self::Uncompressed,
            "bzip2" => Self::Bzip2 { level: None },
            "gzip" => Self::Gzip { level: None },
            "lz4" => Self::Lz4 { level: None },
            "lzma" => Self::Lzma { level: None },
            "lzop" => Self::Lzop { level: None },
            "xz" => Self::Xz { level: None },
            "zstd" => Self::Zstd { level: None },
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Unknown compression format: {name}"),
                ));
            }
        };
        Ok(compression)
    }

    fn set_level(&mut self, new_level: u32) {
        match self {
            Self::Bzip2 { level }
            | Self::Gzip { level }
            | Self::Lz4 { level }
            | Self::Lzma { level }
            | Self::Lzop { level }
            | Self::Xz { level }
            | Self::Zstd { level } => {
                *level = Some(new_level);
            }
            Self::Uncompressed => {}
            #[cfg(test)]
            Self::NonExistent | Self::Failing => {}
        };
    }

    pub fn from_command_line(line: &str) -> Result<Self> {
        let mut iter = line.split_whitespace();
        let mut compression = if let Some(cmd) = iter.next() {
            Self::from_str(cmd)?
        } else {
            Self::Uncompressed
        };
        for parameter in iter {
            match parameter.strip_prefix("-") {
                Some(value) => {
                    if let Ok(mut level) = value.parse::<i64>() {
                        let (min, max) = match compression {
                            Self::Uncompressed => (0, 0),
                            Self::Bzip2 { level: _ } => (1, 9),
                            Self::Gzip { level: _ } => (1, 9),
                            Self::Lz4 { level: _ } => (1, 12),
                            Self::Lzma { level: _ } => (0, 9),
                            Self::Lzop { level: _ } => (1, 9),
                            Self::Xz { level: _ } => (0, 9),
                            Self::Zstd { level: _ } => (1, 19),
                            #[cfg(test)]
                            Self::NonExistent | Self::Failing => (0, 0),
                        };
                        if level < min {
                            eprintln!(
                                "Compression level {level} lower than minimum, raising to {min}."
                            );
                            level = min;
                        } else if level > max {
                            eprintln!(
                                "Compression level {level} higher than maximum, reducing to {max}."
                            );
                            level = max;
                        }
                        compression.set_level(level.try_into().unwrap());
                    } else {
                        eprintln!(
                            "Unknown/unsupported compression parameter '{parameter}'. Ignoring it.",
                        )
                    }
                }
                None => {
                    eprintln!(
                        "Unknown/unsupported compression parameter '{parameter}'. Ignoring it.",
                    )
                }
            }
        }
        Ok(compression)
    }

    pub fn command(&self) -> &str {
        match self {
            Self::Uncompressed => "cpio",
            Self::Bzip2 { level: _ } => "bzip2",
            Self::Gzip { level: _ } => "gzip",
            Self::Lz4 { level: _ } => "lz4",
            Self::Lzma { level: _ } => "lzma",
            Self::Lzop { level: _ } => "lzop",
            Self::Xz { level: _ } => "xz",
            Self::Zstd { level: _ } => "zstd",
            #[cfg(test)]
            Self::NonExistent => "non-existing-program",
            #[cfg(test)]
            Self::Failing => "false",
        }
    }

    pub fn compress(&self, file: Option<File>, source_date_epoch: Option<u32>) -> Result<Child> {
        let mut command = self.compress_command(source_date_epoch);
        // TODO: Propper error message if spawn fails
        command.stdin(Stdio::piped());
        if let Some(file) = file {
            command.stdout(file);
        }
        let cmd = command.spawn().map_err(|e| match e.kind() {
            ErrorKind::NotFound => Error::other(format!(
                "Program '{}' not found in PATH.",
                command.get_program().to_str().unwrap()
            )),
            _ => e,
        })?;
        Ok(cmd)
    }

    fn compress_command(&self, source_date_epoch: Option<u32>) -> Command {
        let mut command = Command::new(self.command());
        match self {
            Self::Gzip { level: _ } => {
                command.arg("-n");
            }
            Self::Lz4 { level: _ } => {
                command.arg("-l");
            }
            Self::Xz { level: _ } => {
                command.arg("--check=crc32");
            }
            Self::Zstd { level: _ } => {
                command.arg("-q");
            }
            Self::Uncompressed
            | Self::Bzip2 { level: _ }
            | Self::Lzma { level: _ }
            | Self::Lzop { level: _ } => {}
            #[cfg(test)]
            Self::NonExistent | Self::Failing => {}
        };

        match self {
            Self::Bzip2 { level: Some(level) }
            | Self::Gzip { level: Some(level) }
            | Self::Lz4 { level: Some(level) }
            | Self::Lzma { level: Some(level) }
            | Self::Lzop { level: Some(level) }
            | Self::Xz { level: Some(level) }
            | Self::Zstd { level: Some(level) } => {
                command.arg(format!("-{level}"));
            }
            Self::Uncompressed
            | Self::Bzip2 { level: None }
            | Self::Gzip { level: None }
            | Self::Lz4 { level: None }
            | Self::Lzma { level: None }
            | Self::Lzop { level: None }
            | Self::Xz { level: None }
            | Self::Zstd { level: None } => {}
            #[cfg(test)]
            Self::NonExistent | Self::Failing => {}
        };
        // If we're not doing a reproducible build, enable multithreading
        if source_date_epoch.is_none()
            && matches!(
                self,
                Self::Lzma { level: _ } | Self::Xz { level: _ } | Self::Zstd { level: _ }
            )
        {
            command.arg("-T0");
        } else if source_date_epoch.is_some()
            && matches!(self, Self::Lzma { level: _ } | Self::Xz { level: _ })
        {
            command.arg("-T1");
        }
        command
    }

    pub fn decompress(&self, file: File) -> Result<ChildStdout> {
        let mut command = self.decompress_command();
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

    fn decompress_command(&self) -> Command {
        let mut command = Command::new(self.command());
        match self {
            Self::Bzip2 { level: _ }
            | Self::Gzip { level: _ }
            | Self::Lz4 { level: _ }
            | Self::Lzma { level: _ }
            | Self::Lzop { level: _ }
            | Self::Xz { level: _ } => {
                command.arg("-cd");
            }
            Self::Zstd { level: _ } => {
                command.arg("-cdq");
            }
            Self::Uncompressed => {}
            #[cfg(test)]
            Self::NonExistent | Self::Failing => {}
        };
        command
    }

    pub fn is_uncompressed(&self) -> bool {
        matches!(self, Self::Uncompressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TEST_LOCK;

    #[test]
    fn test_compression_decompress_program_not_found() {
        let _lock = TEST_LOCK.lock().unwrap();
        let archive = File::open("tests/single.cpio").expect("test cpio should be present");
        let compression = Compression::NonExistent;
        let got = compression.decompress(archive).unwrap_err();
        assert_eq!(got.kind(), ErrorKind::Other);
        assert_eq!(
            got.to_string(),
            "Program 'non-existing-program' not found in PATH."
        );
    }

    #[test]
    fn test_compression_from_command_line_lz4() {
        let compression = Compression::from_command_line(" lz4 ").unwrap();
        assert_eq!(compression, Compression::Lz4 { level: None });
    }

    #[test]
    fn test_compression_from_command_line_xz_6() {
        let compression = Compression::from_command_line("  xz \t -6 ").unwrap();
        assert_eq!(compression, Compression::Xz { level: Some(6) });
    }
}
