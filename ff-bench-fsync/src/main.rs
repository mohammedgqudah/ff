//! Benchmark fsync.
//!
//! # Examples
//! ff-bench-fsync --fs ext4 --mode fsync # benchmark a `write` followed by `fsync`
//! ff-bench-fsync --fs ext4 --mode fdatasync # benchmark a `write` followed by `fdatasync`
//! ff-bench-fsync --fs ext4 --mode nosync # benchmark a `write`
//! ff-bench-fsync --fs ext4 --mode open_sync  # benchmark a `write` on a file opened with O_SYNC
//! ff-bench-fsync --fs ext4 --mode open_datasync  # benchmark a `write` on a file opened with O_DSYNC
//! ff-bench-fsync --fs ext4 --mode nosync -o sync  # benchmark a `write` on a MS_SYNCHRONOUS mount
use std::{
    fs::OpenOptions,
    iter::repeat_with,
    os::unix::fs::{FileExt, OpenOptionsExt},
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use clap::Parser;
use ff::{fs::setup_and_mount, summary};
use indicatif::ProgressBar;

fn main() -> Result<()> {
    // the mean increases by ~40 microseconds if we don't pin
    if let Some(cores) = core_affinity::get_core_ids() {
        core_affinity::set_for_current(cores[0]);
    }

    let args = Args::parse();
    let (dev, ff_dir) = setup_and_mount(None::<String>, args.fs, args.mount_options)?;

    println!("=> found ff-bench device: {:#?}", dev.as_path());
    println!("=> ff-bench directory: {:#?}", ff_dir.as_path());

    let buf: Vec<u8> = repeat_with(|| fastrand::u8(0..=255))
        .take(args.buffer_size)
        .collect();

    let file = OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .custom_flags(match args.mode {
            Mode::OpenSync => libc::O_SYNC,
            Mode::OpenDataSync => libc::O_DSYNC,
            _ => 0,
        })
        .open(ff_dir.join("test.txt"))?;

    // resize the file to fit the benchmark writes
    // to avoid benchmarking the resize during a write
    file.set_len((args.buffer_size * 2) as u64)
        .context("failed to resize test file")?;
    let mut samples_ns = Vec::<f64>::with_capacity(args.iterations);

    let pb = ProgressBar::new(args.iterations as _);
    for _ in 0..args.iterations {
        let start = Instant::now();
        let write_result = file.write_at(buf.as_slice(), 0);
        let sync_result = match args.mode {
            Mode::FSync => file.sync_all(),
            Mode::FDataSync => file.sync_data(),
            Mode::NoSync | Mode::OpenSync | Mode::OpenDataSync => Ok(()),
        };

        samples_ns.push(start.elapsed().as_nanos() as f64);
        pb.inc(1);

        let size = write_result.context("failed to write")?;
        ensure!(size == args.buffer_size, "only {} bytes were written", size);
        sync_result.context("failed to sync")?;
    }

    pb.finish_and_clear();

    summary(samples_ns);
    Ok(())
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// filesystem to mount
    #[arg(long)]
    fs: String,
    /// mount(8)-style options
    #[arg(short = 'o', long, default_value = "")]
    mount_options: String,
    /// iterations
    #[arg(short, long, default_value_t = 10000)]
    iterations: usize,
    /// benchmark mode
    #[arg(short, long)]
    mode: Mode,
    // size
    #[arg(short = 'z', long, default_value_t = 0x2000)]
    buffer_size: usize,
}

#[derive(clap::ValueEnum, Clone, Debug)]
#[allow(clippy::enum_variant_names)]
enum Mode {
    #[value(name = "fdatasync")]
    FDataSync,
    #[value(name = "fsync")]
    FSync,
    #[value(name = "nosync")]
    NoSync,
    #[value(name = "open_sync")]
    OpenSync,
    #[value(name = "open_datasync")]
    OpenDataSync,
}
