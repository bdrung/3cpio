This file summarizes the major and interesting changes for each release. For a
detailed list of changes, please see the git history.

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
