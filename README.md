# ff
ff is my personal toolkit for studying filesystems in Linux. The `ff` crate serves as a utility for mounting, and dealing with filesysems in all binaries.

## ff-cache
show or evict cached pages in the page cache.

it is similar to `vmtouch` but you can show dirty pages and inspect page mapping flags.

```shell
$ truncate --size=10M target.txt
$ ff-cache target.txt
                Resident Pages: 0/2560 0B/10MiB

# read 1K bytes (bring some pages into page cache)
$ head -c1K target.txt > /dev/null
$ ff-cache target.txt
                Resident Pages: 12/2560 48KiB/10MiB

# dirty the last page
$ echo test >> target.txt
$ ff-cache target.txt
                Resident Pages: 13/2561 52KiB/10.00MiB
$ sudo ff-cache target.txt --dirty
                Resident Pages: 13/2561 52KiB/10.00MiB
                Dirty Pages: 1/2561
                             2560
$ sudo ff-cache target.txt -e
                Evicted 13/2561 52KiB/10.00MiB
$ sudo ff-cache target.txt --dirty
                Resident Pages: 1/2561 4KiB/10.00MiB
                Dirty Pages: 0/2561

$ sudo ff-cache target.txt --verbose
                Resident Pages: 1/2561 4KiB/10.00MiB
PAGE 2560
 pagemap (PageMapEntry)   SOFT_DIRTY | EXCL_MAP | FILE_PAGE_OR_SHARED_ANON | PRESENT | 0xdbbf01
 kflags (KPageFlags)      UPTODATE | LRU | MMAP | 0x800000000
```

## ff-bench-fsync
benchmark `fsync(2)` and related system calls.

[![asciicast](https://asciinema.org/a/3M0aK63AYTkEvswiAHnwcuSle.svg)](https://asciinema.org/a/3M0aK63AYTkEvswiAHnwcuSle)

## creating a drive partition for benchmarking and testing

`ff` needs a partition that will be used to do all sorts of tests and benchmarks, it will refuse to work if the partition label is not `ff-bench`, which can be set using `parted`

```sh
sudo parted /dev/<YOUR_DRIVE> name <TESTING_PART_NUMBER> ff-bench
```
