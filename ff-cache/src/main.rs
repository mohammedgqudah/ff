//! Bring file pages into the page cache and print their details
use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use ff::pagemap::{KPageFlags, PageMapExt, vm_page_size};
use humansize::{BINARY, format_size};
use log::{LevelFilter, debug};
use std::cmp::min;
use std::fs::OpenOptions;
use std::io::Write;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    filename: String,
    /// Evict all cached pages.
    #[arg(short = 'e', long, default_value_t = false)]
    evict: bool,
    /// Verbose output.
    ///
    /// This will show the PageMapEntry flags (see man 5 proc_pid_pagemap), and
    ///
    /// physical page frame flags (see man 5 proc_kpageflags) for each cached page.
    #[arg(short = 'v', long, default_value_t = false)]
    verbose: bool,
    /// Show dirty pages
    #[arg(short = 'd', long, default_value_t = false)]
    dirty: bool,
}

/// Returns a string representation of the the range `nums`.
///
/// ```rust
/// assert_eq!(&[1, 2, 3, 4], "1-4");
/// assert_eq!(&[1, 2, 3, 4, 9], "1-4, 9");
/// ```
pub fn fmt_ranges(nums: &[u64]) -> String {
    if nums.is_empty() {
        return String::new();
    }

    let mut ranges = Vec::new();
    let mut start = nums[0];
    let mut end = nums[0];

    for &num in &nums[1..] {
        if num == end + 1 {
            // Extend the current range
            end = num;
        } else {
            // Close current range
            if start == end {
                ranges.push(format!("{}", start));
            } else {
                ranges.push(format!("{}-{}", start, end));
            }
            // Start a new range
            start = num;
            end = num;
        }
    }

    // Push the last range
    if start == end {
        ranges.push(format!("{}", start));
    } else {
        ranges.push(format!("{}-{}", start, end));
    }

    ranges.join(", ")
}

fn main() -> Result<()> {
    let args = Args::parse();
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}: {}",
                record.level().to_string().blue(),
                record.args()
            )
        })
        .filter(
            None,
            if args.verbose {
                LevelFilter::Debug
            } else {
                LevelFilter::Info
            },
        )
        .init();

    let file = OpenOptions::new()
        .read(true)
        .open(&args.filename)
        .context(format!("failed to open {}", args.filename))?;

    let fs_block_size = file.fs_block_size()?;
    let vm_page_size = vm_page_size()?;

    debug!("fs block size: {}", fs_block_size);
    debug!("vm page size: {}", vm_page_size);

    let resident_pages = file.resident_pages()?;

    let len = file
        .metadata()
        .context("failed getting metadata for the file")?
        .len();

    let number_of_pages = len.div_ceil(vm_page_size as u64);

    let formatter = BINARY.space_after_value(false);

    if args.evict {
        file.evict_pages()?;
        println!(
            "\t\tEvicted {}/{} {}/{}",
            resident_pages.len().to_string().bold(),
            number_of_pages.to_string().bold(),
            format_size(
                min((resident_pages.len() as u64) * vm_page_size, len),
                formatter
            )
            .bold(),
            format_size(len, formatter).bold()
        );

        return Ok(());
    }

    println!(
        "\t\tResident Pages: {}/{} {}/{}",
        resident_pages.len().to_string().bold(),
        number_of_pages.to_string().bold(),
        format_size(
            min((resident_pages.len() as u64) * vm_page_size, len),
            formatter
        )
        .bold(),
        format_size(len, formatter).bold()
    );

    // show dirty pages in cache
    if args.dirty {
        let dirty_pages = resident_pages
            .iter()
            .map(|page| {
                let (_, kflags) = file.page_info(*page)?;
                if kflags.contains(KPageFlags::DIRTY) {
                    Ok(Some(*page))
                } else {
                    Ok(None)
                }
            })
            .filter_map(|r| r.transpose())
            .collect::<Result<Vec<_>>>()?;

        println!(
            "\t\tDirty Pages: {}/{}",
            dirty_pages.len().to_string().bold(),
            number_of_pages.to_string().bold(),
        );
        println!("\t\t             {}", fmt_ranges(dirty_pages.as_slice()));
    }

    if args.verbose {
        for i in resident_pages {
            let (pagemap, kflags) = file.page_info(i)?;
            println!("{} {}", "PAGE".bold(), i.to_string().cyan());

            println!(
                " {}\t  {}",
                "pagemap (PageMapEntry)".dimmed(),
                pagemap.to_string().dimmed()
            );
            println!(
                " {}\t  {}",
                "kflags (KPageFlags)".dimmed(),
                kflags.to_string().dimmed()
            );
            println!();
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::fmt_ranges;

    #[test]
    fn fmt_range_test() {
        assert_eq!(fmt_ranges(&[]), "");

        assert_eq!(fmt_ranges(&[1]), "1");
        assert_eq!(fmt_ranges(&[1, 2]), "1-2");
        assert_eq!(fmt_ranges(&[1, 2, 3, 4, 5]), "1-5");
        assert_eq!(fmt_ranges(&[1, 2, 3, 4, 5, 9]), "1-5, 9");
        assert_eq!(fmt_ranges(&[1, 2, 4, 8, 9, 15]), "1-2, 4, 8-9, 15");
    }
}
