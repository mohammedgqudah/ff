<div align="center">
  <h1>ff</h1>
  <p>A toolkit for testing and studying filesystems</p>
</div>
<br>

<div align="center">
  <!-- Tests -->
  <a href="https://github.com/mohammedgqudah/ff/actions/workflows/ci.yml">
    <img src="https://github.com/mohammedgqudah/ff/actions/workflows/ci.yml/badge.svg?style=flat-square" alt="unit-tests">
  </a>
  <!-- License -->
  <a href="https://opensource.org/licenses/MIT">
    <img src="https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square" alt="license">
  </a>
</div>
<br>

This is my personal toolkit for studying filesystems in Linux. The `ff` crate serves as a utility for mounting, and dealing with filesysems in all binaries.

- [ff-cache](#ff-cache): Show or evict cached pages for a file.
- [ff-trace-fsync](#ff-trace-fsync): See how `fsync` behaves under different conditions (e.g. block IO errors, multiple open file handles, different filesystems)
- [ff-bench-fsync](#ff-bench-fsync): Benchmark syncing data to disk using different methods (whether it's an `fsync` call, or a filesystem mounted with the `sync` option, or a file opened with `O_SYNC` etc...)

---

# ff-cache
Show or evict cached pages in the page cache.

It is similar to [`vmtouch`](https://github.com/hoytech/vmtouch) but you can show dirty pages and inspect page mapping flags.

```console
$ ff-cache target.txt
                Resident Pages: 0/2560 0B/10MiB
```
```console
$ ff-cache target.txt
                Resident Pages: 12/2560 48KiB/10MiB
```
```console
$ sudo ff-cache target.txt --dirty
                Resident Pages: 13/2561 52KiB/10.00MiB
                Dirty Pages: 1/2561
                             2560
```
```console
$ sudo ff-cache target.txt -e
                Evicted 13/2561 52KiB/10.00MiB
```
```console
$ sudo ff-cache target.txt --dirty
                Resident Pages: 1/2561 4KiB/10.00MiB
                Dirty Pages: 0/2561
```

# ff-trace-fsync

# ff-bench-fsync
benchmark `fsync(2)` and related system calls.

[![asciicast](https://asciinema.org/a/orYnxYPLO82QWoMJvBLHWu65U.svg)](https://asciinema.org/a/orYnxYPLO82QWoMJvBLHWu65U)

## creating a drive partition for benchmarking and testing

`ff` needs a partition that will be used to do all sorts of tests and benchmarks, it will refuse to work if the partition label is not `ff-bench`, which can be set using `parted`

```sh
sudo parted /dev/<YOUR_DRIVE> name <TESTING_PART_NUMBER> ff-bench
```
