//! Benchmark fsync.
use anyhow::Result;
use clap::Parser;
use ff::fs::{create_ff_bench_dir, setup_and_mount};

fn main() -> Result<()> {
    let args = Args::parse();
    let dev = std::fs::canonicalize("/dev/disk/by-partlabel/ff-bench")
        .expect("this should be a valid os path");
    let ff_dir = create_ff_bench_dir()?;

    println!("=> found ff-bench device: {:#?}", dev.as_path());
    println!("=> ff-bench directory: {:#?}", ff_dir.as_path());

    let (_, _) = setup_and_mount(args.fs, args.mount_options)?;
    Ok(())
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// filesystem to mount
    #[arg(long)]
    fs: String,
    /// filesystem to mount
    #[arg(short = 'o', long, default_value = "")]
    mount_options: String,
}
