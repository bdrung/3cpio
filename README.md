3cpio
=====

3cpio is a tool to manage initramfs cpio files for the Linux kernel. The Linux
kernel's
[initramfs buffer format](https://www.kernel.org/doc/html/latest/driver-api/early-userspace/buffer-format.html)
is based around the `newc` or `crc` cpio formats. Multiple cpio archives can be
concatenated and the last archive can be compressed. Different compression
algorithms can be used depending on what support was compiled into the Linux
kernel. 3cpio is tailored to initramfs cpio files and will not gain support for
other cpio formats.

3cpio supports creating, examining, listing, and extracting the content of the
initramfs cpio.

**Note**: The Rust crate is named threecpio, because package names are not
allowed to start with numbers.

Installation
------------

<a href="https://repology.org/project/3cpio/versions">
  <img
    src="https://repology.org/badge/vertical-allrepos/3cpio.svg"
    alt="Packaging status"
    align="right"
    style="margin-left: 1em;"
  />
</a>

### Distribution Package

The easiest way to install 3cpio is to install a package offered by your
operating system. See the *Packaging status* image next to this text for
a list of distributions with a 3cpio package:

### Manual Installation

Install cargo on your operating system and then run:

```
cargo install threecpio
```

Usage examples
--------------

List the number of cpio archives that an initramfs file contains:

```
$ 3cpio --count /boot/initrd.img
4
```

Examine the content of the initramfs cpio on an Ubuntu 24.04 system:

```
$ 3cpio --examine /boot/initrd.img
Start     End       Size      Compr.   Extracted
0 B       148 kB    148 kB    cpio     147 kB
148 kB    13.3 MB   13.1 MB   cpio     13.1 MB
13.3 MB   55.2 MB   41.9 MB   cpio     41.7 MB
55.2 MB   62.0 MB   6.74 MB   zstd     15.6 MB
```

There is also a machine-readable output format available:

```
$ 3cpio --examine --raw /boot/initrd.img
0	148480	148480	cpio	147350
148480	13275136	13126656	cpio	13125632
13275136	55215104	41939968	cpio	41692226
55215104	61956920	6741816	zstd	15616306
```

This initramfs cpio consists of three uncompressed cpio archives followed by a
Zstandard-compressed cpio archive.

List the content of the initramfs cpio on an Ubuntu 24.04 system:

```
$ 3cpio --list /boot/initrd.img
.
kernel
kernel/x86
kernel/x86/microcode
kernel/x86/microcode/AuthenticAMD.bin
kernel
kernel/x86
kernel/x86/microcode
kernel/x86/microcode/.enuineIntel.align.0123456789abc
kernel/x86/microcode/GenuineIntel.bin
.
usr
usr/lib
usr/lib/firmware
usr/lib/firmware/3com
usr/lib/firmware/3com/typhoon.bin.zst
[...]
```

The first cpio contains only the AMD microcode. The second cpio contains only
the Intel microcode. The third cpio contains firmware files and kernel modules.

Extract the content of the initramfs cpio to the `initrd` subdirectory on an
Ubuntu 24.04 system:

```
$ 3cpio --extract -C initrd /boot/initrd.img
$ ls initrd
bin   cryptroot  init    lib    lib.usr-is-merged  run   scripts  var
conf  etc        kernel  lib64  libx32             sbin  usr
```

Create a cpio archive similar to the other cpio tools using the `find` command:

```
$ cd inputdir && find . | sort | 3cpio --create ../example.cpio
```

Due to its manifest file format support, 3cpio can create cpio archives without
the need of copying files into a temporary directory first. Example for creating
an early microcode cpio image directly using the system installed files:

```
$ cat manifest
-	kernel	dir	755	0	0	1751654557
-	kernel/x86	dir	755	0	0	1752011622
/usr/lib/firmware/amd-ucode	kernel/x86/microcode
/usr/lib/firmware/amd-ucode/microcode_amd_fam19h.bin	kernel/x86/microcode/AuthenticAMD.bin
$ 3cpio --create amd-ucode.img < manifest
$ 3cpio --list --verbose amd-ucode.img
drwxr-xr-x   2 root     root            0 Jul  4 20:42 kernel
drwxr-xr-x   2 root     root            0 Jul  8 23:53 kernel/x86
drwxr-xr-x   2 root     root            0 Jun 10 10:51 kernel/x86/microcode
-rw-r--r--   1 root     root       100684 Mar 23 22:42 kernel/x86/microcode/AuthenticAMD.bin
```

Example for creating an initrd image containing of an uncompressed early
microcode cpio followed by a Zstandard-compressed cpio:

```
$ cat manifest
#cpio
-	kernel	dir	755	0	0	1751654557
-	kernel/x86	dir	755	0	0	1752011622
/usr/lib/firmware/amd-ucode	kernel/x86/microcode
/usr/lib/firmware/amd-ucode/microcode_amd_fam19h.bin	kernel/x86/microcode/AuthenticAMD.bin
#cpio: zstd -9
/
/bin
/usr
/usr/bin
/usr/bin/bash
# This is a comment. Leaving the remaining files as task for the reader.
$ 3cpio --create initrd.img < manifest
$ 3cpio --examine initrd.img
Start     End       Size      Compr.   Extracted
0 B       101 kB    101 kB    cpio     101 kB
101 kB    786 kB    685 kB    zstd     1.45 MB
$ 3cpio --list --verbose initrd.img
drwxr-xr-x   2 root     root            0 Jul  4 20:42 kernel
drwxr-xr-x   2 root     root            0 Jul  8 23:53 kernel/x86
drwxr-xr-x   2 root     root            0 Jun 10 10:51 kernel/x86/microcode
-rw-r--r--   1 root     root       100684 Mar 23 22:42 kernel/x86/microcode/AuthenticAMD.bin
drwxr-xr-x   2 root     root            0 Jun  5 14:11 .
lrwxrwxrwx   1 root     root            7 Mar 20  2022 bin -> usr/bin
drwxr-xr-x   2 root     root            0 Apr 20  2023 usr
drwxr-xr-x   2 root     root            0 Jul  9 09:56 usr/bin
-rwxr-xr-x   1 root     root      1446024 Mar 31  2024 usr/bin/bash
```

Benchmark results
-----------------

### Listing the content of the initrd

Runtime comparison measured with `time` over five runs on different initramfs
cpios:

| System           | Kernel           | Comp.    | Size   | Files | 3cpio  | lsinitramfs | lsinitrd |
| ---------------- | ---------------- | -------- | ------ | ----- | ------ | ----------- | -------- |
| Ryzen 7 5700G    | 6.5.0-27-generic | zstd¹    | 102 MB |  3496 | 0.052s |     14.243s |       –³ |
| Ryzen 7 5700G VM | 6.8.0-22-generic | zstd¹    |  63 MB |  1934 | 0.042s |      7.239s |       –³ |
| Ryzen 7 5700G VM | 6.8.0-22-generic | zstd²    |  53 MB |  1783 | 0.061s |      0.452s |   0.560s |
| RasPi Zero 2W    | 6.5.0-1012-raspi | zstd¹    |  24 MB |  1538 | 0.647s |     56.253s |       –³ |
| RasPi Zero 2W    | 6.5.0-1012-raspi | zstd²    |  30 MB |  2028 | 1.141s |      2.286s |   6.118s |
| RasPi Zero 2W    | 6.8.0-1002-raspi | zstd¹    |  51 MB |  2532 | 0.713s |    164.575s |       –³ |
| RasPi Zero 2W    | 6.8.0-1002-raspi | zstd -1² |  47 MB |  2778 | 1.156s |      2.842s |   9.508s |
| RasPi Zero 2W    | 6.8.0-1002-raspi | xz²      |  41 MB |  2778 | 6.922s |     13.451s |  35.184s |

**Legend**:
1. generated by initramfs-tools
2. generated by `dracut --force --${compression}`. On Raspberry Pi Zero 2W there
   is not enough memory for the default `zstd -15`. So using the default from
   initramfs-tools there: `dracut --force --compress "zstd -1 -q -T0"`
3. lsinitrd only reads the first two cpio archives of the file, but the
   initramfs consists of four cpios.

**Results**:
* 3cpio is 87 to 274 times faster than lsinitramfs for images generated by
  initramfs-tools.
* 3cpio is two to eight times faster than lsinitramfs for images generated
  by dracut.
* 3cpio five to nine times faster than lsinitrd for images generated by dracut.

Commands used:

```
3cpio -t /boot/initrd.img-${version} | wc -l
time 3cpio -t /boot/initrd.img-${version} > /dev/null
time lsinitramfs /boot/initrd.img-${version} > /dev/null
time lsinitrd /boot/initrd.img-${version} > /dev/null
```

List the content of single cpio archive that is not compressed (see
[doc/Benchmarks.md](doc/Benchmarks.md) for details) on a Raspberry Pi Zero 2W:

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -t initrd.img` | 84.3 ± 1.1 | 82.1 | 87.0 | 1.00 |
| `bsdcpio -itF initrd.img` | 98.4 ± 0.9 | 96.4 | 101.0 | 1.17 ± 0.02 |
| `cpio -t --file initrd.img` | 1321.2 ± 2.8 | 1314.6 | 1327.6 | 15.68 ± 0.20 |

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -tv initrd.img` | 109.2 ± 1.1 | 106.9 | 111.7 | 1.00 |
| `bsdcpio -itvF initrd.img` | 114.9 ± 1.1 | 112.6 | 117.4 | 1.05 ± 0.01 |
| `cpio -tv --file initrd.img` | 1423.0 ± 3.5 | 1417.1 | 1440.6 | 13.03 ± 0.13 |

### Extracting the content of the initrd

Benchmarking the time to extraction initrd:

| System        | Distro   | Kernel           | Size   | Files | 3cpio  | unmkinitramfs |
| ------------- | -------- | ---------------- | ------ | ----- | ------ | ------------- |
| Ryzen 7 5700G | noble    | 6.8.0-35-generic |  70 MB |  2097 | 0.107s |        6.698s |
| Ryzen 7 5700G | jammy    | 6.8.0-35-generic | 112 MB |  3789 | 0.455s |        2.217s |
| Ryzen 7 5700G | bookworm | 6.1.0-21-amd64   |  62 MB |  2935 | 0.268s |        1.362s |
| RasPi Zero 2W | noble    | 6.8.0-1005-raspi |  53 MB |  2534 | 5.075s |      173.847s |

Raw measurements can be found in [doc/Benchmarks.md](doc/Benchmarks.md).

### Creating cpio archives

3cpio is the fastest tool by far in all tested scenarios
(the other tools are 1.13 to 4.48 times slower with a cold cache
and 1.52 to 5.87 times slower with a warm cache):

| System        | Distro | Kernel            | Size   | Cache | 3cpio   | bsdcpio | cpio    |
| ------------- | ------ | ----------------- | ------ | ----- | ------- | ------- | ------- |
| Ryzen 7 5700G | noble* | 6.8.0-63-generic  |  84 MB | warm  |  0.061s |  0.237s |  0.323s |
| Ryzen 7 5700G | noble* | 6.8.0-63-generic  |  84 MB | cold  |  0.068s |  0.257s |  0.337s |
| Ryzen 7 5700G | plucky | 6.14.0-23-generic |  68 MB | warm  |  0.065s |  0.299s |  0.383s |
| Ryzen 7 5700G | plucky | 6.14.0-23-generic |  68 MB | cold  |  0.257s |  0.491s |  0.559s |
| RasPi Zero 2W | noble  | 6.8.0-1030-raspi  |  80 MB | warm  |  2.460s |  3.733s |  4.833s |
| RasPi Zero 2W | noble  | 6.8.0-1030-raspi  |  80 MB | cold  | 10.743s | 12.200s | 12.154s |

The Ryzen 7 5700G noble tests were done in chroots with tmpfs.
Raw measurements can be found in [doc/Benchmarks.md](doc/Benchmarks.md).

Naming and alternatives
-----------------------

The tool is named 3cpio because it is the third cpio tool besides
[GNU cpio](https://www.gnu.org/software/cpio/) and `bsdcpio` provided by
[libarchive](https://www.libarchive.org/). 3cpio is also the third tool that can
list the content of initramfs cpio archives besides `lsinitramfs` from
[initramfs-tools](https://tracker.debian.org/pkg/initramfs-tools) and `lsinitrd`
from [dracut](https://github.com/dracut-ng/dracut-ng).
