//! trace fsync and analyze it's behaviour.
use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use devicemapper::{DM, DevId, DmFlags, DmOptions};
use ff::{
    devicemapper::dm_table_for_bad_range,
    fs::{ff_device, setup_and_mount, unmount_new},
    pagemap::PageMapExt,
};
use fiemap::FiemapExtent;
use log::debug;
use nix::{
    errno::Errno,
    fcntl::{FallocateFlags, fallocate},
};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::FileExt,
    path::PathBuf,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// filesystem to mount
    #[arg(long)]
    fs: String,
    /// mount(8)-style options
    #[arg(short = 'o', long, default_value = "")]
    mount_options: String,
    /// file sync behaviour
    #[arg(short, long)]
    mode: Mode,
    /// the total number of pages to use for the test file
    #[arg(short, long, default_value_t = 1)]
    pages: usize,
    /// a comma separated list of ranges to fail e.g. 0,3-5
    #[arg(long)]
    fail_pages: Option<String>,
    /// whether to reopen the file before reporting results and page information
    #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
    reopen: bool,
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

/// Update the device table.
fn remap_device(dm: DM, dev_id: &DevId, table: &[(u64, u64, String, String)]) -> Result<()> {
    // load
    dm.table_load(dev_id, table, DmOptions::default())
        .context("failed to reload DM targets")?;

    // suspend
    dm.device_suspend(
        dev_id,
        DmOptions::default().set_flags(DmFlags::DM_SUSPEND | DmFlags::DM_NOFLUSH),
    )
    .context("failed to suspend DM device")?;

    // resume the device
    dm.device_suspend(dev_id, DmOptions::default().set_flags(DmFlags::DM_NOFLUSH))
        .context("failed to resume DM device")?;

    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let total_blocks = 114294784 / 512;
    let table = dm_table_for_bad_range(ff_device(), total_blocks, None);
    let device_name = devicemapper::DmName::new("ff-bench-device").expect("is a valid device name");
    let path = PathBuf::from(format!("/dev/mapper/{device_name}"));
    let dm = devicemapper::DM::new().context("failed to open DM_CTL")?;
    let dev_id = DevId::Name(device_name);

    // unmount the backing device
    unmount_new(ff_device())?;
    // unmount and delete old DM device
    let _ = unmount_new(&path);
    let _ = dm.device_remove(&dev_id, DmOptions::default());

    dm.device_create(device_name, None, DmOptions::default())
        .context("failed to create DM device")?;
    dm.table_load(&dev_id, table.as_slice(), DmOptions::default())
        .context("failed to load DM targets")?;
    // resume the device (DmFlags::DM_SUSPECT is not set)
    dm.device_suspend(&dev_id, DmOptions::default())
        .context("failed to resume DM device")?;

    let (_, ff_dir) = setup_and_mount(Some(path), args.fs, args.mount_options)?;

    let filepath = ff_dir.join("test.txt");

    let mut binding = OpenOptions::new();
    let file_open_options = binding.write(true).read(true).create(true).truncate(true);
    let mut file = file_open_options.open(&filepath)?;

    let fs_block_size = file.fs_block_size()?;

    // allocate blocks on disk for this file so we don't
    // deal with delayed allocation.
    match fallocate(
        &file,
        FallocateFlags::empty(),
        0,
        (args.pages * fs_block_size) as i64,
    ) {
        Err(Errno::EOPNOTSUPP) => {
            println!("=> fallocate is not supported on this filesystem");
            println!("=> ignoring.");
            Ok(())
        }
        Ok(_) => {
            debug!("allocated {} page(s)", args.pages);
            Ok(())
        }
        Err(e) => Err(e).context(format!("failed to fallocate {} pages", args.pages)),
    }?;

    let extent = dbg_extent(&file)?;

    let mut buf = vec![0u8; args.pages * fs_block_size];
    buf.fill(120);

    file.write_at("HELLO!!!".as_bytes(), 0x1000)?;
    file.sync_all()?;

    let write_result = file.write(buf.as_slice());
    if write_result.is_err() {
        let message = format!("writing {} page(s) failed", args.pages);
        println!("{}", message.red());
    } else {
        let message = format!("writing {} page(s) succeeded", args.pages);
        println!("{}", message.green());
    }

    let start_blk = extent.fe_physical / 512;

    let table = dm_table_for_bad_range(
        ff_device(),
        total_blocks,
        #[allow(clippy::single_range_in_vec_init)]
        Some(&[start_blk..start_blk + 1]), // fail the first block of the file
    );

    remap_device(dm, &dev_id, table.as_slice())?;

    let sync_result = match args.mode {
        Mode::FSync => file.sync_all(),
        Mode::FDataSync => file.sync_data(),
        Mode::NoSync | Mode::OpenSync | Mode::OpenDataSync => Ok(()),
    };
    match args.mode {
        Mode::FSync | Mode::FDataSync => {
            if sync_result.is_err() {
                println!("{}", "sync failed".red());
            } else {
                println!("{}", "sync succeeded".green());
            }
        }
        _ => (),
    };

    let file = if args.reopen {
        println!("=> closing old file");
        drop(file);
        OpenOptions::new()
            .write(true)
            .read(true)
            .open(&filepath)
            .context("failed to re-open the file after it was closed")?
    } else {
        file
    };

    for i in 0..args.pages {
        println!("{} {}", "PAGE".bold(), i.to_string().cyan());
        let (pagemap, kflags) = file.page_info(i)?;

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

    Ok(())
}

fn dbg_extent(file: &File) -> Result<fiemap::FiemapExtent> {
    let fs_block_size = file.fs_block_size()?;

    // TODO: filesystem may not support fiemap
    let map = fiemap::Fiemap::new(&file);
    let col: Vec<std::io::Result<FiemapExtent>> = map.into_iter().collect();
    debug!("{:#?}", &col);
    let extent = col.first().unwrap().as_ref().unwrap();

    debug!(
        " {}\t {}",
        "physical block number".dimmed(),
        extent.fe_physical / 512
    );
    debug!(
        " {}\t {}",
        "blocks in extent".dimmed(),
        extent.fe_length / 512
    );
    debug!(
        " {}\t {}",
        "fs pages in extent".dimmed(),
        extent.fe_length / fs_block_size as u64
    );

    Ok(*extent)
}
