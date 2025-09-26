//! inspect file page maps.
use anyhow::{Context, Result, ensure};
use bitflags::bitflags;
use colored::Colorize;
use log::debug;
use nix::{
    libc::{
        MADV_RANDOM, MAP_FAILED, MAP_PRIVATE, MAP_SHARED, POSIX_FADV_DONTNEED, PROT_NONE,
        PROT_READ, madvise, mincore, mmap64, munmap, posix_fadvise64,
    },
    sys::statfs::fstatfs,
    unistd::sysconf,
};
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::{AsFd, AsRawFd},
        unix::fs::FileExt,
    },
};

macro_rules! cvt {
    ($expr:expr) => {{
        let ret = $expr;
        if ret == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }};
}

// link: https://www.kernel.org/doc/Documentation/vm/pagemap.txt
//
// bits 0-54  page frame number (PFN) if present
// bits 0-4   swap type if swapped
// bits 5-54  swap offset if swapped
// bit  55    pte is soft-dirty (see Documentation/admin-guide/mm/soft-dirty.rst)
// bit  56    page exclusively mapped
// bit  57    pte is uffd-wp write-protected
// bit  58    pte is a guard region
// bits 59-60 zero
// bit  61    page is file-page or shared-anon
// bit  62    page swapped
// bit  63    page present
bitflags! {
   /// A 64-bit entry in /proc/self/pagemap
   /// see: `man 5 proc_pid_pagemap`
   #[derive(Debug)]
   pub struct PageMapEntry: u64 {
        const SOFT_DIRTY = 1 << 55;
        const EXCL_MAP = 1 << 56;
        // If set, the page is write-protected through userfaultfd(2).
        const PTE_UFFD_WP_WR_PROTECTED = 1 << 57;
        const PTE_GUARD_REGION = 1 << 58;
        const FILE_PAGE_OR_SHARED_ANON = 1 << 61;
        const SWAPPED = 1 << 62;
        const PRESENT = 1 << 63;
    }
}

bitflags! {
   #[derive(Debug)]
    pub struct KPageFlags: u64 {
        const LOCKED        = 1 << 0;
        const ERROR         = 1 << 1;
        const REFERENCED    = 1 << 2;
        const UPTODATE      = 1 << 3;
        const DIRTY         = 1 << 4;
        const LRU           = 1 << 5;
        const ACTIVE        = 1 << 6;
        const SLAB          = 1 << 7;
        const WRITEBACK     = 1 << 8;
        const RECLAIM       = 1 << 9;
        const BUDDY         = 1 << 10;
        const MMAP          = 1 << 11;
        const ANON          = 1 << 12;
        const SWAPCACHE     = 1 << 13;
        const SWAPBACKED    = 1 << 14;
        const COMPOUND_HEAD = 1 << 15;
        const COMPOUND_TAIL = 1 << 16;
        const HUGE          = 1 << 17;
        const UNEVICTABLE   = 1 << 18;
        const HWPOISON      = 1 << 19;
        const NOPAGE        = 1 << 20;
        const KSM           = 1 << 21;
        const THP           = 1 << 22;
        const BALLOON       = 1 << 23;
        const ZERO_PAGE     = 1 << 24;
        const IDLE          = 1 << 25;
    }
}

pub trait PageMapExt {
    fn page_info(&self, page: usize) -> Result<(PageMapEntry, KPageFlags)>;
    fn evict_pages(&self) -> Result<()>;
    fn resident_pages(&self) -> Result<Vec<u64>>;
    fn fs_block_size(&self) -> Result<usize>;
}

impl PageMapExt for File {
    fn fs_block_size(&self) -> Result<usize> {
        let stats = fstatfs(&self).context("failed to get stats for file")?;
        Ok(stats.block_size() as usize)
    }

    /// Returns the page map entry and kernel flags for `page`.
    ///
    /// This will fault the page in to make it present.
    fn page_info(&self, page: usize) -> Result<(PageMapEntry, KPageFlags)> {
        // TODO: check file_len before dereferencing
        let vm_page = vm_page_size()?;

        let fs_block_size = self.fs_block_size()?;

        let byte_off = fs_block_size * page;
        // mmap offset must be aligned to vm page size.
        let mmap_off = byte_off & !(vm_page - 1);
        let delta = byte_off - mmap_off;

        // SAFETY: we have exclusive access to the file.
        let mmap_address = unsafe {
            mmap64(
                std::ptr::null_mut(),
                vm_page,
                PROT_READ,
                MAP_PRIVATE,
                self.as_fd().as_raw_fd(),
                mmap_off as i64,
            )
        };

        ensure!(
            mmap_address != MAP_FAILED,
            "failed to mmap page `{}` for `{}`",
            page,
            self.as_fd().as_raw_fd()
        );

        // Touching a non-present page (page fault) will trigger a readahead by the linux kernel, which
        // will bring adjecent pages into cache.
        //
        // Note: The man page doesn't explicitly say that `MADV_RANDOM` is guaranteed to disable readahead,
        // but it strongly suggets that, and the kernel function `do_sync_mmap_readahead` returns
        // early if the VMA has the RAND_READ flag.
        cvt!(unsafe { madvise(mmap_address, vm_page, MADV_RANDOM) })
            .context("failed to disable readahead on mmaped region")?;

        // SAFETY: this is a valid aligned pointer.
        unsafe {
            // read one byte to cause a page fault and make the page present.
            mmap_address.add(delta).cast::<u8>().read_volatile();
        }

        let pagemap_entry = get_page_map_entry((mmap_address as usize) / vm_page)?;
        // ideally, this should never error because we faulted the page.
        let pfn = pagemap_entry.pfn().context(format!(
            "the PFN for {} is not present",
            self.as_fd().as_raw_fd()
        ))?;

        let kpage_flags = get_kernel_page(pfn)?;

        // SAFETY: we are unmapping an mmaped region that we no longer use or need.
        let munmap_ret = unsafe { munmap(mmap_address, vm_page) };

        ensure!(
            munmap_ret == 0,
            "failed to munmap page `{}` for `{}`",
            page,
            self.as_fd().as_raw_fd()
        );

        Ok((pagemap_entry, kpage_flags))
    }

    /// Returns a list of file pages present in the page cache.
    ///
    /// See `man 2 mincore` and `man 2 posix_fadvise`:
    ///
    /// > One can obtain a snapshot of which pages of a file are resident in
    /// > the buffer cache by  opening  a  file,  mapping  it  with
    /// > mmap(2), and then applying mincore(2) to the mapping.
    fn resident_pages(&self) -> Result<Vec<u64>> {
        let vm_page = vm_page_size()? as u64;
        let len = self
            .metadata()
            .context("failed getting metadata for the file")?
            .len();
        let number_of_pages = len.div_ceil(vm_page as u64);

        debug!("file has `{}` pages", number_of_pages.to_string().bold());

        // SAFETY: we have exclusive access to the file.
        let mmap_address = unsafe {
            mmap64(
                std::ptr::null_mut(),
                (vm_page * number_of_pages) as usize,
                PROT_NONE,
                MAP_SHARED,
                self.as_fd().as_raw_fd(),
                0,
            )
        };

        ensure!(
            mmap_address != MAP_FAILED,
            "faied to mmap file `{}`",
            self.as_raw_fd(),
        );

        let mut vec = vec![0u8; number_of_pages as usize];

        // SAFETY: vec is large enough and is a buffer of bytes
        let mincore_ret = unsafe {
            mincore(
                mmap_address,
                (vm_page * number_of_pages) as usize,
                vec.as_mut_ptr() as _,
            )
        };
        ensure!(
            mincore_ret == 0,
            "mincore(2) failed for `{}`",
            self.as_raw_fd(),
        );

        // SAFETY: we are unmapping an mmaped region that we no longer use or need.
        let munmap_ret = unsafe { munmap(mmap_address, (vm_page * number_of_pages) as usize) };

        ensure!(
            munmap_ret == 0,
            "failed to munmap page `{}`",
            self.as_fd().as_raw_fd()
        );

        Ok(vec
            .iter()
            .enumerate()
            .filter_map(|(page_num, b)| {
                // if the page is in page cache then it will not be zero,
                // because the least significant bit will be set. See: `man 2 mincore`
                if *b != 0 { Some(page_num as u64) } else { None }
            })
            .collect())
    }

    /// Returns a list of file pages present in the page cache.
    fn evict_pages(&self) -> Result<()> {
        let vm_page = vm_page_size()?;
        let len = self
            .metadata()
            .context("failed getting metadata for the file")?
            .len();
        let number_of_pages = len.div_ceil(vm_page as u64) as usize;

        // SAFETY: we have exclusive access to the file.
        let mmap_address = unsafe {
            mmap64(
                std::ptr::null_mut(),
                (vm_page * number_of_pages) as usize,
                PROT_NONE,
                MAP_PRIVATE,
                self.as_fd().as_raw_fd(),
                0,
            )
        };

        ensure!(
            mmap_address != MAP_FAILED,
            "faied to mmap file `{}`",
            self.as_raw_fd(),
        );

        let ret = unsafe { posix_fadvise64(self.as_raw_fd(), 0, len as _, POSIX_FADV_DONTNEED) };
        assert!(ret == 0, "couldnt evict");
        Ok(())
    }
}

/// return `PageMapEntry` for `page` from /proc/self/pagemap
pub fn get_page_map_entry(page: usize) -> Result<PageMapEntry> {
    let pagemap_file = OpenOptions::new()
        .read(true)
        .open("/proc/self/pagemap")
        .context("failed to open /proc/self/pagemap")?;

    let mut buf = [0u8; 8];
    pagemap_file
        .read_exact_at(&mut buf, (page * size_of::<PageMapEntry>()) as u64)
        .context("faild to read first entry")?;

    let raw_entry = u64::from_ne_bytes(buf);
    Ok(PageMapEntry::from_bits_retain(raw_entry))
}

/// return `KPageFlags` for for the Physical Frame Number (PFN)
pub fn get_kernel_page(pfn: u64) -> Result<KPageFlags> {
    let pagemap_file = OpenOptions::new()
        .read(true)
        .open("/proc/kpageflags")
        .context("failed to open /proc/kpageflags")?;

    let mut buf = [0u8; 8];
    pagemap_file
        .read_exact_at(&mut buf, pfn * size_of::<KPageFlags>() as u64)
        .context("faild to read first entry")?;

    let raw_entry = u64::from_ne_bytes(buf);

    Ok(KPageFlags::from_bits_retain(raw_entry))
}

impl PageMapEntry {
    pub fn pfn(&self) -> Option<u64> {
        // docs: bits 0-54  page frame number (PFN) if present
        let pfn_mask = (1u64 << 55) - 1;
        let pfn = self.bits() & pfn_mask;

        if self.contains(PageMapEntry::PRESENT) {
            // TODO: this will be zero if user is not a superuser. Show a better error message.
            assert!(pfn != 0);
            Some(pfn)
        } else {
            None
        }
    }
}

impl std::fmt::Display for KPageFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        bitflags::parser::to_writer(self, f)
    }
}
impl std::fmt::Display for PageMapEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        bitflags::parser::to_writer(self, f)
    }
}

pub fn vm_page_size() -> Result<usize> {
    Ok(sysconf(nix::unistd::SysconfVar::PAGE_SIZE)
        .context("failed to get sys page size")?
        .expect("page_size should be a supported option") as usize)
}
