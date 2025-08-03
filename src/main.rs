// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::env::set_current_dir;
use std::fs::{create_dir, read_dir, File};
use std::io::ErrorKind;
use std::path::Path;
use std::process::ExitCode;

use glob::Pattern;
use lexopt::prelude::*;

use threecpio::{
    create_cpio_archive, examine_cpio_content, extract_cpio_archive, list_cpio_content,
    print_cpio_archive_count, LOG_LEVEL_DEBUG, LOG_LEVEL_INFO, LOG_LEVEL_WARNING,
};

#[derive(Debug)]
struct Args {
    count: bool,
    create: bool,
    directory: String,
    examine: bool,
    extract: bool,
    force: bool,
    list: bool,
    log_level: u32,
    archive: Option<String>,
    patterns: Vec<Pattern>,
    preserve_permissions: bool,
    subdir: Option<String>,
}

fn print_help() {
    let executable = std::env::args().next().unwrap();
    println!(
        "Usage:
    {executable} --count ARCHIVE
    {executable} {{-c|--create}} [-v|--debug] [-C DIR] [ARCHIVE] < manifest
    {executable} {{-e|--examine}} ARCHIVE
    {executable} {{-t|--list}} [-v|--debug] ARCHIVE
    {executable} {{-x|--extract}} [-v|--debug] [-C DIR] [-p] [-s NAME] [--force] ARCHIVE

Optional arguments:
  --count        Print the number of concatenated cpio archives.
  -c, --create   Create a new cpio archive from the manifest on stdin.
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
    println!("{name} {version}");
}

fn parse_args() -> Result<Args, lexopt::Error> {
    let mut count = 0;
    let mut create = 0;
    let mut examine = 0;
    let mut extract = 0;
    let mut force = false;
    let mut preserve_permissions = is_root();
    let mut list = 0;
    let mut log_level = LOG_LEVEL_WARNING;
    let mut directory = ".".into();
    let mut archive = None;
    let mut patterns = Vec::new();
    let mut subdir: Option<String> = None;
    let mut arguments = Vec::new();
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Long("count") => {
                count = 1;
            }
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
            Value(val) if archive.is_none() => {
                archive = Some(val.string()?);
            }
            Value(val) => arguments.push(val.string()?),
            _ => return Err(arg.unexpected()),
        }
    }

    if count + create + examine + extract + list != 1 {
        return Err(
            "Either --count, --create, --examine, --extract, or --list must be specified!".into(),
        );
    }

    if extract + list == 1 {
        for argument in arguments {
            let pattern = Pattern::new(&argument)
                .map_err(|e| format!("invalid pattern '{argument}': {e}"))?;
            patterns.push(pattern);
        }
    } else if !arguments.is_empty() {
        let first = &arguments[0];
        return Err(Value(first.into()).unexpected());
    }

    if let Some(ref s) = subdir {
        if s.contains('/') {
            return Err(format!("Subdir '{s}' must not contain slashes!").into());
        }
    }

    if create != 1 && archive.is_none() {
        return Err("missing argument ARCHIVE".into());
    }

    Ok(Args {
        count: count == 1,
        create: create == 1,
        directory,
        examine: examine == 1,
        extract: extract == 1,
        force,
        list: list == 1,
        log_level,
        archive,
        patterns,
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
            return Err(format!("Failed to change directory to '{path}': {e}"));
        }
        if let Err(e) = create_dir(path) {
            return Err(format!("Failed to create directory '{path}': {e}"));
        }
        if let Err(e) = set_current_dir(path) {
            return Err(format!("Failed to change directory to '{path}': {e}"));
        }
    }
    if !force {
        match is_empty_directory(".") {
            Err(e) => {
                return Err(format!(
                    "Failed to check content of directory '{path}': {e}"
                ));
            }
            Ok(false) => {
                return Err(format!(
                    "Target directory '{path}' is not empty. \
                    Use --force to overwrite existing files!",
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
            eprintln!("{executable}: Error: {e}");
            return ExitCode::from(2);
        }
    };

    if args.create {
        let mut archive = None;
        if let Some(path) = args.archive.as_ref() {
            archive = match File::create(path) {
                Ok(f) => Some(f),
                Err(e) => {
                    eprintln!("{executable}: Error: Failed to create '{path}': {e}");
                    return ExitCode::FAILURE;
                }
            };
            if args.log_level >= LOG_LEVEL_DEBUG {
                eprintln!("{executable}: Opened '{path}' for writing.");
            }
        }
        if let Err(e) = set_current_dir(&args.directory) {
            eprintln!(
                "{executable}: Error: Failed to change directory to '{}': {e}",
                args.directory,
            );
            return ExitCode::FAILURE;
        }
        let result = create_cpio_archive(archive, args.log_level);
        if let Err(error) = result {
            match error.kind() {
                ErrorKind::BrokenPipe => {}
                _ => {
                    eprintln!(
                        "{executable}: Error: Failed to create '{}': {error}",
                        args.archive.unwrap_or("cpio on stdout".into()),
                    );
                    return ExitCode::FAILURE;
                }
            }
        }
        return ExitCode::SUCCESS;
    };

    let archive = match File::open(args.archive.as_ref().unwrap()) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "{executable}: Error: Failed to open '{}': {e}",
                args.archive.unwrap(),
            );
            return ExitCode::FAILURE;
        }
    };

    if args.extract {
        if let Err(e) = create_and_set_current_dir(&args.directory, args.force) {
            eprintln!("{executable}: Error: {e}");
            return ExitCode::FAILURE;
        }
    }

    let mut stdout = std::io::stdout();
    let (operation, result) = if args.count {
        (
            "count number of cpio archives",
            print_cpio_archive_count(archive, &mut stdout),
        )
    } else if args.examine {
        (
            "examine content",
            examine_cpio_content(archive, &mut stdout),
        )
    } else if args.extract {
        (
            "extract content",
            extract_cpio_archive(
                archive,
                args.patterns,
                args.preserve_permissions,
                args.subdir,
                args.log_level,
            ),
        )
    } else if args.list {
        (
            "list content",
            list_cpio_content(archive, &mut stdout, &args.patterns, args.log_level),
        )
    } else {
        unreachable!("no operation specified");
    };

    if let Err(e) = result {
        match e.kind() {
            ErrorKind::BrokenPipe => {}
            _ => {
                eprintln!(
                    "{executable}: Error: Failed to {operation} of '{}': {e}",
                    args.archive.unwrap(),
                );
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}
