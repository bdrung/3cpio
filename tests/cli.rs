// Copyright (C) 2024-2026, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::env;
use std::error::Error;
use std::fs::{symlink_metadata, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::os::unix::net::UnixStream;
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
    if let Ok(path) = std::env::var("THREECPIO_BIN") {
        return Command::new(path);
    }

    let mut program = get_target_dir();
    program.push("3cpio");
    Command::new(program)
}

fn program_not_available(program: &str) -> bool {
    let mut cmd = Command::new(program);
    cmd.arg("--help");
    cmd.output().is_err_and(|e| e.kind() == ErrorKind::NotFound)
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
fn test_create_compressed_cpio_file() -> Result<(), Box<dyn Error>> {
    let temp_dir = TempDir::new()?;
    let path = temp_dir.path.join("empty.cpio");
    let path = path.into_os_string().into_string().unwrap();

    let mut cmd = get_command();
    cmd.args(["--create", &path])
        .env("SOURCE_DATE_EPOCH", "1754509394")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let process = cmd.spawn()?;
    let mut stdin = process.stdin.as_ref().unwrap();
    stdin.write_all(b"#cpio: lz4 -0\n")?;
    process
        .wait_with_output()?
        .assert_stdout("")
        .assert_stderr("Compression level 0 lower than minimum, raising to 1.\n");

    let mut cpio = Vec::new();
    let mut cpio_file = File::open(&path)?;
    cpio_file.read_to_end(&mut cpio)?;
    assert_eq!(
        cpio,
        b"\x02!L\x18%\0\0\0\x7f0707010\
        \x01\0\x13\x0f(\0\x15\x0c\x02\0\x14B\x11\0\xe0T\
        RAILER!!!\0\0\0\0",
    );
    Ok(())
}

#[test]
fn test_create_compressed_cpio_file_no_source_date_epoch() -> Result<(), Box<dyn Error>> {
    let temp_dir = TempDir::new()?;
    let path = temp_dir.path.join("test.cpio");
    let path = path.into_os_string().into_string().unwrap();

    let mut cmd = get_command();
    cmd.args(["--create", &path])
        .env_remove("SOURCE_DATE_EPOCH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let process = cmd.spawn()?;
    let mut stdin = process.stdin.as_ref().unwrap();
    stdin.write_all(b"#cpio: gzip -1\ntests/generate\tgenerate\n")?;
    process
        .wait_with_output()?
        .assert_stdout("")
        .assert_stderr("");

    let mut cmd = get_command();
    cmd.arg("-x")
        .arg("-C")
        .arg(temp_dir.path.join("extracted"))
        .arg("-v")
        .arg(&path);
    cmd.output()?
        .assert_stderr("generate\n")
        .assert_success()
        .assert_stdout("");
    assert!(temp_dir.path.join("extracted/generate").exists());
    Ok(())
}

#[test]
fn test_create_compressed_cpio_on_stdout() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--create")
        .env("SOURCE_DATE_EPOCH", "1754504178")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let process = cmd.spawn()?;
    let mut stdin = process.stdin.as_ref().unwrap();
    stdin.write_all(b"#cpio: zstd -42\n/usr\t\t\t\t\t\t1681992796\n")?;
    let output = process
        .wait_with_output()?
        .assert_stderr("Compression level 42 higher than maximum, reducing to 19.\n");
    assert_eq!(
        output.stdout,
        b"(\xb5/\xfd$\xf0\x0d\x02\0\x02\xc3\x0a\x11\x90M\x07\
        \xa0\xff\x18S\x04G\xf3[\xc9\xb1\xef\x8eT\x06m\x0b\
        \0h\x8a-\xd3\xdc\xe7l\xfb`\\\x8c\x06\x0a)\x04\
        \x09'\x95\xe2\xbc\\\x0e\x08 \xc0s\x07\x19\xde\x89v\
        \xe16%\xc3\x9b\x88\xd2F1\x02\xd2\\\x1b:"
    );
    Ok(())
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
fn test_create_data_align() -> Result<(), Box<dyn Error>> {
    let temp_dir = TempDir::new()?;
    let path = temp_dir.create("example.txt", b"This is just an example text file!\n")?;
    let mut cmd = get_command();
    cmd.args(["--create", "--data-align", "16"]);
    let process = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
    let mut stdin = process.stdin.as_ref().unwrap();
    let manifest =
        format!("/usr\t\t\t\t\t\t1681992796\n{path}\tusr/file\t\t644\t3\t7\t1755046204\n");
    stdin.write_all(manifest.as_bytes())?;
    let output = process.wait_with_output()?;
    assert_eq!(
        std::str::from_utf8(&output.stdout).unwrap(),
        "07070100000000000041ED00000000000000000000000264412C5C\
        00000000000000000000000000000000000000000000000400000000\
        usr\0\0\0\
        07070100000001000081A4000000030000000700000001689BE13C\
        00000023000000000000000000000000000000000000000E00000000\
        usr/file\0\0\
        \0\0\0\0\
        This is just an example text file!\n\0\
        070701000000000000000000000000000000000000000100000000\
        00000000000000000000000000000000000000000000000B00000000\
        TRAILER!!!\0\0\0\0",
    );
    Ok(())
}

#[test]
fn test_create_data_align_negative_number() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.args(["--create", "--data-align", "-42", "/tmp/initrd"]);

    cmd.output()?
        .assert_failure(2)
        .assert_stderr_contains("Error: --data-align must be a positive number")
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_create_data_align_not_a_multiple_of_four() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.args(["--create", "--data-align", "7", "/tmp/initrd"]);

    cmd.output()?
        .assert_failure(2)
        .assert_stderr_contains("Error: --data-align must be a multiple of 4 bytes")
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_create_data_align_zero() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.args(["--create", "--data-align", "0", "/tmp/initrd"]);

    cmd.output()?
        .assert_failure(2)
        .assert_stderr_contains("Error: --data-align must be a positive number")
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_create_invalid_manifest() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--create")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let process = cmd.spawn()?;
    let mut stdin = process.stdin.as_ref().unwrap();
    stdin.write_all(b"#cpio: brotli\n")?;
    process
        .wait_with_output()?
        .assert_failure(1)
        .assert_stderr_contains(
            "Error: Failed to create 'cpio on stdout': \
            line 1: Unknown compression format: brotli",
        )
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_create_missing_path() -> Result<(), Box<dyn Error>> {
    let temp_dir = TempDir::new()?;
    let path = temp_dir.path.join("nonexistent").join("empty.cpio");
    let path = path.into_os_string().into_string().unwrap();

    let mut cmd = get_command();
    cmd.args(["--create", &path]);
    cmd.output()?
        .assert_failure(1)
        .assert_stderr_contains(&format!(
            "Error: Failed to create '{path}': No such file or directory"
        ))
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_create_uncompressed_plus_zstd_on_stdout() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--create");
    let process = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
    let mut stdin = process.stdin.as_ref().unwrap();
    stdin.write_all(b"#cpio\n/usr\t\t\t\t\t\t1681992796\n#cpio: zstd -2\n")?;
    let output = process.wait_with_output()?;
    assert_eq!(
        output.stdout,
        b"07070100000000000041ED00000000000000000000000264412C5C\
        00000000000000000000000000000000000000000000000400000000\
        usr\0\0\0\
        070701000000000000000000000000000000000000000100000000\
        00000000000000000000000000000000000000000000000B00000000\
        TRAILER!!!\0\0\0\0\
        (\xb5/\xfd$|\x15\x01\0\xc8070701\
        010B0TRAILER!!!\0\
        \0\0\0\x03\x10\0\x19\xde\x89?F\x95\xfb\x16m",
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
fn test_examine_compressed_cpio_raw() -> Result<(), Box<dyn Error>> {
    for compression in ["bzip2", "gzip", "lz4", "lzop", "xz", "zstd"] {
        if program_not_available(compression) {
            continue;
        }
        let path = format!("tests/{compression}.cpio");
        let mut cmd = get_command();
        cmd.arg("-e").arg(&path).arg("--raw");
        let size = symlink_metadata(&path)?.size();

        cmd.output()?
            .assert_stderr("")
            .assert_success()
            .assert_stdout(format!(
                "0\t512\t512\tcpio\t8\n512\t{size}\t{}\t{compression}\t58\n",
                size - 512
            ));
    }
    Ok(())
}

#[test]
fn test_extract_parts_to_stdout() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-x")
        .arg("-P")
        .arg("2-")
        .arg("--to-stdout")
        .arg("tests/zstd.cpio");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout("This is a fake busybox binary to simulate a POSIX shell\n");
    Ok(())
}

#[test]
fn test_extract_make_directories_with_pattern() -> Result<(), Box<dyn Error>> {
    let tempdir = TempDir::new()?;
    let mut cmd = get_command();
    cmd.arg("-x")
        .arg("-C")
        .arg(&tempdir.path)
        .arg("--make-directories")
        .arg("-v")
        .arg("tests/zstd.cpio")
        .arg("path/file");

    cmd.output()?
        .assert_stderr("path/file\n")
        .assert_success()
        .assert_stdout("");
    assert!(tempdir.path.join("path/file").exists());
    Ok(())
}

#[test]
fn test_examine_single_cpio_raw() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-e").arg("--raw").arg("tests/single.cpio");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout("0\t512\t512\tcpio\t8\n");
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
        .assert_stderr(".\npath\npath/file\n.\nusr\nusr/bin\nusr/bin/ash\nusr/bin/sh\n")
        .assert_success()
        .assert_stdout("");
    assert!(tempdir.path.join("subdir1/path/file").exists());
    assert!(tempdir.path.join("subdir2/usr/bin/ash").exists());
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
        if program_not_available(compression) {
            continue;
        }
        let mut cmd = get_command();
        cmd.arg("-t").arg(format!("tests/{compression}.cpio"));

        cmd.output()?
            .assert_stderr("")
            .assert_success()
            .assert_stdout(".\npath\npath/file\n.\nusr\nusr/bin\nusr/bin/ash\nusr/bin/sh\n");
    }
    Ok(())
}

#[test]
fn test_list_content_compressed_cpio_path_unset() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--list").arg("tests/gzip.cpio").env_remove("PATH");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout(".\npath\npath/file\n.\nusr\nusr/bin\nusr/bin/ash\nusr/bin/sh\n");
    Ok(())
}

#[test]
fn test_list_content_compressed_cpio_verbose() -> Result<(), Box<dyn Error>> {
    for compression in ["bzip2", "gzip", "lz4", "lzma", "lzop", "xz", "zstd"] {
        if program_not_available(compression) {
            continue;
        }
        let mut cmd = get_command();
        cmd.arg("-tv").arg(format!("tests/{compression}.cpio"));
        cmd.env("TZ", "UTC");

        cmd.output()?
            .assert_stderr("")
            .assert_success()
            .assert_stdout(
                "drwxrwxr-x   2 root     root            0 Apr 14  2024 .\n\
                 drwxrwxr-x   2 root     root            0 Apr 14  2024 path\n\
                 -rw-rw-r--   1 root     root            8 Apr 14  2024 path/file\n\
                 drwxrwxr-x   2 root     root            0 Apr 14  2024 .\n\
                 drwxrwxr-x   2 root     root            0 Apr 14  2024 usr\n\
                 drwxrwxr-x   2 root     root            0 Apr 14  2024 usr/bin\n\
                 lrwxrwxrwx   1 root     root            2 Apr 14  2024 usr/bin/ash -> sh\n\
                 -rw-rw-r--   1 root     root           56 Apr 14  2024 usr/bin/sh\n",
            );
    }
    Ok(())
}

#[test]
fn test_list_content_invalid_archive() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("--list").arg("tests/generate");

    cmd.output()?
        .assert_failure(1)
        .assert_stderr_contains(
            "Error: Failed to list content of 'tests/generate': \
             Failed to determine CPIO or compression magic number",
        )
        .assert_stdout("");
    Ok(())
}

#[test]
fn test_list_content_parts_compressed_cpio() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-t").arg("--parts=1").arg("tests/xz.cpio");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout(".\npath\npath/file\n");
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
fn test_list_content_single_cpio_verbose() -> Result<(), Box<dyn Error>> {
    let mut cmd = get_command();
    cmd.arg("-tv").arg("tests/single.cpio");
    cmd.env("TZ", "UTC");

    cmd.output()?
        .assert_stderr("")
        .assert_success()
        .assert_stdout(
            "drwxrwxr-x   2 root     root            0 Apr 14  2024 .\n\
             drwxrwxr-x   2 root     root            0 Apr 14  2024 path\n\
             -rw-rw-r--   1 root     root            8 Apr 14  2024 path/file\n",
        );
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
fn test_stdout_write_pipe_fail() -> Result<(), Box<dyn Error>> {
    for argument in ["--help", "--version"] {
        let (reader, writer) = UnixStream::pair()?;
        // Close the reader before spawning (similar to "3cpio --help | true")
        drop(reader);
        let stdout = unsafe { Stdio::from_raw_fd(writer.into_raw_fd()) };

        let mut cmd = get_command();
        cmd.arg(argument)
            .env("LANG", "C.UTF-8")
            .stdout(stdout)
            .stderr(Stdio::piped());
        let process = cmd.spawn()?;

        process
            .wait_with_output()?
            .assert_stderr("")
            .assert_failure(141);
    }
    Ok(())
}

#[test]
fn test_stdout_write_pipe_full() -> Result<(), Box<dyn Error>> {
    for argument in ["--help", "--version"] {
        let dev_full = OpenOptions::new().write(true).open("/dev/full")?;

        let mut cmd = get_command();
        cmd.arg(argument)
            .env("LANG", "C.UTF-8")
            .stdout(Stdio::from(dev_full))
            .stderr(Stdio::piped());
        let process = cmd.spawn()?;

        process
            .wait_with_output()?
            .assert_stderr("3cpio: stdout write error: No space left on device (os error 28)\n")
            .assert_failure(1);
    }
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
