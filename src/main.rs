// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::fs::File;
use std::io::ErrorKind;
use std::process::ExitCode;

use gumdrop::Options;

use threecpio::examine_cpio_content;
use threecpio::list_cpio_content;

#[derive(Debug, Options)]
struct MyOptions {
    #[options(help = "List the offsets of the cpio archives and their compression.")]
    examine: bool,

    #[options(short = 't', help = "List the contents of the cpio archives.")]
    list: bool,

    #[options(help = "print help message")]
    help: bool,

    #[options(short = 'V', help = "print version number and exit")]
    version: bool,

    #[options(free)]
    file: String,
}

fn main() -> ExitCode {
    let opts = MyOptions::parse_args_default_or_exit();
    let executable = std::env::args().next().unwrap();
    if opts.version {
        let name = std::option_env!("CARGO_PKG_NAME").unwrap();
        let version = std::option_env!("CARGO_PKG_VERSION").unwrap();
        println!("{} {}", name, version);
        return ExitCode::SUCCESS;
    }
    if opts.file.is_empty() {
        eprintln!("{}: missing required cpio file argument", executable);
        return ExitCode::from(2);
    }

    let file = match File::open(&opts.file) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "{}: Error: Failed to open '{}': {}",
                executable, opts.file, e
            );
            return ExitCode::FAILURE;
        }
    };

    let mut stdout = std::io::stdout();
    let (operation, result) = if opts.examine {
        ("examine", examine_cpio_content(file, &mut stdout))
    } else if opts.list {
        ("list", list_cpio_content(file, &mut stdout))
    } else {
        eprintln!(
            "{}: Error: Either --examine or --list must be specified!",
            executable
        );
        return ExitCode::FAILURE;
    };

    if let Err(e) = result {
        match e.kind() {
            ErrorKind::BrokenPipe => {}
            _ => {
                eprintln!(
                    "{}: Error: Failed to {} content of '{}': {}",
                    executable, operation, opts.file, e
                );
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}
