// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use core::panic;
use std::fs::File;
use std::io::ErrorKind;
use std::process::ExitCode;

use lexopt::prelude::*;

use threecpio::examine_cpio_content;
use threecpio::list_cpio_content;

#[derive(Debug)]
struct Args {
    examine: bool,
    list: bool,
    file: String,
}

fn print_help() {
    let executable = std::env::args().next().unwrap();
    println!(
        "Usage: {} [-e|--examine|-t|--list] FILE

Optional arguments:
  -e, --examine  List the offsets of the cpio archives and their compression.
  -t, --list     List the contents of the cpio archives.
  -h, --help     print help message
  -V, --version  print version number and exit",
        executable
    );
}

fn print_version() {
    let name = std::option_env!("CARGO_PKG_NAME").unwrap();
    let version = std::option_env!("CARGO_PKG_VERSION").unwrap();
    println!("{} {}", name, version);
}

fn parse_args() -> Result<Args, lexopt::Error> {
    let mut examine = 0;
    let mut list = 0;
    let mut file = None;
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Short('e') | Long("examine") => {
                examine = 1;
            }
            Short('h') | Long("help") => {
                print_help();
                std::process::exit(0);
            }
            Short('t') | Long("list") => {
                list = 1;
            }
            Short('V') | Long("version") => {
                print_version();
                std::process::exit(0);
            }
            Value(val) if file.is_none() => {
                file = Some(val.string()?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    if examine + list != 1 {
        return Err("Either --examine or --list must be specified!".into());
    }

    Ok(Args {
        examine: examine == 1,
        list: list == 1,
        file: file.ok_or("missing argument FILE")?,
    })
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

    let file = match File::open(&args.file) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "{}: Error: Failed to open '{}': {}",
                executable, args.file, e
            );
            return ExitCode::FAILURE;
        }
    };

    let mut stdout = std::io::stdout();
    let (operation, result) = if args.examine {
        ("examine", examine_cpio_content(file, &mut stdout))
    } else if args.list {
        ("list", list_cpio_content(file, &mut stdout))
    } else {
        panic!("no operation specified");
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
