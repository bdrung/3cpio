// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::time::SystemTime;

use threecpio::temp_dir::TempDir;

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
    fn assert_stdout_contains(self, expected: &str) -> Self;
}

impl OutputContainsAssertion for Output {
    fn assert_stderr_contains(self, expected: &str) -> Self {
        let stderr = String::from_utf8(self.stderr.clone()).expect("stderr");
        assert!(
            stderr.contains(expected),
            "'{expected}' not found in '{stderr}'",
        );
        self
    }

    fn assert_stdout_contains(self, expected: &str) -> Self {
        let stdout = String::from_utf8(self.stdout.clone()).expect("stdout");
        assert!(
            stdout.contains(expected),
            "'{expected}' not found in '{stdout}'",
        );
        self
    }
}

#[test]
fn test_create_cpio_on_stdout() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--create");
    let process = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
    let mut stdin = process.stdin.as_ref().unwrap();
    stdin.write_all(b"/usr\t\t\t\t\t\t1681992796\n")?;
    let output = process.wait_with_output()?;
    assert_eq!(
        std::str::from_utf8(&output.stdout).unwrap(),
        "07070100000000000041ED00000000000000000000000264412C5C\
        00000000000000000000000000000000000000000000000400000000\
        usr\0\0\0\
        070701000000000000000000000000000000000000000100000000\
        00000000000000000000000000000000000000000000000B00000000\
        TRAILER!!!\0\0\0\0",
    );
    Ok(())
}

#[test]
fn test_create_cpio_file() -> Result<(), Box<dyn Error>> {
    let temp_dir = TempDir::new()?;
    let mut path = temp_dir.path.clone();
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    path.push(format!("3cpio-{now:?}.cpio"));
    let path = path.into_os_string().into_string().unwrap();

    let mut cmd = get_command();
    cmd.args(["--create", &path]);
    let process = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
    let mut stdin = process.stdin.as_ref().unwrap();
    stdin.write_all(b"/usr\t\t\t\t\t\t1681992796\n")?;
    process.wait_with_output()?.assert_stdout("");

    let mut cpio = Vec::new();
    let mut cpio_file = File::open(&path)?;
    cpio_file.read_to_end(&mut cpio)?;
    assert_eq!(
        std::str::from_utf8(&cpio).unwrap(),
        "07070100000000000041ED00000000000000000000000264412C5C\
        00000000000000000000000000000000000000000000000400000000\
        usr\0\0\0\
        070701000000000000000000000000000000000000000100000000\
        00000000000000000000000000000000000000000000000B00000000\
        TRAILER!!!\0\0\0\0",
    );
    Ok(())
}

#[test]
fn test_count_cpio_archives() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--count").arg("tests/zstd.cpio");

    cmd.output()?.assert_success().assert_stdout("2\n");
    Ok(())
}

#[test]
fn test_count_unexpected_argument() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--count").arg("tests/single.cpio").arg("foobar");

    cmd.output()?
        .assert_failure(2)
        .assert_stderr_contains("Error: unexpected argument \"foobar\"")
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_examine_compressed_cpio() -> Result<(), Box<dyn Error>> {
    for compression in ["bzip2", "gzip", "lz4", "lzop", "xz", "zstd"] {
        let mut cmd = get_command();
        cmd.arg("-e").arg(format!("tests/{compression}.cpio"));

        cmd.output()?
            .assert_stderr("")
            .assert_success()
            .assert_stdout(format!("0\tcpio\n512\t{compression}\n"));
    }
    Ok(())
}

#[test]
fn test_examine_single_cpio() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-e").arg("tests/single.cpio");

    cmd.output()?.assert_success().assert_stdout("0\tcpio\n");
    Ok(())
}

#[test]
fn test_extract_to_stdout() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-x")
        .arg("--to-stdout")
        .arg("tests/gzip.cpio")
        .arg("path/f?le");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout("content\n");
    Ok(())
}

#[test]
fn test_extract_with_subdir() -> Result<(), Box<dyn Error>> {
    let tempdir = TempDir::new()?;
    let mut cmd = get_command();
    cmd.arg("-x")
        .arg("-C")
        .arg(&tempdir.path)
        .arg("-s")
        .arg("subdir")
        .arg("-v")
        .arg("tests/lz4.cpio");

    println!("tempdir = {:?}", tempdir.path);
    cmd.output()?
        .assert_stderr(".\npath\npath/file\n.\nusr\nusr/bin\nusr/bin/sh\n")
        .assert_success()
        .assert_stdout("");
    assert!(tempdir.path.join("subdir1/path/file").exists());
    assert!(tempdir.path.join("subdir2/usr/bin/sh").exists());
    Ok(())
}

#[test]
fn test_help() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--help");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout_contains("Extract the cpio archives into separate directories");
    Ok(())
}

#[test]
fn test_archive_doesnt_exist() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-t").arg("test/file/does/not/exist");

    cmd.output()?
        .assert_failure(1)
        .assert_stderr_contains("No such file or directory")
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_invalid_pattern() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-t").arg("tests/single.cpio").arg("[abc.txt");

    cmd.output()?
        .assert_failure(2)
        .assert_stderr_contains("Error: invalid pattern '[abc.txt'")
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_list_content_compressed_cpio() -> Result<(), Box<dyn Error>> {
    for compression in ["bzip2", "gzip", "lz4", "lzma", "lzop", "xz", "zstd"] {
        let mut cmd = get_command();
        cmd.arg("-t").arg(format!("tests/{compression}.cpio"));

        cmd.output()?
            .assert_stderr("")
            .assert_success()
            .assert_stdout(".\npath\npath/file\n.\nusr\nusr/bin\nusr/bin/sh\n");
    }
    Ok(())
}

#[test]
fn test_list_content_single_cpio() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-t").arg("tests/single.cpio");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout(".\npath\npath/file\n");
    Ok(())
}

#[test]
fn test_list_content_single_cpio_with_pattern() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-t").arg("tests/single.cpio").arg("p?th");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout("path\n");
    Ok(())
}

#[test]
fn test_missing_archive_argument() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-t");

    cmd.output()?
        .assert_failure(2)
        .assert_stderr_contains("missing argument ARCHIVE")
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_print_version() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--version");

    let stdout = cmd.output()?.assert_stderr("").assert_success().stdout;
    let stdout = String::from_utf8(stdout).expect("stdout");
    let words: Vec<&str> = stdout.split_whitespace().collect();
    assert_eq!(words.len(), 2, "not two words: '{stdout}'");
    assert_eq!(words[0], "3cpio");

    let version = words[1];
    // Simple implementation for regular expression match: [0-9.]+
    let mut matches = String::from(version);
    matches.retain(|c| c.is_ascii_digit() || c == '.');
    assert_eq!(matches, version);
    Ok(())
}

#[test]
fn test_unexpected_option() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--foobar");

    cmd.output()?
        .assert_failure(2)
        .assert_stderr_contains("Error: invalid option '--foobar'")
        .assert_stdout("");
    Ok(())
}
