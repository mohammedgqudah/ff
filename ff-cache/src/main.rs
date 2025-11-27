//! Show information about the cached pages of a specific file.
use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use ff::args::fmt_ranges;
use ff::pagemap::{KPageFlags, PageMapExt, vm_page_size};
use humansize::{BINARY, format_size};
use log::{LevelFilter, debug};
use std::cmp::min;
use std::fs::OpenOptions;
use std::io::Write;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
/// Show information about cached pages for a file.
struct Args {
    filename: String,
    /// Evict all cached pages.
    #[arg(short = 'e', long, default_value_t = false)]
    evict: bool,
    /// Show dirty pages
    #[arg(short = 'd', long, default_value_t = false)]
    dirty: bool,
    /// Verbose output.
    ///
    /// This will show the PageMapEntry flags (see man 5 proc_pid_pagemap), and
    ///
    /// physical page frame flags (see man 5 proc_kpageflags) for each cached page.
    #[arg(short = 'v', long, default_value_t = false)]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    env_logger::builder()
        .format(|buf, record| {
            let warn_style = buf.default_level_style(record.level());
            match record.level() {
                log::Level::Info => {
                    writeln!(buf, "{}", record.args())
                }
                _ => {
                    writeln!(buf, "{warn_style}{}{warn_style:#}", record.args())
                }
            }
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
