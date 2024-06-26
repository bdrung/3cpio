// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::env;
use std::error::Error;
use std::process::{Command, Output};

// Derive target directory (e.g. `target/debug`) from current executable
fn get_target_dir() -> std::path::PathBuf {
    let mut path = env::current_exe().expect("env::current_exe not set");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path
}

fn get_command() -> Command {
    let mut program = get_target_dir();
    program.push("3cpio");
    Command::new(program)
}

trait ExitCodeAssertion {
    fn assert_failure(self, expected_code: i32) -> Self;
    fn assert_success(self) -> Self;
}

impl ExitCodeAssertion for Output {
    fn assert_failure(self, expected_code: i32) -> Self {
        assert_eq!(self.status.code().expect("exit code"), expected_code);
        self
    }

    fn assert_success(self) -> Self {
        assert!(self.status.success());
        self
    }
}

trait OutputAssertion<S> {
    fn assert_stderr(self, expected: S) -> Self;
    fn assert_stdout(self, expected: S) -> Self;
}

impl<S> OutputAssertion<S> for Output
where
    String: PartialEq<S>,
    S: std::fmt::Debug,
{
    fn assert_stderr(self, expected: S) -> Self {
        let stderr = String::from_utf8(self.stderr.clone()).expect("stderr");
        assert_eq!(stderr, expected);
        self
    }

    fn assert_stdout(self, expected: S) -> Self {
        let stdout = String::from_utf8(self.stdout.clone()).expect("stdout");
        assert_eq!(stdout, expected);
        self
    }
}

trait OutputContainsAssertion {
    fn assert_stderr_contains(self, expected: &str) -> Self;
}

impl OutputContainsAssertion for Output {
    fn assert_stderr_contains(self, expected: &str) -> Self {
        let stderr = String::from_utf8(self.stderr.clone()).expect("stderr");
        assert!(
            stderr.contains(expected),
            "'{}' not found in '{}'",
            expected,
            stderr
        );
        self
    }
}

#[test]
fn examine_compressed_cpio() -> Result<(), Box<dyn Error>> {
    for compression in ["bzip2", "gzip", "lz4", "lzop", "xz", "zstd"] {
        let mut cmd = get_command();
        cmd.arg("-e").arg(format!("tests/{}.cpio", compression));

        cmd.output()?
            .assert_stderr("")
            .assert_success()
            .assert_stdout(format!("0\tcpio\n512\t{}\n", compression));
    }
    Ok(())
}

#[test]
fn examine_single_cpio() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-e").arg("tests/single.cpio");

    cmd.output()?.assert_success().assert_stdout("0\tcpio\n");
    Ok(())
}

#[test]
fn file_doesnt_exist() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-t").arg("test/file/does/not/exist");

    cmd.output()?
        .assert_failure(1)
        .assert_stderr_contains("No such file or directory")
        .assert_stdout("");
    Ok(())
}

#[test]
fn list_content_compressed_cpio() -> Result<(), Box<dyn Error>> {
    for compression in ["bzip2", "gzip", "lz4", "lzop", "xz", "zstd"] {
        let mut cmd = get_command();
        cmd.arg("-t").arg(format!("tests/{}.cpio", compression));

        cmd.output()?
            .assert_stderr("")
            .assert_success()
            .assert_stdout(".\npath\npath/file\n.\nusr\nusr/bin\nusr/bin/sh\n");
    }
    Ok(())
}

#[test]
fn list_content_single_cpio() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-t").arg("tests/single.cpio");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout(".\npath\npath/file\n");
    Ok(())
}

#[test]
fn missing_file_argument() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-t");

    cmd.output()?
        .assert_failure(2)
        .assert_stderr_contains("missing argument FILE")
        .assert_stdout("");
    Ok(())
}

#[test]
fn print_version() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--version");

    let stdout = cmd.output()?.assert_stderr("").assert_success().stdout;
    let stdout = String::from_utf8(stdout).expect("stdout");
    let words: Vec<&str> = stdout.split_whitespace().collect();
    assert_eq!(words.len(), 2, "not two words: '{}'", stdout);
    assert_eq!(words[0], "3cpio");

    let version = words[1];
    // Simple implementation for regular expression match: [0-9.]+
    let mut matches = String::from(version);
    matches.retain(|c| c.is_ascii_digit() || c == '.');
    assert_eq!(matches, version);
    Ok(())
}
