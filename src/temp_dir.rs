// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::env::{self, current_dir, set_current_dir};
use std::fs::File;
use std::io::{Read, Result, Write};
use std::path::{Path, PathBuf};

pub struct TempDir {
    /// Path of the temporary directory.
    pub path: PathBuf,
    cwd: Option<PathBuf>,
}

impl TempDir {
    /// Create a file in the temporary directory and return full path.
    pub fn create<P: AsRef<Path>>(&self, filename: P, content: &[u8]) -> Result<String> {
        let path = self.path.join(filename);
        let mut file = File::create(&path)?;
        file.write_all(content)?;
        Ok(path.into_os_string().into_string().unwrap())
    }

    /// Creates a new temporary directory.
    ///
    /// This temporary directory and all the files it contains will be removed
    /// on drop.
    ///
    /// The temporary directory name is constructed by using `CARGO_PKG_NAME`
    /// followed by a dot and random alphanumeric characters.
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
    /// The temporary directory name is constructed by using `CARGO_PKG_NAME`
    /// followed by a dot and random alphanumeric characters.
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

/// Similar to base64 encoding (but without last two elements)
fn base62_encode(byte: u8) -> char {
    let lowerbits: u8 = byte % 62;
    let char = match lowerbits {
        0..=25 => lowerbits + 65,
        26..=51 => lowerbits - 26 + 97,
        52..=61 => lowerbits - 52 + 48,
        _ => unreachable!(),
    };
    char.into()
}

fn create_tempdir() -> Result<PathBuf> {
    let mut random = [0u8; 10];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let name = std::option_env!("CARGO_PKG_NAME").unwrap();
    let dir_builder = std::fs::DirBuilder::new();
    let mut path = env::temp_dir();
    path.push(format!("{name}.{}", to_alphanumerics(&random)));
    dir_builder.create(&path)?;
    Ok(path)
}

/// Encode given data in alphanumeric characters.
///
/// For simplicity throw away some bits of the given data.
fn to_alphanumerics(data: &[u8]) -> String {
    let mut encoded = String::new();
    for byte in data {
        encoded.push(base62_encode(*byte));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base62_encode() {
        assert_eq!(base62_encode(0), 'A');
        assert_eq!(base62_encode(27), 'b');
        assert_eq!(base62_encode(55), '3');
        assert_eq!(base62_encode(118), '4');
    }

    #[test]
    fn test_to_alphanumerics() {
        assert_eq!(to_alphanumerics(b"\x2c\x37\xeb\x18"), "s3xY");
    }
}
