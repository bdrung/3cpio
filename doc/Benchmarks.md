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

Benchmark results on a Raspberry Pi Zero 2W running Ubuntu 24.04 (noble) arm64
on 2025-07-06:

```
$ sudo 3cpio -x /boot/initrd.img -C /var/tmp/initrd
$ ( cd /var/tmp/initrd && find . | LC_ALL=C sort | sudo cpio --reproducible --quiet -o -H newc ) > initrd.img
$ ls -l initrd.img
-rw-rw-r-- 1 user user 80422400 Jul  6 11:49 initrd.img
$ 3cpio -t initrd.img | wc -l
2542
$ 3cpio -e initrd.img
0	cpio
$ 3cpio -e /boot/initrd.img
0	cpio
42943488	zstd
$ hyperfine -N -w 2 -r 100 "3cpio -t initrd.img" "3cpio -tv initrd.img" "3cpio -t --debug initrd.img" "3cpio -t /boot/initrd.img" "3cpio -tv /boot/initrd.img" "3cpio -t --debug /boot/initrd.img" --export-markdown list-variants.md
Benchmark 1: 3cpio -t initrd.img
  Time (mean ± σ):      89.7 ms ±   1.1 ms    [User: 26.8 ms, System: 61.7 ms]
  Range (min … max):    87.6 ms …  92.8 ms    100 runs

Benchmark 2: 3cpio -tv initrd.img
  Time (mean ± σ):     112.4 ms ±   1.2 ms    [User: 47.8 ms, System: 63.4 ms]
  Range (min … max):   110.4 ms … 115.2 ms    100 runs

Benchmark 3: 3cpio -t --debug initrd.img
  Time (mean ± σ):     114.3 ms ±   1.1 ms    [User: 49.4 ms, System: 63.6 ms]
  Range (min … max):   112.1 ms … 117.7 ms    100 runs

Benchmark 4: 3cpio -t /boot/initrd.img
  Time (mean ± σ):     703.8 ms ±   2.6 ms    [User: 39.4 ms, System: 267.5 ms]
  Range (min … max):   699.1 ms … 712.0 ms    100 runs

Benchmark 5: 3cpio -tv /boot/initrd.img
  Time (mean ± σ):     722.5 ms ±   3.1 ms    [User: 61.9 ms, System: 268.3 ms]
  Range (min … max):   715.9 ms … 742.8 ms    100 runs

Benchmark 6: 3cpio -t --debug /boot/initrd.img
  Time (mean ± σ):     724.4 ms ±   2.5 ms    [User: 65.6 ms, System: 267.1 ms]
  Range (min … max):   719.2 ms … 733.6 ms    100 runs

Summary
  3cpio -t initrd.img ran
    1.25 ± 0.02 times faster than 3cpio -tv initrd.img
    1.27 ± 0.02 times faster than 3cpio -t --debug initrd.img
    7.85 ± 0.10 times faster than 3cpio -t /boot/initrd.img
    8.05 ± 0.10 times faster than 3cpio -tv /boot/initrd.img
    8.08 ± 0.10 times faster than 3cpio -t --debug /boot/initrd.img
```

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -t initrd.img` | 89.7 ± 1.1 | 87.6 | 92.8 | 1.00 |
| `3cpio -tv initrd.img` | 112.4 ± 1.2 | 110.4 | 115.2 | 1.25 ± 0.02 |
| `3cpio -t --debug initrd.img` | 114.3 ± 1.1 | 112.1 | 117.7 | 1.27 ± 0.02 |
| `3cpio -t /boot/initrd.img` | 703.8 ± 2.6 | 699.1 | 712.0 | 7.85 ± 0.10 |
| `3cpio -tv /boot/initrd.img` | 722.5 ± 3.1 | 715.9 | 742.8 | 8.05 ± 0.10 |
| `3cpio -t --debug /boot/initrd.img` | 724.4 ± 2.5 | 719.2 | 733.6 | 8.08 ± 0.10 |

Benchmark results on a Raspberry Pi Zero 2W running Ubuntu 24.04 (noble) arm64
on 2025-07-10:

```
$ ls -l /boot/initrd.img*
lrwxrwxrwx 1 root root       27 Jul  3 08:18 /boot/initrd.img -> initrd.img-6.8.0-1030-raspi
-rw-r--r-- 1 root root 57286143 Jul  3 08:23 /boot/initrd.img-6.8.0-1030-raspi
$ sudo 3cpio -x /boot/initrd.img -C initrd
$ ( cd initrd && find . ) | sed -e 's,\./,,g' | sort > files
$ wc -l < files
2542
$ sudo hyperfine -w 1 -p "rm -f initrd.img && sync && echo 3 > /proc/sys/vm/drop_caches" "3cpio -c initrd.img -C initrd < files" "cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files" "cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files" --export-markdown create-cold.md
Benchmark 1: 3cpio -c initrd.img -C initrd < files
  Time (mean ± σ):     10.743 s ±  0.213 s    [User: 0.140 s, System: 2.264 s]
  Range (min … max):   10.470 s … 11.176 s    10 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
  Time (mean ± σ):     12.200 s ±  0.339 s    [User: 0.576 s, System: 4.840 s]
  Range (min … max):   11.603 s … 12.749 s    10 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
  Time (mean ± σ):     12.154 s ±  0.502 s    [User: 0.839 s, System: 5.494 s]
  Range (min … max):   11.549 s … 12.946 s    10 runs

Summary
  3cpio -c initrd.img -C initrd < files ran
    1.13 ± 0.05 times faster than cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
    1.14 ± 0.04 times faster than cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
$ sudo hyperfine -w 2 -p "rm -f initrd.img && sync" "3cpio -c initrd.img -C initrd < files" "cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files" "cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files" --export-markdown create-warm.md
Benchmark 1: 3cpio -c initrd.img -C initrd < files
  Time (mean ± σ):      2.460 s ±  0.192 s    [User: 0.103 s, System: 1.129 s]
  Range (min … max):    2.266 s …  2.778 s    10 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
  Time (mean ± σ):      3.733 s ±  0.013 s    [User: 0.453 s, System: 3.257 s]
  Range (min … max):    3.716 s …  3.762 s    10 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
  Time (mean ± σ):      4.833 s ±  0.009 s    [User: 0.737 s, System: 4.069 s]
  Range (min … max):    4.821 s …  4.845 s    10 runs

Summary
  3cpio -c initrd.img -C initrd < files ran
    1.52 ± 0.12 times faster than cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
    1.96 ± 0.15 times faster than cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
$ stat -c %s initrd.img
80422400
$ { echo "#cpio: zstd -1" && cat files; } > manifest
$ sudo hyperfine -w 1 -p "rm -f initrd.img && sync && echo 3 > /proc/sys/vm/drop_caches" "3cpio -c initrd.img -C initrd < manifest" "cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img" "cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img" --export-markdown create-cold.md
Benchmark 1: 3cpio -c initrd.img -C initrd < manifest
  Time (mean ± σ):      8.667 s ±  0.291 s    [User: 0.197 s, System: 2.036 s]
  Range (min … max):    8.364 s …  9.127 s    10 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img
  Time (mean ± σ):     10.367 s ±  0.891 s    [User: 3.300 s, System: 5.913 s]
  Range (min … max):    9.507 s … 11.742 s    10 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img
  Time (mean ± σ):     10.276 s ±  0.921 s    [User: 3.612 s, System: 7.762 s]
  Range (min … max):    9.461 s … 12.092 s    10 runs

Summary
  3cpio -c initrd.img -C initrd < manifest ran
    1.19 ± 0.11 times faster than cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img
    1.20 ± 0.11 times faster than cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img
$ sudo hyperfine -w 1 -p "rm -f initrd.img && sync" "3cpio -c initrd.img -C initrd < manifest" "cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img" "cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img" --export-markdown create-warm.md
Benchmark 1: 3cpio -c initrd.img -C initrd < manifest
  Time (mean ± σ):      2.107 s ±  0.087 s    [User: 0.153 s, System: 0.942 s]
  Range (min … max):    2.024 s …  2.260 s    10 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img
  Time (mean ± σ):      2.874 s ±  0.029 s    [User: 3.237 s, System: 4.182 s]
  Range (min … max):    2.801 s …  2.903 s    10 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img
  Time (mean ± σ):      3.785 s ±  0.012 s    [User: 3.428 s, System: 5.966 s]
  Range (min … max):    3.767 s …  3.801 s    10 runs

Summary
  3cpio -c initrd.img -C initrd < manifest ran
    1.36 ± 0.06 times faster than cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img
    1.80 ± 0.07 times faster than cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img
$ stat -c %s initrd.img
57021773
```

Cold caches:

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < files` | 10.743 ± 0.213 | 10.470 | 11.176 | 1.00 |
| `cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files` | 12.200 ± 0.339 | 11.603 | 12.749 | 1.14 ± 0.04 |
| `cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files` | 12.154 ± 0.502 | 11.549 | 12.946 | 1.13 ± 0.05 |

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < manifest` | 8.667 ± 0.291 | 8.364 | 9.127 | 1.00 |
| `cd initrd && bsdcpio -o -H newc < ../files \| zstd -q1 -T0 > ../initrd.img` | 10.367 ± 0.891 | 9.507 | 11.742 | 1.20 ± 0.11 |
| `cd initrd && cpio -o -H newc --reproducible < ../files \| zstd -q1 -T0 > ../initrd.img` | 10.276 ± 0.921 | 9.461 | 12.092 | 1.19 ± 0.11 |

Warm caches (results rely heavily on the available amount of memory):

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < files` | 2.460 ± 0.192 | 2.266 | 2.778 | 1.00 |
| `cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files` | 3.733 ± 0.013 | 3.716 | 3.762 | 1.52 ± 0.12 |
| `cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files` | 4.833 ± 0.009 | 4.821 | 4.845 | 1.96 ± 0.15 |

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < manifest` | 2.107 ± 0.087 | 2.024 | 2.260 | 1.00 |
| `cd initrd && bsdcpio -o -H newc < ../files \| zstd -q1 -T0 > ../initrd.img` | 2.874 ± 0.029 | 2.801 | 2.903 | 1.36 ± 0.06 |
| `cd initrd && cpio -o -H newc --reproducible < ../files \| zstd -q1 -T0 > ../initrd.img` | 3.785 ± 0.012 | 3.767 | 3.801 | 1.80 ± 0.07 |

The manifest parsing in 3cpio took 740 ms with a cold cache and 140 ms with a warm cache.

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

Benchmark results on a desktop machine with an AMD Ryzen 7 5700G running Ubuntu
25.04 (plucky) on 2025-07-10. The tests were done in chroots that use overlayfs
on tmpfs for writes.

```
$ schroot-wrapper -p initramfs-tools,linux-image-generic,cryptsetup-initramfs,lvm2,kbd,mdadm,ntfs-3g,plymouth,console-setup,libarchive-tools,hyperfine -u root -c noble
(noble)root@desktop:~# ls -l /boot/initrd.img*
lrwxrwxrwx 1 root root       27 Jul  9 23:47 /boot/initrd.img -> initrd.img-6.8.0-63-generic
-rw-r--r-- 1 root root 67139659 Jul  9 23:47 /boot/initrd.img-6.8.0-63-generic
(noble)root@desktop:~# 3cpio -x /boot/initrd.img -C initrd
(noble)root@desktop:~# ( cd initrd && find . ) | sed -e 's,\./,,g' | sort > files
(noble)root@desktop:~# wc -l < files
1901
(noble)root@desktop:~# hyperfine -w 2 -r 100 -p "rm -f initrd.img && sync && echo 3 > /proc/sys/vm/drop_caches" "3cpio -c initrd.img -C initrd < files" "cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files" "cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files" --export-markdown create-cold.md
Benchmark 1: 3cpio -c initrd.img -C initrd < files
  Time (mean ± σ):      75.1 ms ±   4.3 ms    [User: 3.9 ms, System: 63.8 ms]
  Range (min … max):    67.5 ms …  80.7 ms    100 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
  Time (mean ± σ):     270.0 ms ±   6.5 ms    [User: 19.1 ms, System: 230.5 ms]
  Range (min … max):   256.6 ms … 281.4 ms    100 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
  Time (mean ± σ):     336.7 ms ±   5.0 ms    [User: 31.2 ms, System: 298.6 ms]
  Range (min … max):   328.2 ms … 348.8 ms    100 runs

Summary
  3cpio -c initrd.img -C initrd < files ran
    3.59 ± 0.22 times faster than cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
    4.48 ± 0.27 times faster than cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
(noble)root@desktop:~# hyperfine -w 2 -r 100 -p "rm -f initrd.img && sync" "3cpio -c initrd.img -C initrd < files" "cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files" "cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files" --export-markdown create-warm.md
Benchmark 1: 3cpio -c initrd.img -C initrd < files
  Time (mean ± σ):      60.8 ms ±   0.9 ms    [User: 3.9 ms, System: 56.9 ms]
  Range (min … max):    58.5 ms …  62.5 ms    100 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
  Time (mean ± σ):     237.2 ms ±   3.2 ms    [User: 18.3 ms, System: 218.8 ms]
  Range (min … max):   231.6 ms … 244.8 ms    100 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
  Time (mean ± σ):     322.5 ms ±   4.1 ms    [User: 29.6 ms, System: 292.8 ms]
  Range (min … max):   315.4 ms … 336.1 ms    100 runs

Summary
  3cpio -c initrd.img -C initrd < files ran
    3.90 ± 0.08 times faster than cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
    5.30 ± 0.10 times faster than cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
(noble)root@desktop:~# stat -c %s initrd.img
83589120
(noble)root@desktop:~# { echo "#cpio: zstd -1" && cat files; } > manifest
(noble)root@desktop:~# hyperfine -w 2 -r 100 -p "rm -f initrd.img && sync && echo 3 > /proc/sys/vm/drop_caches" "3cpio -c initrd.img -C initrd < manifest" "cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img" "cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img" --export-markdown create-cold.md
Benchmark 1: 3cpio -c initrd.img -C initrd < manifest
  Time (mean ± σ):      80.8 ms ±   4.6 ms    [User: 6.0 ms, System: 62.7 ms]
  Range (min … max):    72.9 ms …  89.6 ms    100 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img
  Time (mean ± σ):     179.6 ms ±   6.2 ms    [User: 161.9 ms, System: 260.8 ms]
  Range (min … max):   167.9 ms … 192.9 ms    100 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img
  Time (mean ± σ):     322.6 ms ±   4.8 ms    [User: 193.4 ms, System: 464.3 ms]
  Range (min … max):   312.5 ms … 333.1 ms    100 runs

Summary
  3cpio -c initrd.img -C initrd < manifest ran
    2.22 ± 0.15 times faster than cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img
    3.99 ± 0.24 times faster than cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img
(noble)root@desktop:~# hyperfine -w 2 -r 100 -p "rm -f initrd.img && sync" "3cpio -c initrd.img -C initrd < manifest" "cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img" "cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img" --export-markdown create-warm.md
Benchmark 1: 3cpio -c initrd.img -C initrd < manifest
  Time (mean ± σ):      63.2 ms ±   1.7 ms    [User: 6.6 ms, System: 54.1 ms]
  Range (min … max):    60.0 ms …  68.1 ms    100 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img
  Time (mean ± σ):     151.7 ms ±   2.3 ms    [User: 160.4 ms, System: 248.2 ms]
  Range (min … max):   147.1 ms … 158.2 ms    100 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img
  Time (mean ± σ):     306.1 ms ±   3.0 ms    [User: 194.7 ms, System: 456.0 ms]
  Range (min … max):   300.2 ms … 313.6 ms    100 runs

Summary
  3cpio -c initrd.img -C initrd < manifest ran
    2.40 ± 0.07 times faster than cd initrd && bsdcpio -o -H newc < ../files | zstd -q1 -T0 > ../initrd.img
    4.84 ± 0.14 times faster than cd initrd && cpio -o -H newc --reproducible < ../files | zstd -q1 -T0 > ../initrd.img
(noble)root@desktop:~# stat -c %s initrd.img
67215993
```

Cold cache:

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < files` | 75.1 ± 4.3 | 67.5 | 80.7 | 1.00 |
| `cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files` | 270.0 ± 6.5 | 256.6 | 281.4 | 3.59 ± 0.22 |
| `cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files` | 336.7 ± 5.0 | 328.2 | 348.8 | 4.48 ± 0.27 |

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < manifest` | 80.8 ± 4.6 | 72.9 | 89.6 | 1.00 |
| `cd initrd && bsdcpio -o -H newc < ../files \| zstd -q1 -T0 > ../initrd.img` | 179.6 ± 6.2 | 167.9 | 192.9 | 2.22 ± 0.15 |
| `cd initrd && cpio -o -H newc --reproducible < ../files \| zstd -q1 -T0 > ../initrd.img` | 322.6 ± 4.8 | 312.5 | 333.1 | 3.99 ± 0.24 |

Warm cache:

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < files` | 60.8 ± 0.9 | 58.5 | 62.5 | 1.00 |
| `cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files` | 237.2 ± 3.2 | 231.6 | 244.8 | 3.90 ± 0.08 |
| `cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files` | 322.5 ± 4.1 | 315.4 | 336.1 | 5.30 ± 0.10 |

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < manifest` | 63.2 ± 1.7 | 60.0 | 68.1 | 1.00 |
| `cd initrd && bsdcpio -o -H newc < ../files \| zstd -q1 -T0 > ../initrd.img` | 151.7 ± 2.3 | 147.1 | 158.2 | 2.40 ± 0.07 |
| `cd initrd && cpio -o -H newc --reproducible < ../files \| zstd -q1 -T0 > ../initrd.img` | 306.1 ± 3.0 | 300.2 | 313.6 | 4.84 ± 0.14 |

The manifest parsing in 3cpio took 21 ms with a cold cache and 6 ms with a warm cache.

Benchmark results on a desktop machine with an AMD Ryzen 7 5700G running Ubuntu
25.04 (plucky) on a Samsung SSD 980 PRO NMVe with Dracut on 2025-07-10:

```
$ ls -l /boot/initrd.img*
lrwxrwxrwx 1 root root       28 Jul  2 11:35 /boot/initrd.img -> initrd.img-6.14.0-23-generic
-rw------- 1 root root 28276693 Jul  4 09:58 /boot/initrd.img-6.14.0-23-generic
$ sudo 3cpio -x /boot/initrd.img -C initrd
$ ( cd initrd && find . ) | sed -e 's,\./,,g' | sort > files
$ wc -l < files
1714
$ sudo hyperfine -w 1 -p "rm -f initrd.img && sync && echo 3 > /proc/sys/vm/drop_caches" "3cpio -c initrd.img -C initrd < files" "cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files" "cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files" --export-markdown create-cold.md
Benchmark 1: 3cpio -c initrd.img -C initrd < files
  Time (mean ± σ):     257.2 ms ±   3.4 ms    [User: 5.4 ms, System: 111.2 ms]
  Range (min … max):   252.8 ms … 262.4 ms    10 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
  Time (mean ± σ):     490.5 ms ±   5.8 ms    [User: 19.7 ms, System: 341.8 ms]
  Range (min … max):   482.1 ms … 497.7 ms    10 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
  Time (mean ± σ):     559.3 ms ±   6.3 ms    [User: 33.9 ms, System: 416.1 ms]
  Range (min … max):   547.5 ms … 570.8 ms    10 runs

Summary
  3cpio -c initrd.img -C initrd < files ran
    1.91 ± 0.03 times faster than cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
    2.17 ± 0.04 times faster than cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
$ sudo hyperfine -w 2 -p "rm -f initrd.img && sync" "3cpio -c initrd.img -C initrd < files" "cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files" "cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files" --export-markdown create-warm.md
Benchmark 1: 3cpio -c initrd.img -C initrd < files
  Time (mean ± σ):      65.1 ms ±   1.4 ms    [User: 3.6 ms, System: 61.4 ms]
  Range (min … max):    63.1 ms …  68.2 ms    33 runs

Benchmark 2: cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
  Time (mean ± σ):     298.8 ms ±   3.2 ms    [User: 16.0 ms, System: 282.7 ms]
  Range (min … max):   295.6 ms … 304.3 ms    10 runs

Benchmark 3: cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
  Time (mean ± σ):     382.6 ms ±   7.6 ms    [User: 30.8 ms, System: 351.7 ms]
  Range (min … max):   370.2 ms … 393.2 ms    10 runs

Summary
  3cpio -c initrd.img -C initrd < files ran
    4.59 ± 0.11 times faster than cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files
    5.87 ± 0.17 times faster than cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files
$ stat -c %s initrd.img
68406784
```

Cold cache:

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < files` | 257.2 ± 3.4 | 252.8 | 262.4 | 1.00 |
| `cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files` | 490.5 ± 5.8 | 482.1 | 497.7 | 1.91 ± 0.03 |
| `cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files` | 559.3 ± 6.3 | 547.5 | 570.8 | 2.17 ± 0.04 |

Warm cache:

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `3cpio -c initrd.img -C initrd < files` | 65.1 ± 1.4 | 63.1 | 68.2 | 1.00 |
| `cd initrd && bsdcpio -o -H newc > ../initrd.img < ../files` | 298.8 ± 3.2 | 295.6 | 304.3 | 4.59 ± 0.11 |
| `cd initrd && cpio -o -H newc --reproducible > ../initrd.img < ../files` | 382.6 ± 7.6 | 370.2 | 393.2 | 5.87 ± 0.17 |

The manifest parsing in 3cpio took 40 ms with a cold cache and 5 ms with a warm cache.
