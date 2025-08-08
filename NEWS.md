This file summarizes the major and interesting changes for each release. For a
detailed list of changes, please see the git history.

0.8.1 (2025-07-31)
------------------

### Fixed

* test:
  - use temporary directory for write tests
  - canonicalize `/dev/console`
    ([bug #16](https://github.com/bdrung/3cpio/issues/16))

0.8.0 (2025-07-11)
------------------

### What's new

* Use a write buffer for `--create` for a massive performance improvement

### Fixed

* Check exit status of compressor commands
* test:
  - Use `gzip` instead of `true` which might be a symlink
  - Skip `test_file_from_line_location_*` if required file is missing

0.7.0 (2025-07-10)
------------------

### What's new

* Add support for creating cpio archives from a manifest file
  ([feature #3](https://github.com/bdrung/3cpio/issues/3))
* Print inode on `--list --debug`

0.6.0 (2025-06-30)
------------------

### What's new

* doc: add 3cpio man page

### Fixed

* Fix "No such file or directory" error when using `--subdir`
* test: fix race condition in tests by using a lock

0.5.1 (2025-04-11)
------------------

### Fixed

* Fix directory traversal vulnerability: Prevent extracting CPIOs outside of the
  destination directory to prevent directory traversal attacks. This new
  behaviour is similar to `cpio --no-absolute-filenames`.

0.5.0 (2025-03-30)
------------------

### What's new

* add `--count` parameter

0.4.0 (2025-03-11)
------------------

### What's new

* add support for extracting character devices

### Fixed

* print major/minor of character devices in long format

0.3.2 (2024-08-19)
------------------

### What's new

* Support lzma compression ([bug #8](https://github.com/bdrung/3cpio/issues/8))

### Fixed

* Avoid `timespec` struct literal
  ([LP: #2076903](https://launchpad.net/bugs/2076903))
* Include missing helper program name in error message
  ([bug #4](https://github.com/bdrung/3cpio/issues/4))

0.3.1 (2024-08-06)
------------------

### What's new

* Various changes to speed up `3cpio --list --verbose` to make 3cpio faster
  than bsdcpio in all benchmarks.

0.3.0 (2024-08-03)
------------------

### What's new

* support preserving the owner/group of symlinks
* Add `--verbose` mode to `--list` mode. The output will be similar to
  `cpio --list --verbose` and `ls -l`.

### Fixed

* 3cpio: fix setting the directory/file permissions
  ([bug #5](https://github.com/bdrung/3cpio/issues/5))

0.2.0 (2024-07-05)
------------------

### What's new

* Add support for extracting (`--extract`) cpio archives. New parameters are
  `--directory`, `--preserve-permissions`, and `--subdir`.
* Add `--verbose` and `--debug` log levels

### Changed

* Replace command line argument parser `gumdrop` by `lexopt`, because the
  latter has no dependencies.
* Drop `assert_cmd` and `predicates` dev dependencies.

### Fixed

* 3cpio: fix binary name in `--version` output

0.1.0 (2024-04-18)
------------------

Initial release. 3cpio only supports examining (`--examine`) and listing
(`--list`) the content of the initramfs cpio.
