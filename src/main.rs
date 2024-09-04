// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::env::set_current_dir;
use std::fs::{create_dir, read_dir, File};
use std::io::ErrorKind;
use std::path::Path;
use std::process::ExitCode;

use lexopt::prelude::*;

use threecpio::{
    create_cpio_archive, examine_cpio_content, extract_cpio_archive, list_cpio_content,
    LOG_LEVEL_DEBUG, LOG_LEVEL_INFO, LOG_LEVEL_WARNING,
};

#[derive(Debug)]
struct Args {
    create: bool,
    directory: String,
    examine: bool,
    extract: bool,
    force: bool,
    list: bool,
    log_level: u32,
    file: String,
    preserve_permissions: bool,
    subdir: Option<String>,
}

fn print_help() {
    let executable = std::env::args().next().unwrap();
    println!(
        "Usage:
    {executable} {{-c|--create}} [FILE]
    {executable} {{-e|--examine}} FILE
    {executable} {{-t|--list}} FILE
    {executable} {{-x|--extract}} [-v|--debug] [-C DIR] [-p] [-s NAME] [--force] FILE

Optional arguments:
  -c, --create   Create a new cpio archive.
  -e, --examine  List the offsets of the cpio archives and their compression.
  -t, --list     List the contents of the cpio archives.
  -x, --extract  Extract cpio archives.
  -C, --directory=DIR  Change directory before performing any operation.
  -p, --preserve-permissions
                 Set permissions of extracted files to those recorded in the
                 archive (default for superuser).
  -s, --subdir   Extract the cpio archives into separate directories (using the
                 given name plus an incrementing number)
  -v, --verbose  Verbose output
  --debug        Debug output
  --force        Force overwriting existing files
  -h, --help     print help message
  -V, --version  print version number and exit",
    );
}

fn print_version() {
    let name = std::option_env!("CARGO_BIN_NAME").unwrap();
    let version = std::option_env!("CARGO_PKG_VERSION").unwrap();
    println!("{} {}", name, version);
}

fn parse_args() -> Result<Args, lexopt::Error> {
    let mut create = 0;
    let mut examine = 0;
    let mut extract = 0;
    let mut force = false;
    let mut preserve_permissions = is_root();
    let mut list = 0;
    let mut log_level = LOG_LEVEL_WARNING;
    let mut directory = ".".into();
    let mut file = None;
    let mut subdir: Option<String> = None;
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Short('c') | Long("create") => {
                create = 1;
            }
            Short('C') | Long("directory") => {
                directory = parser.value()?.string()?;
            }
            Long("debug") => {
                log_level = LOG_LEVEL_DEBUG;
            }
            Short('e') | Long("examine") => {
                examine = 1;
            }
            Long("force") => {
                force = true;
            }
            Short('h') | Long("help") => {
                print_help();
                std::process::exit(0);
            }
            Short('p') | Long("preserve-permissions") => {
                preserve_permissions = true;
            }
            Short('s') | Long("subdir") => {
                subdir = Some(parser.value()?.string()?);
            }
            Short('t') | Long("list") => {
                list = 1;
            }
            Short('v') | Long("verbose") => {
                if log_level <= LOG_LEVEL_INFO {
                    log_level = LOG_LEVEL_INFO;
                }
            }
            Short('V') | Long("version") => {
                print_version();
                std::process::exit(0);
            }
            Short('x') | Long("extract") => {
                extract = 1;
            }
            Value(val) if file.is_none() => {
                file = Some(val.string()?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    if create + examine + extract + list != 1 {
        return Err("Either --create, --examine, --extract or --list must be specified!".into());
    }

    if let Some(ref s) = subdir {
        if s.contains('/') {
            return Err(format!("Subdir '{}' must not contain slashes!", s).into());
        }
    }

    Ok(Args {
        create: create == 1,
        directory,
        examine: examine == 1,
        extract: extract == 1,
        force,
        list: list == 1,
        log_level,
        file: file.ok_or("missing argument FILE")?,
        preserve_permissions,
        subdir,
    })
}

fn is_empty_directory<P: AsRef<Path>>(path: P) -> std::io::Result<bool> {
    Ok(read_dir(path)?.next().is_none())
}

fn is_root() -> bool {
    let uid = unsafe { libc::getuid() };
    uid == 0
}

fn create_and_set_current_dir(path: &str, force: bool) -> Result<(), String> {
    if let Err(e) = set_current_dir(path) {
        if e.kind() != ErrorKind::NotFound {
            return Err(format!("Failed to change directory to '{}': {}", path, e));
        }
        if let Err(e) = create_dir(path) {
            return Err(format!("Failed to create directory '{}': {}", path, e));
        }
        if let Err(e) = set_current_dir(path) {
            return Err(format!("Failed to change directory to '{}': {}", path, e));
        }
    }
    if !force {
        match is_empty_directory(".") {
            Err(e) => {
                return Err(format!(
                    "Failed to check content of directory '{}': {}",
                    path, e
                ));
            }
            Ok(false) => {
                return Err(format!(
                    "Target directory '{}' is not empty. Use --force to overwrite existing files!",
                    path
                ));
            }
            Ok(true) => {}
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let executable = std::env::args().next().unwrap();
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}: Error: {}", executable, e);
            return ExitCode::from(2);
        }
    };

    let file = if args.create {
        match File::create(&args.file) {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "{}: Error: Failed to create '{}': {}",
                    executable, args.file, e
                );
                return ExitCode::FAILURE;
            }
        }
    } else {
        match File::open(&args.file) {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "{}: Error: Failed to open '{}': {}",
                    executable, args.file, e
                );
                return ExitCode::FAILURE;
            }
        }
    };

    if args.create {
        if let Err(e) = set_current_dir(&args.directory) {
            eprintln!(
                "{}: Error: Failed to change directory to '{}': {}",
                executable, args.directory, e
            );
            return ExitCode::FAILURE;
        }
    } else if args.extract {
        if let Err(e) = create_and_set_current_dir(&args.directory, args.force) {
            eprintln!("{}: Error: {}", executable, e);
            return ExitCode::FAILURE;
        }
    }

    let mut stdout = std::io::stdout();
    let (operation, result) = if args.create {
        ("create", create_cpio_archive(file, args.log_level))
    } else if args.examine {
        ("examine", examine_cpio_content(file, &mut stdout))
    } else if args.extract {
        (
            "extract",
            extract_cpio_archive(file, args.preserve_permissions, args.subdir, args.log_level),
        )
    } else if args.list {
        ("list", list_cpio_content(file, &mut stdout, args.log_level))
    } else {
        unreachable!("no operation specified");
    };

    if let Err(e) = result {
        match e.kind() {
            ErrorKind::BrokenPipe => {}
            _ => {
                eprintln!(
                    "{}: Error: Failed to {} content of '{}': {}",
                    executable, operation, args.file, e
                );
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}
