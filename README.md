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

As of now, 3cpio supports examining and listing the content of the initramfs
cpio.

**Note**: The Rust crate is named threecpio, because package names are not
allowed to start with numbers.

Usage examples
--------------

Examine the content of the initramfs cpio on an Ubuntu 24.04 system:

```
$ 3cpio --examine /boot/initrd.img
0	cpio
77312	cpio
7286272	cpio
85523968	zstd
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

Quick examples comparing
------------------------

Runtime comparison measured with `time` over five runs on different initramfs
cpios:

| System               | File                        | Size   | Entries | 3cpio  | lsinitramfs |
| -------------------- | --------------------------- | ------ | ------- | ------ | ----------- |
| AMD Ryzen 7 5700G    | initrd.img-6.5.0-27-generic | 102 MB |    3496 | 0.052s |     14.243s |
| AMD Ryzen 7 5700G    | initrd.img-6.8.0-22-generic |  63 MB |    1934 | 0.042s |      7.239s |
| Raspberry Pi Zero 2W | initrd.img-6.5.0-1012-raspi |  24 MB |    1537 | 0.647s |     57.235s |

3cpio is 88 to 274 times faster than lsinitramfs.

Commands used:

```
3cpio -t /boot/$file | wc -l
time 3cpio -t /boot/$file > /dev/null
time lsinitramfs /boot/$file > /dev/null
```

Naming and alternatives
-----------------------

The tool is named 3cpio because it is the third cpio tool besides
[GNU cpio](https://www.gnu.org/software/cpio/) and `bsdcpio` provided by
[libarchive](https://www.libarchive.org/). 3cpio is also the third tool that can
list the content of initramfs cpio archives besides `lsinitramfs` from
[initramfs-tools](https://tracker.debian.org/pkg/initramfs-tools) and `lsinitrd`
from [dracut](https://github.com/dracut-ng/dracut-ng).
