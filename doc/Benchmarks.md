Benchmarks
==========

This page contains the raw measurements.

Raspberry Pi Zero 2W
--------------------

Benchmark results on a Raspberry Pi Zero 2W running Ubuntu 24.04 (noble) arm64
on 2024-06-05:

```
$ ls -l /boot/initrd.img*
lrwxrwxrwx 1 root root       27 Jun  4 06:50 /boot/initrd.img -> initrd.img-6.8.0-1005-raspi
-rw-r--r-- 1 root root 52794656 Jun  4 18:29 /boot/initrd.img-6.8.0-1005-raspi
$ 3cpio -t /boot/initrd.img | wc -l
2534
$ hyperfine -p "rm -rf initrd" "3cpio -x /boot/initrd.img -C initrd" "unmkinitramfs /boot/initrd.img initrd" --export-markdown extract.md
Benchmark 1: 3cpio -x /boot/initrd.img -C initrd
  Time (mean ± σ):      5.075 s ±  0.247 s    [User: 0.116 s, System: 1.932 s]
  Range (min … max):    4.631 s …  5.591 s    10 runs

Benchmark 2: unmkinitramfs /boot/initrd.img initrd
  Time (mean ± σ):     173.847 s ±  8.368 s    [User: 31.155 s, System: 269.939 s]
  Range (min … max):   162.180 s … 183.792 s    10 runs

Summary
  3cpio -x /boot/initrd.img -C initrd ran
   34.25 ± 2.34 times faster than unmkinitramfs /boot/initrd.img initrd
```

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -x /boot/initrd.img -C initrd` | 5.075 ± 0.247 | 4.631 | 5.591 | 1.00 |
| `unmkinitramfs /boot/initrd.img initrd` | 173.847 ± 8.368 | 162.180 | 183.792 | 34.25 ± 2.34 |

```
$ hyperfine --warmup 1 "3cpio -t /boot/initrd.img" "lsinitramfs /boot/initrd.img" -u second --export-markdown list.md
Benchmark 1: 3cpio -t /boot/initrd.img
  Time (mean ± σ):      0.697 s ±  0.003 s    [User: 0.039 s, System: 0.265 s]
  Range (min … max):    0.692 s …  0.703 s    10 runs

Benchmark 2: lsinitramfs /boot/initrd.img
  Time (mean ± σ):     165.425 s ±  7.986 s    [User: 30.696 s, System: 259.767 s]
  Range (min … max):   154.661 s … 176.996 s    10 runs

Summary
  3cpio -t /boot/initrd.img ran
  237.45 ± 11.51 times faster than lsinitramfs /boot/initrd.img
```

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -t /boot/initrd.img` | 0.697 ± 0.003 | 0.692 | 0.703 | 1.00 |
| `lsinitramfs /boot/initrd.img` | 165.425 ± 7.986 | 154.661 | 176.996 | 237.45 ± 11.51 |

Benchmark results on a Raspberry Pi Zero 2W running Ubuntu 24.04 (noble) arm64
on 2024-08-06:

```
$ sudo 3cpio -x /boot/initrd.img -C /var/tmp/initrd
$ ( cd /var/tmp/initrd && find . | LC_ALL=C sort | sudo cpio --reproducible --quiet -o -H newc ) > initrd.img
$ ls -l initrd.img
-rw-rw-r-- 1 user user 75868160 Aug  3 02:10 initrd.img
$ 3cpio -t initrd.img | wc -l
2529
$ 3cpio -e initrd.img
0	cpio
$ hyperfine -N -w 2 -r 100 "3cpio -t initrd.img" "bsdcpio -itF initrd.img" "cpio -t --file initrd.img" --export-markdown list.md
Benchmark 1: 3cpio -t initrd.img
  Time (mean ± σ):      84.3 ms ±   1.1 ms    [User: 25.6 ms, System: 57.5 ms]
  Range (min … max):    82.1 ms …  87.0 ms    100 runs

Benchmark 2: bsdcpio -itF initrd.img
  Time (mean ± σ):      98.4 ms ±   0.9 ms    [User: 29.1 ms, System: 67.6 ms]
  Range (min … max):    96.4 ms … 101.0 ms    100 runs

Benchmark 3: cpio -t --file initrd.img
  Time (mean ± σ):      1.321 s ±  0.003 s    [User: 0.277 s, System: 1.039 s]
  Range (min … max):    1.315 s …  1.328 s    100 runs

Summary
  3cpio -t initrd.img ran
    1.17 ± 0.02 times faster than bsdcpio -itF initrd.img
   15.68 ± 0.20 times faster than cpio -t --file initrd.img
$ hyperfine -N -w 2 -r 100 "3cpio -tv initrd.img" "bsdcpio -itvF initrd.img" "cpio -tv --file initrd.img" --export-markdown list-verbose.md
Benchmark 1: 3cpio -tv initrd.img
  Time (mean ± σ):     109.2 ms ±   1.1 ms    [User: 46.3 ms, System: 61.7 ms]
  Range (min … max):   106.9 ms … 111.7 ms    100 runs

Benchmark 2: bsdcpio -itvF initrd.img
  Time (mean ± σ):     114.9 ms ±   1.1 ms    [User: 44.2 ms, System: 69.0 ms]
  Range (min … max):   112.6 ms … 117.4 ms    100 runs

Benchmark 3: cpio -tv --file initrd.img
  Time (mean ± σ):      1.423 s ±  0.004 s    [User: 0.318 s, System: 1.099 s]
  Range (min … max):    1.417 s …  1.441 s    100 runs

Summary
  3cpio -tv initrd.img ran
    1.05 ± 0.01 times faster than bsdcpio -itvF initrd.img
   13.03 ± 0.13 times faster than cpio -tv --file initrd.img
```

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

AMD Ryzen 7 5700G
-----------------

Benchmark results on a desktop machine with an AMD Ryzen 7 5700G running Ubuntu
24.04 (noble) on 2024-06-09. The tests were done in chroots that use overlayfs
on tmpfs for writes.

```
$ schroot-wrapper -p initramfs-tools,linux-image-generic,zstd,busybox-initramfs,cryptsetup-initramfs,kbd,lvm2,mdadm,ntfs-3g,plymouth,plymouth-theme-spinner,hyperfine -u root -c noble
(noble)root@desktop:~# ls -l /boot/initrd.img*
lrwxrwxrwx 1 root root       27 Jun  4 23:37 /boot/initrd.img -> initrd.img-6.8.0-35-generic
-rw-r--r-- 1 root root 70220742 Jun  4 23:37 /boot/initrd.img-6.8.0-35-generic
(noble)root@desktop:~# 3cpio -t /boot/initrd.img | wc -l
2097
(noble)root@desktop:~# 3cpio -e /boot/initrd.img
0	cpio
77312	cpio
8033792	cpio
51411456	zstd
(noble)root@desktop:~# hyperfine -p "rm -rf initrd" "3cpio -x /boot/initrd.img -C initrd" "unmkinitramfs /boot/initrd.img initrd" --export-markdown extract.md
Benchmark 1: 3cpio -x /boot/initrd.img -C initrd
  Time (mean ± σ):     107.2 ms ±   1.0 ms    [User: 3.8 ms, System: 91.9 ms]
  Range (min … max):   105.8 ms … 110.2 ms    27 runs

Benchmark 2: unmkinitramfs /boot/initrd.img initrd
  Time (mean ± σ):      6.698 s ±  0.026 s    [User: 5.106 s, System: 5.639 s]
  Range (min … max):    6.648 s …  6.724 s    10 runs

Summary
  3cpio -x /boot/initrd.img -C initrd ran
   62.48 ± 0.62 times faster than unmkinitramfs /boot/initrd.img initrd
```

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -x /boot/initrd.img -C initrd` | 107.2 ± 1.0 | 105.8 | 110.2 | 1.00 |
| `unmkinitramfs /boot/initrd.img initrd` | 6697.5 ± 25.6 | 6647.8 | 6723.6 | 62.48 ± 0.62 |

```
(noble)root@desktop:~# hyperfine --warmup 1 "3cpio -t /boot/initrd.img" "lsinitramfs /boot/initrd.img" --export-markdown list.md
Benchmark 1: 3cpio -t /boot/initrd.img
  Time (mean ± σ):      42.9 ms ±   0.5 ms    [User: 1.7 ms, System: 13.9 ms]
  Range (min … max):    42.0 ms …  43.9 ms    68 runs

Benchmark 2: lsinitramfs /boot/initrd.img
  Time (mean ± σ):      6.471 s ±  0.041 s    [User: 5.054 s, System: 5.323 s]
  Range (min … max):    6.408 s …  6.536 s    10 runs

Summary
  3cpio -t /boot/initrd.img ran
  150.68 ± 1.88 times faster than lsinitramfs /boot/initrd.img
```

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -t /boot/initrd.img` | 42.9 ± 0.5 | 42.0 | 43.9 | 1.00 |
| `lsinitramfs /boot/initrd.img` | 6471.0 ± 41.0 | 6408.1 | 6536.3 | 150.68 ± 1.88 |

```
$ schroot-wrapper -p initramfs-tools,linux-image-generic,zstd,busybox-initramfs,cryptsetup-initramfs,kbd,lvm2,mdadm,ntfs-3g,plymouth,plymouth-theme-spinner,hyperfine -u root -c jammy
(jammy)root@desktop:~# ls -l /boot/initrd.img*
lrwxrwxrwx 1 root root        29 Jun  4 23:49 /boot/initrd.img -> initrd.img-5.15.0-107-generic
-rw-r--r-- 1 root root 112100650 Jun  4 23:50 /boot/initrd.img-5.15.0-107-generic
(jammy)root@desktop:~# 3cpio -t /boot/initrd.img | wc -l
3789
(jammy)root@desktop:~# 3cpio -e /boot/initrd.img
0	cpio
77312	cpio
8033792	zstd
(jammy)root@desktop:~# hyperfine -p "rm -rf initrd" "3cpio -x /boot/initrd.img -C initrd" "unmkinitramfs /boot/initrd.img initrd" --export-markdown extract.md
Benchmark 1: 3cpio -x /boot/initrd.img -C initrd
  Time (mean ± σ):     455.1 ms ±   3.6 ms    [User: 10.7 ms, System: 263.4 ms]
  Range (min … max):   451.5 ms … 464.5 ms    10 runs

Benchmark 2: unmkinitramfs /boot/initrd.img initrd
  Time (mean ± σ):      2.217 s ±  0.008 s    [User: 0.878 s, System: 2.264 s]
  Range (min … max):    2.198 s …  2.227 s    10 runs

Summary
  '3cpio -x /boot/initrd.img -C initrd' ran
    4.87 ± 0.04 times faster than 'unmkinitramfs /boot/initrd.img initrd'
```

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -x /boot/initrd.img -C initrd` | 455.1 ± 3.6 | 451.5 | 464.5 | 1.00 |
| `unmkinitramfs /boot/initrd.img initrd` | 2216.5 ± 8.3 | 2198.2 | 2227.3 | 4.87 ± 0.04 |

```
(jammy)root@desktop:~# hyperfine --warmup 1 "3cpio -t /boot/initrd.img" "lsinitramfs /boot/initrd.img" --export-markdown list.md
Benchmark 1: 3cpio -t /boot/initrd.img
  Time (mean ± σ):     336.0 ms ±   6.3 ms    [User: 5.5 ms, System: 77.8 ms]
  Range (min … max):   326.5 ms … 345.0 ms    10 runs

Benchmark 2: lsinitramfs /boot/initrd.img
  Time (mean ± σ):      1.374 s ±  0.010 s    [User: 0.725 s, System: 1.050 s]
  Range (min … max):    1.354 s …  1.393 s    10 runs

Summary
  '3cpio -t /boot/initrd.img' ran
    4.09 ± 0.08 times faster than 'lsinitramfs /boot/initrd.img'
```

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -t /boot/initrd.img` | 336.0 ± 6.3 | 326.5 | 345.0 | 1.00 |
| `lsinitramfs /boot/initrd.img` | 1374.3 ± 10.3 | 1354.0 | 1392.9 | 4.09 ± 0.08 |

```
$ schroot-wrapper -p initramfs-tools,linux-image-generic,firmware-linux,zstd,cryptsetup-initramfs,lvm2,kbd,mdadm,ntfs-3g,plymouth,console-setup,hyperfine -u root -c bookworm
(bookworm)root@desktop:~# ( cd /boot && ln -s initrd.img-* initrd.img )
(bookworm)root@desktop:~# ls -l /boot/initrd.img*
lrwxrwxrwx 1 root root       25 Jun  9 15:55 /boot/initrd.img -> initrd.img-6.1.0-21-amd64
-rw-r--r-- 1 root root 62448197 Jun  9 15:53 /boot/initrd.img-6.1.0-21-amd64
(bookworm)root@desktop:~# 3cpio -t /boot/initrd.img | wc -l
2935
(bookworm)root@desktop:~# 3cpio -e /boot/initrd.img
0	zstd
(bookworm)root@desktop:~# hyperfine -p "rm -rf initrd" "3cpio -x /boot/initrd.img -C initrd" "unmkinitramfs /boot/initrd.img-6.1.0-21-amd64 initrd" --export-markdown extract.md
Benchmark 1: 3cpio -x /boot/initrd.img -C initrd
  Time (mean ± σ):     267.5 ms ±   2.4 ms    [User: 7.6 ms, System: 209.0 ms]
  Range (min … max):   264.8 ms … 273.2 ms    10 runs

Benchmark 2: unmkinitramfs /boot/initrd.img-6.1.0-21-amd64 initrd
  Time (mean ± σ):      1.362 s ±  0.004 s    [User: 0.681 s, System: 1.513 s]
  Range (min … max):    1.355 s …  1.368 s    10 runs

Summary
  '3cpio -x /boot/initrd.img -C initrd' ran
    5.09 ± 0.05 times faster than 'unmkinitramfs /boot/initrd.img-6.1.0-21-amd64 initrd'
```

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -x /boot/initrd.img -C initrd` | 267.5 ± 2.4 | 264.8 | 273.2 | 1.00 |
| `unmkinitramfs /boot/initrd.img-6.1.0-21-amd64 initrd` | 1361.7 ± 4.4 | 1354.6 | 1368.4 | 5.09 ± 0.05 |

```
(bookworm)root@desktop:~# hyperfine --warmup 1 "3cpio -t /boot/initrd.img" "lsinitramfs /boot/initrd.img-6.1.0-21-amd64" --export-markdown list.md
Benchmark 1: 3cpio -t /boot/initrd.img
  Time (mean ± σ):     210.0 ms ±   2.3 ms    [User: 4.8 ms, System: 66.2 ms]
  Range (min … max):   207.1 ms … 214.7 ms    14 runs

Benchmark 2: lsinitramfs /boot/initrd.img-6.1.0-21-amd64
  Time (mean ± σ):     571.8 ms ±   1.9 ms    [User: 515.7 ms, System: 496.8 ms]
  Range (min … max):   568.7 ms … 574.5 ms    10 runs

Summary
  '3cpio -t /boot/initrd.img' ran
    2.72 ± 0.03 times faster than 'lsinitramfs /boot/initrd.img-6.1.0-21-amd64'
```

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -t /boot/initrd.img` | 210.0 ± 2.3 | 207.1 | 214.7 | 1.00 |
| `lsinitramfs /boot/initrd.img-6.1.0-21-amd64` | 571.8 ± 1.9 | 568.7 | 574.5 | 2.72 ± 0.03 |

Benchmark results on a desktop machine with an AMD Ryzen 7 5700G running Ubuntu
24.04 (noble) on 2024-08-06. The tests were done in chroots that use overlayfs
on tmpfs for writes:

```
$ schroot-wrapper -p initramfs-tools,linux-image-generic,firmware-linux,zstd,cryptsetup-initramfs,lvm2,kbd,mdadm,ntfs-3g,plymouth,console-setup,libarchive-tools,hyperfine -u root -c bookworm
(bookworm)root@desktop:~# mv /boot/initrd.img-6.1.0-23-amd64{,.zstd}
(bookworm)root@desktop:~# zstd --rm -d /boot/initrd.img-6.1.0-23-amd64.zstd
(bookworm)root@desktop:~# ( cd /boot && ln -s initrd.img-* initrd.img )
(bookworm)root@desktop:~# ls -l /boot/initrd.img*
lrwxrwxrwx 1 root root        25 Aug  6 01:57 /boot/initrd.img -> initrd.img-6.1.0-23-amd64
-rw-r--r-- 1 root root 282020864 Aug  6 01:56 /boot/initrd.img-6.1.0-23-amd64
(bookworm)root@desktop:~# 3cpio -t /boot/initrd.img | wc -l
2935
(bookworm)root@desktop:~# 3cpio -e /boot/initrd.img
0	cpio
(bookworm)root@desktop:~# hyperfine -N -w 2 -r 100 "3cpio -t /boot/initrd.img" "bsdcpio -itF /boot/initrd.img" "cpio -t --file /boot/initrd.img" --export-markdown list.md
Benchmark 1: 3cpio -t /boot/initrd.img
  Time (mean ± σ):       7.1 ms ±   0.1 ms    [User: 1.4 ms, System: 5.6 ms]
  Range (min … max):     6.9 ms …   7.4 ms    100 runs

Benchmark 2: bsdcpio -itF /boot/initrd.img
  Time (mean ± σ):      12.2 ms ±   0.3 ms    [User: 2.4 ms, System: 9.7 ms]
  Range (min … max):    11.4 ms …  13.0 ms    100 runs

Benchmark 3: cpio -t --file /boot/initrd.img
  Time (mean ± σ):     370.8 ms ±   2.7 ms    [User: 41.7 ms, System: 329.0 ms]
  Range (min … max):   366.7 ms … 381.3 ms    100 runs

Summary
  '3cpio -t /boot/initrd.img' ran
    1.70 ± 0.05 times faster than 'bsdcpio -itF /boot/initrd.img'
   51.96 ± 0.82 times faster than 'cpio -t --file /boot/initrd.img'
(bookworm)root@desktop:~# hyperfine -N -w 2 -r 100 "3cpio -tv /boot/initrd.img" "bsdcpio -itvF /boot/initrd.img" "cpio -tv --file /boot/initrd.img" --export-markdown list-verbose.md
Benchmark 1: 3cpio -tv /boot/initrd.img
  Time (mean ± σ):       9.1 ms ±   0.1 ms    [User: 2.9 ms, System: 6.2 ms]
  Range (min … max):     8.8 ms …   9.5 ms    100 runs

Benchmark 2: bsdcpio -itvF /boot/initrd.img
  Time (mean ± σ):      13.5 ms ±   0.4 ms    [User: 4.1 ms, System: 9.3 ms]
  Range (min … max):    12.7 ms …  14.9 ms    100 runs

Benchmark 3: cpio -tv --file /boot/initrd.img
  Time (mean ± σ):     383.3 ms ±   2.2 ms    [User: 45.1 ms, System: 338.1 ms]
  Range (min … max):   379.6 ms … 390.0 ms    100 runs

Summary
  '3cpio -tv /boot/initrd.img' ran
    1.48 ± 0.05 times faster than 'bsdcpio -itvF /boot/initrd.img'
   42.14 ± 0.58 times faster than 'cpio -tv --file /boot/initrd.img'
```

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -t /boot/initrd.img` | 7.2 ± 0.1 | 6.9 | 7.5 | 1.00 |
| `bsdcpio -itF /boot/initrd.img` | 12.6 ± 0.6 | 11.3 | 14.0 | 1.77 ± 0.09 |
| `cpio -t --file /boot/initrd.img` | 375.1 ± 4.8 | 368.2 | 390.6 | 52.45 ± 1.00 |

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -tv /boot/initrd.img` | 9.1 ± 0.1 | 8.8 | 9.5 | 1.00 |
| `bsdcpio -itvF /boot/initrd.img` | 13.5 ± 0.4 | 12.7 | 14.9 | 1.48 ± 0.05 |
| `cpio -tv --file /boot/initrd.img` | 383.3 ± 2.2 | 379.6 | 390.0 | 42.14 ± 0.58 |
