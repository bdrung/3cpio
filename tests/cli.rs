// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::error::Error;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn examine_compressed_cpio() -> Result<(), Box<dyn Error>> {
    for compression in ["bzip2", "gzip", "lz4", "lzop", "xz", "zstd"] {
        let mut cmd = Command::cargo_bin("3cpio")?;
        cmd.arg("-e").arg(format!("tests/{}.cpio", compression));

        cmd.assert()
            .success()
            .stdout(format!("0\tcpio\n512\t{}\n", compression));
    }
    Ok(())
}

#[test]
fn examine_single_cpio() -> Result<(), Box<dyn Error>> {
    let mut cmd = Command::cargo_bin("3cpio")?;
    cmd.arg("-e").arg("tests/single.cpio");

    cmd.assert().success().stdout("0\tcpio\n");
    Ok(())
}

#[test]
fn file_doesnt_exist() -> Result<(), Box<dyn Error>> {
    let mut cmd = Command::cargo_bin("3cpio")?;
    cmd.arg("-t").arg("test/file/does/not/exist");

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("No such file or directory"));
    Ok(())
}

#[test]
fn list_content_compressed_cpio() -> Result<(), Box<dyn Error>> {
    for compression in ["bzip2", "gzip", "lz4", "lzop", "xz", "zstd"] {
        let mut cmd = Command::cargo_bin("3cpio")?;
        cmd.arg("-t").arg(format!("tests/{}.cpio", compression));

        cmd.assert()
            .success()
            .stdout(".\npath\npath/file\n.\nusr\nusr/bin\nusr/bin/sh\n");
    }
    Ok(())
}

#[test]
fn list_content_single_cpio() -> Result<(), Box<dyn Error>> {
    let mut cmd = Command::cargo_bin("3cpio")?;
    cmd.arg("-t").arg("tests/single.cpio");

    cmd.assert().success().stdout(".\npath\npath/file\n");
    Ok(())
}

#[test]
fn missing_file_argument() -> Result<(), Box<dyn Error>> {
    let mut cmd = Command::cargo_bin("3cpio")?;
    cmd.arg("-t");

    cmd.assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("missing argument FILE"));
    Ok(())
}

#[test]
fn print_version() -> Result<(), Box<dyn Error>> {
    let mut cmd = Command::cargo_bin("3cpio")?;
    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::is_match("^[a-z]+ [0-9.]+\n$").unwrap());
    Ok(())
}
