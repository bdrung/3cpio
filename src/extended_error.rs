// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::io::Error;

pub(crate) trait ExtendedError {
    fn add_prefix<S: AsRef<str>>(self, filename: S) -> Self;
    fn add_line(self, line_number: usize) -> Self;
}

impl ExtendedError for Error {
    fn add_prefix<S: AsRef<str>>(self, prefix: S) -> Self {
        Self::new(self.kind(), format!("{}: {self}", prefix.as_ref()))
    }

    fn add_line(self, line_number: usize) -> Self {
        Self::new(self.kind(), format!("line {line_number}: {self}"))
    }
}
