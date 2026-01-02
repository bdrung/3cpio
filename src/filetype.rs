// Copyright (C) 2025-2026, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

pub(crate) const MODE_PERMISSION_MASK: u32 = 0o007_777;
pub(crate) const MODE_FILETYPE_MASK: u32 = 0o770_000;
pub(crate) const FILETYPE_FIFO: u32 = 0o010_000;
pub(crate) const FILETYPE_CHARACTER_DEVICE: u32 = 0o020_000;
pub(crate) const FILETYPE_DIRECTORY: u32 = 0o040_000;
pub(crate) const FILETYPE_BLOCK_DEVICE: u32 = 0o060_000;
pub(crate) const FILETYPE_REGULAR_FILE: u32 = 0o100_000;
pub(crate) const FILETYPE_SYMLINK: u32 = 0o120_000;
pub(crate) const FILETYPE_SOCKET: u32 = 0o140_000;
