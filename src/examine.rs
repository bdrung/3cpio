// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::fs::File;
use std::io::{Read, Result, Seek, Write};
use std::os::unix::fs::MetadataExt;

use crate::compression::read_magic_header;
use crate::header::{read_file_name_and_size_from_next_cpio_object, TRAILER_FILENAME};
use crate::seek_forward::SeekForward;

struct Examination<'a> {
    start: u64,
    end: u64,
    compression: &'a str,
    extracted_size: u64,
}

impl<'a> Examination<'a> {
    fn new(start: u64, end: u64, compression: &'a str, extracted_size: u64) -> Self {
        Examination {
            start,
            end,
            compression,
            extracted_size,
        }
    }

    fn write<W: Write>(&self, out: &mut W, raw: bool) -> Result<()> {
        if raw {
            writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}",
                self.start,
                self.end,
                self.end - self.start,
                self.compression,
                self.extracted_size,
            )
        } else {
            writeln!(
                out,
                "{:<7}   {:<7}   {:<7}   {:<6}   {}",
                format_bytes(self.start),
                format_bytes(self.end),
                format_bytes(self.end - self.start),
                self.compression,
                format_bytes(self.extracted_size),
            )
        }
    }

    fn write_header<W: Write>(out: &mut W, raw: bool) -> Result<()> {
        if !raw {
            writeln!(out, "Start     End       Size      Compr.   Extracted")?;
        }
        Ok(())
    }
}

const fn div_round(value: u64, divisor: u64) -> u64 {
    (value + divisor / 2) / divisor
}

/// List the offsets of the cpio archives and their compression.
///
/// **Warning**: This function was designed for the `3cpio` command-line application.
/// The API can change between releases and no stability promises are given.
/// Please get in contact to support your use case and make the API for this function stable.
pub fn examine_cpio_content<W: Write>(mut archive: File, out: &mut W, raw: bool) -> Result<()> {
    Examination::write_header(out, raw)?;
    let mut end = archive.stream_position()?;
    let mut magic_header = read_magic_header(&mut archive)?;
    while let Some(compression) = magic_header {
        let start = end;
        let size = if compression.is_uncompressed() {
            read_file_sizes(&mut archive)?
        } else {
            // Assume that the compressor command will read the file to the end.
            let end = archive.metadata()?.size();
            let mut decompressed = compression.decompress(archive)?;
            let size = read_file_sizes(&mut decompressed)?;
            let examination = Examination::new(start, end, compression.command(), size);
            examination.write(out, raw)?;
            break;
        };
        magic_header = read_magic_header(&mut archive)?;
        end = archive.stream_position()?;
        let examination = Examination::new(start, end, compression.command(), size);
        examination.write(out, raw)?;
    }
    Ok(())
}

fn format_bytes(value: u64) -> String {
    if value < 1000 {
        format!("{} B", value)
    } else if value < 10000 {
        format!("{:.2} kB", f64::from(value as u32) / 1000.0)
    } else if value < 100000 {
        format!("{:.1} kB", f64::from(value as u32) / 1000.0)
    } else if value < 1000000 {
        format!("{} kB", div_round(value, 1000))
    } else if value < 10000000 {
        format!("{:.2} MB", f64::from(value as u32) / 1000000.0)
    } else if value < 100000000 {
        format!("{:.1} MB", f64::from(value as u32) / 1000000.0)
    } else {
        format!("{} MB", div_round(value, 1000000))
    }
}

fn read_file_sizes<R: Read + SeekForward>(archive: &mut R) -> Result<u64> {
    let mut file_sizes = 0;
    loop {
        let (filename, size) = read_file_name_and_size_from_next_cpio_object(archive)?;
        file_sizes += u64::from(size);
        if filename == TRAILER_FILENAME {
            break;
        }
    }
    Ok(file_sizes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::tests_path;

    #[test]
    fn test_examine_cpio_content() {
        let archive = File::open(tests_path("bigdata.cpio")).unwrap();
        let mut output = Vec::new();
        examine_cpio_content(archive, &mut output, false).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "Start     End       Size      Compr.   Extracted\n\
            0 B       512 B     512 B     cpio     8 B\n\
            512 B     1.54 kB   1.02 kB   cpio     56 B\n\
            1.54 kB   4.83 kB   3.29 kB   zstd     103 MB\n"
        );
    }

    #[test]
    fn test_format_bytes_kilobytes_with_dot() {
        assert_eq!(format_bytes(12345), "12.3 kB");
    }

    #[test]
    fn test_format_bytes_kilobytes_without_dot() {
        assert_eq!(format_bytes(543210), "543 kB");
    }

    #[test]
    fn test_format_bytes_megabytes_two_decimal_places() {
        assert_eq!(format_bytes(7415000), "7.42 MB");
    }

    #[test]
    fn test_format_bytes_megabytes_one_decimal_place() {
        assert_eq!(format_bytes(83684618), "83.7 MB");
    }
}
