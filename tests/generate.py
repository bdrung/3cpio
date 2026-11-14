#!/usr/bin/python3

# Copyright (C) 2026, Benjamin Drung <bdrung@posteo.de>
# SPDX-License-Identifier: ISC

"""Generate more test cpio files."""

import dataclasses
import gzip
import pathlib


@dataclasses.dataclass
# pylint: disable-next=too-many-instance-attributes
class Header:
    """CPIO header."""

    ino: int
    mode: int
    uid: int
    gid: int
    nlink: int
    mtime: int
    filesize: int
    major: int
    minor: int
    rmajor: int
    rminor: int
    filename: str

    @classmethod
    def regular_file(cls, ino: int, filename: str, filesize: int) -> "Header":
        """Create a Header for a regular file with default ownership and timestamps."""
        return cls(
            ino=ino,
            mode=0o100644,
            uid=0,
            gid=0,
            nlink=1,
            mtime=0,
            filesize=filesize,
            major=0,
            minor=0,
            rmajor=0,
            rminor=0,
            filename=filename,
        )

    def to_bytes(self) -> bytes:
        """Serialize the CPIO header.

        This includes NUL-terminated filename and 4-byte padding.
        """
        encoded_name = self.filename.encode() + b"\0"
        header = (
            f"070701{self.ino:08X}{self.mode:08X}{self.uid:08X}{self.gid:08X}"
            f"{self.nlink:08X}{self.mtime:08X}{self.filesize:08X}"
            f"{self.major:08X}{self.minor:08X}{self.rmajor:08X}{self.rminor:08X}"
            f"{len(encoded_name):08X}{0:08X}".encode() + encoded_name
        )
        header += b"\0" * (-len(header) % 4)
        return header


def generate_compressed_truncated_cpio(path: pathlib.Path) -> None:
    """Generate a compressed truncated cpio file.

    The generated file is a valid gzip file, but the CPIO content is truncated
    in the file data.

    See https://github.com/bdrung/3cpio/pull/38
    """
    header = Header.regular_file(1, "first", 4096)
    cpio = header.to_bytes() + b"x" * 100
    path.write_bytes(gzip.compress(cpio, mtime=0))


def main() -> None:
    """Generate more test cpio files."""
    script_dir = pathlib.Path(__file__).resolve().parent
    generate_compressed_truncated_cpio(script_dir / "truncated-gzip.cpio")


if __name__ == "__main__":
    main()
