// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

pub const MODE_PERMISSION_MASK: u32 = 0o007_777;
pub const MODE_FILETYPE_MASK: u32 = 0o770_000;
pub const FILETYPE_FIFO: u32 = 0o010_000;
pub const FILETYPE_CHARACTER_DEVICE: u32 = 0o020_000;
pub const FILETYPE_DIRECTORY: u32 = 0o040_000;
pub const FILETYPE_BLOCK_DEVICE: u32 = 0o060_000;
pub const FILETYPE_REGULAR_FILE: u32 = 0o100_000;
pub const FILETYPE_SYMLINK: u32 = 0o120_000;
pub const FILETYPE_SOCKET: u32 = 0o140_000;
