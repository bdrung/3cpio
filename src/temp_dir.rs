// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::env::{self, current_dir, set_current_dir};
use std::io::Result;
use std::path::PathBuf;
use std::time::SystemTime;

pub struct TempDir {
    /// Path of the temporary directory.
    pub path: PathBuf,
    cwd: Option<PathBuf>,
}

impl TempDir {
    /// Creates a new temporary directory.
    ///
    /// This temporary directory and all the files it contains will be removed
    /// on drop.
    ///
    /// **Warning**: The temporary directory is constructed by using `CARGO_PKG_NAME`
    /// plus the UNIX timestamp in nanoseconds. This directory name could be guessed
    /// and result in "insecure temporary file" security vulnerabilities. Therefore
    /// this function is only meant for being use in test cases.
    ///
    /// # Examples
    ///
    /// ```
    /// use threecpio::temp_dir::TempDir;
    ///
    /// let tempdir = TempDir::new().unwrap();
    /// println!("Temporary directory: {}", tempdir.path.display());
    /// ```
    pub fn new() -> Result<Self> {
        let path = create_tempdir()?;
        Ok(Self { path, cwd: None })
    }

    /// Creates a new temporary directory and changes the current working
    /// directory to this directory.
    ///
    /// This temporary directory and all the files it contains will be removed
    /// on drop. The current working directory will be set back on drop as well.
    ///
    /// **Warning**: The temporary directory is constructed by using `CARGO_PKG_NAME`
    /// plus the UNIX timestamp in nanoseconds. This directory name could be guessed
    /// and result in "insecure temporary file" security vulnerabilities. Therefore
    /// this function is only meant for being use in test cases.
    pub fn new_and_set_current_dir() -> Result<Self> {
        let path = create_tempdir()?;
        let cwd = current_dir()?;
        set_current_dir(&path)?;
        Ok(Self {
            path,
            cwd: Some(cwd),
        })
    }
}

impl Drop for TempDir {
    /// Removes the temporary directory and all the files it contains.
    fn drop(&mut self) {
        if let Some(cwd) = self.cwd.as_ref() {
            let _ = set_current_dir(cwd);
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn create_tempdir() -> Result<PathBuf> {
    // Use some very pseudo-random number
    let epoch = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let name = std::option_env!("CARGO_PKG_NAME").unwrap();
    let dir_builder = std::fs::DirBuilder::new();
    let mut path = env::temp_dir();
    path.push(format!("{name}-{}", epoch.subsec_nanos()));
    dir_builder.create(&path)?;
    Ok(path)
}
