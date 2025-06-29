// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::fs::File;
use std::io::{Error, ErrorKind, Result};
use std::process::{ChildStdout, Command, Stdio};

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
        }
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
            Self::NonExistent => {}
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
        let file = File::open("tests/single.cpio").expect("test cpio should be present");
        let compression = Compression::NonExistent;
        let got = compression.decompress(file).unwrap_err();
        assert_eq!(got.kind(), ErrorKind::Other);
        assert_eq!(
            got.to_string(),
            "Program 'non-existing-program' not found in PATH."
        );
    }
}
