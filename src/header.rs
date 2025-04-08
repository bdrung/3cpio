// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::fs::Permissions;
use std::io::{Error, ErrorKind, Read, Result};
use std::os::unix::fs::PermissionsExt;

use crate::seek_forward::SeekForward;
use crate::{align_to_4_bytes, SeenFiles};

const CPIO_HEADER_LENGTH: u32 = 110;
const CPIO_MAGIC_NUMBER: [u8; 6] = *b"070701";

const MODE_PERMISSION_MASK: u32 = 0o007_777;
pub const MODE_FILETYPE_MASK: u32 = 0o770_000;
pub const FILETYPE_FIFO: u32 = 0o010_000;
pub const FILETYPE_CHARACTER_DEVICE: u32 = 0o020_000;
pub const FILETYPE_DIRECTORY: u32 = 0o040_000;
pub const FILETYPE_BLOCK_DEVICE: u32 = 0o060_000;
pub const FILETYPE_REGULAR_FILE: u32 = 0o100_000;
pub const FILETYPE_SYMLINK: u32 = 0o120_000;
pub const FILETYPE_SOCKET: u32 = 0o140_000;

#[derive(Debug, PartialEq)]
pub struct Header {
    pub ino: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub mtime: u32,
    pub filesize: u32,
    major: u32,
    minor: u32,
    pub rmajor: u32,
    pub rminor: u32,
    pub filename: String,
}

impl Header {
    #![allow(clippy::too_many_arguments)]
    #[cfg(test)]
    pub fn new<S>(
        ino: u32,
        mode: u32,
        uid: u32,
        gid: u32,
        nlink: u32,
        mtime: u32,
        filesize: u32,
        filename: S,
    ) -> Self
    where
        S: Into<String>,
    {
        Self {
            ino,
            mode,
            uid,
            gid,
            nlink,
            mtime,
            filesize,
            major: 0,
            minor: 0,
            rmajor: 0,
            rminor: 0,
            filename: filename.into(),
        }
    }

    // Return major and minor combined as u64
    fn dev(&self) -> u64 {
        (u64::from(self.major) << 32) | u64::from(self.minor)
    }

    pub fn mode_perm(&self) -> u32 {
        self.mode & MODE_PERMISSION_MASK
    }

    // ls-style ASCII representation of the mode
    pub fn mode_string(&self) -> [u8; 10] {
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

    pub fn permission(&self) -> Permissions {
        PermissionsExt::from_mode(self.mode & MODE_PERMISSION_MASK)
    }

    fn ino_and_dev(&self) -> u128 {
        (u128::from(self.ino) << 64) | u128::from(self.dev())
    }

    pub fn mark_seen(&self, seen_files: &mut SeenFiles) {
        seen_files.insert(self.ino_and_dev(), self.filename.clone());
    }

    pub fn read<R: Read>(file: &mut R) -> Result<Self> {
        let mut buffer = [0; CPIO_HEADER_LENGTH as usize];
        file.read_exact(&mut buffer)?;
        check_begins_with_cpio_magic_header(&buffer)?;
        let namesize = hex_str_to_u32(&buffer[94..102])?;
        let filename = read_filename(file, namesize)?;
        Ok(Self {
            ino: hex_str_to_u32(&buffer[6..14])?,
            mode: hex_str_to_u32(&buffer[14..22])?,
            uid: hex_str_to_u32(&buffer[22..30])?,
            gid: hex_str_to_u32(&buffer[30..38])?,
            nlink: hex_str_to_u32(&buffer[38..46])?,
            mtime: hex_str_to_u32(&buffer[46..54])?,
            filesize: hex_str_to_u32(&buffer[54..62])?,
            major: hex_str_to_u32(&buffer[62..70])?,
            minor: hex_str_to_u32(&buffer[70..78])?,
            rmajor: hex_str_to_u32(&buffer[78..86])?,
            rminor: hex_str_to_u32(&buffer[86..94])?,
            filename,
        })
    }

    pub fn read_only_filesize_and_filename<R: Read>(file: &mut R) -> Result<(u32, String)> {
        let mut header = [0; CPIO_HEADER_LENGTH as usize];
        file.read_exact(&mut header)?;
        check_begins_with_cpio_magic_header(&header)?;
        let filesize = hex_str_to_u32(&header[54..62])?;
        let namesize = hex_str_to_u32(&header[94..102])?;
        let filename = read_filename(file, namesize)?;
        Ok((filesize, filename))
    }

    pub fn read_symlink_target<R: Read>(&self, file: &mut R) -> Result<String> {
        let align = align_to_4_bytes(self.filesize);
        let mut target_bytes = vec![0u8; (self.filesize + align).try_into().unwrap()];
        file.read_exact(&mut target_bytes)?;
        target_bytes.truncate(self.filesize.try_into().unwrap());
        // TODO: propper name reading handling
        let target = std::str::from_utf8(&target_bytes).unwrap();
        Ok(target.into())
    }

    pub fn skip_file_content<R: SeekForward>(&self, file: &mut R) -> Result<()> {
        if self.filesize == 0 {
            return Ok(());
        };
        let skip = self.filesize + align_to_4_bytes(self.filesize);
        file.seek_forward(skip.into())?;
        Ok(())
    }

    pub fn try_get_hard_link_target<'a>(&self, seen_files: &'a SeenFiles) -> Option<&'a String> {
        if self.nlink <= 1 {
            return None;
        }
        seen_files.get(&self.ino_and_dev())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_read() {
        // Wrapped before mtime and filename
        let cpio_data = b"07070100000002000081B4000003E8000007D000000001\
            661BE5C600000008000000000000000000000000000000000000000A00000000\
            path/file\0content\0";
        let header = Header::read(&mut cpio_data.as_ref()).unwrap();
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
                rmajor: 0,
                rminor: 0,
                filename: "path/file".into()
            }
        )
    }

    #[test]
    fn test_header_read_invalid_magic_number() {
        let invalid_data = b"abc\tefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\
            abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let got = Header::read(&mut invalid_data.as_ref()).unwrap_err();
        assert_eq!(got.kind(), ErrorKind::InvalidData);
        assert_eq!(
            got.to_string(),
            "Invalid CPIO magic number 'abc\\tef'. Expected 070701"
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
}
