// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::env::{self, current_dir, set_current_dir};
use std::io::Result;
use std::path::PathBuf;
use std::time::SystemTime;

pub struct TempDir {
    pub path: PathBuf,
    cwd: PathBuf,
}

impl TempDir {
    pub fn new() -> Result<Self> {
        // Use some very pseudo-random number
        let cwd = current_dir()?;
        let path = create_tempdir()?;
        Ok(Self { path, cwd })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = set_current_dir(&self.cwd);
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
