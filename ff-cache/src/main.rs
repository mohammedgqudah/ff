//! Bring file pages into the page cache and print their details
use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use ff::pagemap::{PageMapExt, vm_page_size};
use humansize::{BINARY, format_size};
use log::{LevelFilter, debug};
use std::cmp::min;
use std::fs::OpenOptions;
use std::io::Write;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    filename: String,
    #[arg(short = 'e', default_value_t = false)]
    evict: bool,
    #[arg(short = 'v', default_value_t = false)]
    verbose: bool,
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
