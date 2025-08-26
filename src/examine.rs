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

    fn write<W: Write>(&self, out: &mut W) -> Result<()> {
        writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}",
            self.start,
            self.end,
            self.end - self.start,
            self.compression,
            self.extracted_size,
        )
    }
}

/// List the offsets of the cpio archives and their compression.
///
/// **Warning**: This function was designed for the `3cpio` command-line application.
/// The API can change between releases and no stability promises are given.
/// Please get in contact to support your use case and make the API for this function stable.
pub fn examine_cpio_content<W: Write>(mut archive: File, out: &mut W) -> Result<()> {
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
            examination.write(out)?;
            break;
        };
        magic_header = read_magic_header(&mut archive)?;
        end = archive.stream_position()?;
        let examination = Examination::new(start, end, compression.command(), size);
        examination.write(out)?;
    }
    Ok(())
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
