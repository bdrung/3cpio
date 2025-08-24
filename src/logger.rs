// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::io::{Stderr, Write};

/// An enum representing the available verbosity levels of the logger.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum Level {
    /// The warning level. Designates hazardous situations.
    Warning = 5,
    /// The info level. Designates useful information.
    Info = 7,
    /// The debug level. Designates lower priority information and for debugging.
    Debug = 8,
}

macro_rules! debug {
    ($dst:ident, $($arg:tt)*) => {
        if $dst.is_enabled_for_debug() {
            writeln!($dst.out, $($arg)*)
        } else {
            Ok(())
        }
    };
}

macro_rules! info {
    ($dst:ident, $($arg:tt)*) => {
        if $dst.is_enabled_for_info() {
            writeln!($dst.out, $($arg)*)
        } else {
            Ok(())
        }
    };
}

/// Simple logging implementation that logs to a writer and supports log levels.
///
/// In contrast to the common `log` crate, the `Logger` needs to be specified
/// as parameter in the logging macros.
#[derive(Debug)]
pub struct Logger<W: Write> {
    level: Level,
    pub(crate) out: W,
}

impl<W: Write> Logger<W> {
    pub(crate) fn is_enabled_for_debug(&self) -> bool {
        self.level >= Level::Debug
    }

    pub(crate) fn is_enabled_for_info(&self) -> bool {
        self.level >= Level::Info
    }
}

impl Logger<Stderr> {
    /// Create a new `Logger` that logs to standard error (stderr).
    pub fn new_stderr(level: Level) -> Self {
        Self {
            level,
            out: std::io::stderr(),
        }
    }
}

#[cfg(test)]
impl Logger<Vec<u8>> {
    pub(crate) fn new_vec(level: Level) -> Self {
        Self {
            level,
            out: Vec::new(),
        }
    }

    pub(crate) fn get_logs(&self) -> &str {
        core::str::from_utf8(&self.out).unwrap()
    }
}
