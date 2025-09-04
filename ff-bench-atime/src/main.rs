//! Benchmark reading a file with and without updating atime.
//!
//! On most Linux systems `relatime` is the default (see `mount(8)`). With `relatime`, the kernel
//! refreshes atime only when it’s stale (e.g. when mtime/ctime is newer, or once every day),
//! which keeps the overhead negligible.
//!
//! Using `--atime strict` mounts with strict atime, updating atime on every access. On my
//! machine, that leaves the median nearly unchanged but increases variability and tail latency
//! (p95/p99). If your filesystem already uses `relatime`, this flag makes no difference.
//!
//! TL;DR: This benchmark is for historical intrest, O_NOATIME has no practical speedup.
use anyhow::{Context, Result};
use clap::Parser;
use ff::fs::{create_ff_bench_dir, mkfs, mount_ff_bench, unmount};
use ff::summary;
use libc::O_NOATIME;
use nix::mount::MsFlags;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::{fs::OpenOptions, os::unix::fs::FileExt, time::Instant};

fn main() -> Result<()> {
    let args = Args::parse();
    let dev = std::fs::canonicalize("/dev/disk/by-partlabel/ff-bench")
        .expect("this should be a valid os path");
    let ff_dir = create_ff_bench_dir()?;

    println!("=> found ff-bench device: {:#?}", dev.as_path());
    println!("=> ff-bench directory: {:#?}", ff_dir.as_path());

    unmount(&ff_dir)?;
    mkfs(&dev, "ext4")?; // TODO: accept --fs {} instead of hardcoding ext4
    mount_ff_bench(&dev, &ff_dir, "ext4", args.atime.into(), &"".into())?;

    let mut test_file = OpenOptions::new()
        .read(true)
        .create(true)
        .write(true)
        .custom_flags(if args.no_atime { O_NOATIME } else { 0 })
        .open(ff_dir.join("test.txt"))
        .context("unable to create test.txt")?;

    // write dummy data that we can read
    test_file
        .write("ff-bench-atime".as_bytes())
        .context("failed to write to the test file")?;

    // warmup
    for _ in 0..1000 {
        let mut buf = [0u8];
        let _ = test_file.read_exact_at(&mut buf, 0);
    }

    let mut samples_ns: Vec<f64> = Vec::with_capacity(args.iterations);
    // read one byte per iteration
    for _ in 0..args.iterations {
        let mut buf = [0u8];
        let start = Instant::now();
        let res = test_file.read_exact_at(&mut buf, 0);
        samples_ns.push(start.elapsed().as_nanos() as f64);

        res.context("failed to read 1 byte from the test file")?;
    }

    summary(samples_ns);

    Ok(())
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Atime {
    Strict,
    Relatime,
}

impl From<Atime> for MsFlags {
    fn from(value: Atime) -> Self {
        match value {
            Atime::Strict => MsFlags::MS_STRICTATIME,
            Atime::Relatime => MsFlags::MS_RELATIME,
        }
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// read without modifying access time (O_NO_ATIME)
    #[arg(short, long)]
    no_atime: bool,
    /// number of iterations
    #[arg(short, long, default_value_t = 100_000)]
    iterations: usize,
    /// access time behaviour
    #[arg(long, value_enum, default_value_t = Atime::Relatime)]
    atime: Atime,
}
