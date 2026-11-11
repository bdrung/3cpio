// Copyright (C) 2024-2026, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::fs::File;
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};
use std::process::ChildStdout;

const PIPE_SIZE: usize = 65536;

pub(crate) trait SeekForward {
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
            if read == 0 {
                return Err(Error::new(
                    ErrorKind::UnexpectedEof,
                    format!("missing {remaining} bytes while seeking forward {offset} bytes"),
                ));
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn child_stdout_seek_forward_detects_eof() {
        for (offset, expected) in [(0, None), (3, None), (4, Some(ErrorKind::UnexpectedEof))] {
            let mut child = Command::new("printf")
                .arg("abc")
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut stream = child.stdout.take().unwrap();
            assert_eq!(
                stream.seek_forward(offset).err().map(|error| error.kind()),
                expected
            );
            if offset == 0 {
                let mut output = String::new();
                stream.read_to_string(&mut output).unwrap();
                assert_eq!(output, "abc");
            }
            assert!(child.wait().unwrap().success());
        }
    }
}
