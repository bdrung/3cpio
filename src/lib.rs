// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::fs::File;
use std::io::prelude::*;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::io::SeekFrom;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;

const CPIO_HEADER_LENGTH: u32 = 110;
const CPIO_MAGIC_NUMBER: [u8; 6] = *b"070701";
const PIPE_SIZE: usize = 65536;

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
        // TODO: Use i64::try_from(offset)
        self.seek(SeekFrom::Current(offset as i64))?;
        Ok(())
    }
}

impl SeekForward for ChildStdout {
    fn seek_forward(&mut self, offset: u64) -> Result<()> {
        let mut seek_reader = self.take(offset);
        // TODO: check offset fits into usize
        let mut remaining = offset as usize;
        let mut buffer = [0; PIPE_SIZE];
        while remaining > 0 {
            let read = seek_reader.read(&mut buffer)?;
            remaining -= read;
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

fn align_to_4_bytes(length: u32) -> u32 {
    let unaligned = length % 4;
    if unaligned == 0 {
        0
    } else {
        4 - unaligned
    }
}

fn hex_str_to_u32(bytes: &[u8]) -> Result<u32> {
    // TODO: propper error handling
    let s = std::str::from_utf8(bytes).unwrap();
    Ok(u32::from_str_radix(s, 16).unwrap())
}

fn read_filename<R: Read>(file: &mut R, namesize: u32) -> Result<String> {
    let header_align = align_to_4_bytes(CPIO_HEADER_LENGTH + namesize);
    // TODO: Check namesize to fit into usize
    let mut filename_bytes = vec![0u8; (namesize + header_align) as usize];
    file.read_exact(&mut filename_bytes)?;
    if filename_bytes[namesize as usize - 1] != 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Entry name '{:?}' is not NULL-terminated",
                &filename_bytes[0..namesize as usize - 1]
            ),
        ));
    }
    filename_bytes.truncate(namesize as usize - 1);
    // TODO: propper name reading handling
    let filename = std::str::from_utf8(&filename_bytes).unwrap();
    Ok(filename.to_string())
}

/// Read only the file name from the next cpio object.
///
/// Read the next cpio object header, check the magic, skip the file data.
/// Return the file name.
fn read_filename_from_next_cpio_object<R: Read + SeekForward>(file: &mut R) -> Result<String> {
    let mut header = [0; CPIO_HEADER_LENGTH as usize];
    file.read_exact(&mut header)?;
    if header[0..6] != CPIO_MAGIC_NUMBER {
        // TODO: Check this case
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid CPIO magic number '{}{}{}{}{}{}'. Expected {}{}{}{}{}{}",
                header[0],
                header[1],
                header[2],
                header[3],
                header[4],
                header[5],
                CPIO_MAGIC_NUMBER[0],
                CPIO_MAGIC_NUMBER[1],
                CPIO_MAGIC_NUMBER[2],
                CPIO_MAGIC_NUMBER[3],
                CPIO_MAGIC_NUMBER[4],
                CPIO_MAGIC_NUMBER[5]
            ),
        ));
    }
    let filesize = hex_str_to_u32(&header[54..62])?;
    let namesize = hex_str_to_u32(&header[94..102])?;
    let filename = read_filename(file, namesize)?;

    let skip = filesize + align_to_4_bytes(filesize);
    file.seek_forward(skip as u64)?;
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

pub fn list_cpio_content<W: Write>(mut file: File, out: &mut W) -> Result<()> {
    loop {
        let mut command = match read_magic_header(&mut file) {
            None => return Ok(()),
            Some(x) => x?,
        };
        if command.get_program() == "cpio" {
            read_cpio_and_print_filenames(&mut file, out)?;
        } else {
            let mut decompressed = decompress(&mut command, file)?;
            read_cpio_and_print_filenames(&mut decompressed, out)?;
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_to_4_bytes() {
        assert_eq!(align_to_4_bytes(110), 2);
    }

    #[test]
    fn test_align_to_4_bytes_is_aligned() {
        assert_eq!(align_to_4_bytes(32), 0);
    }
}
