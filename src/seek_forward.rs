// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::fs::File;
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};
use std::process::ChildStdout;

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
        self.seek(SeekFrom::Current(offset.try_into().unwrap()))?;
        Ok(())
    }
}

impl SeekForward for ChildStdout {
    fn seek_forward(&mut self, offset: u64) -> Result<()> {
        let mut seek_reader = self.take(offset);
        let mut remaining: usize = offset.try_into().unwrap();
        let mut buffer = [0; PIPE_SIZE];
        while remaining > 0 {
            let read = seek_reader.read(&mut buffer)?;
            remaining -= read;
        }
        Ok(())
    }
}

impl SeekForward for &[u8] {
    fn seek_forward(&mut self, offset: u64) -> Result<()> {
        let mut seek_reader = std::io::Read::take(self, offset);
        let mut buffer = Vec::new();
        let read = seek_reader.read_to_end(&mut buffer)?;
        if read < offset.try_into().unwrap() {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                format!("read only {read} bytes, but {offset} wanted"),
            ));
        }
        Ok(())
    }
}
