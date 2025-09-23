use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::mount::msflags_from_mount_opts;

/// Check if a mountpoint exists.
pub fn mountpoint_exists<P: AsRef<Path>>(path: P) -> Result<bool> {
    let path = std::fs::canonicalize(path).expect("this is a valid os path");
    Ok(std::fs::read_to_string("/proc/self/mounts")
        .context("unable to read /proc/self/mounts")?
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .any(|p| p == path.as_os_str()))
}

/// Return the first mountpoint for a device.
pub fn find_first_mountpoint<P: AsRef<Path>>(device: P) -> Result<Option<PathBuf>> {
    // use the canonical path. dm devices are passed as a symlink from /dev/mapper
    let device = std::fs::canonicalize(&device).context(format!(
        "{} is not a valid os path",
        device.as_ref().display()
    ))?;
    Ok(std::fs::read_to_string("/proc/self/mounts")
        .context("unable to read /proc/self/mounts")?
        .lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let dev = parts.next().expect("/proc/self/mounts should be valid");
            // use the canonical path to match `device`
            let Ok(dev) = std::fs::canonicalize(dev) else {
                return None;
            };
            let mountpoint = parts.next().expect("/proc/self/mounts should be valid");
            if dev == device.as_os_str() {
                Some(mountpoint.into())
            } else {
                None
            }
        })
        .next())
}

/// Forcibly make a filesystem.
/// TODO: accept mkfs options.
pub fn mkfs<P: AsRef<Path>, S: AsRef<str>>(dev: P, filesystem: S) -> Result<()> {
    if !dev.as_ref().exists() {
        return Err(anyhow!("device `{:#?}` does not exist", dev.as_ref()));
    }

    // TODO: this is assuming all filesystems follow this pattern, it's better
    // if I accept a `--mkfs {}` option instead.
    let bin = format!("mkfs.{}", filesystem.as_ref());
    let mut cmd = Command::new(&bin);
    // TODO: don't commit: this is a workaround, this fn should be fs aware, and not blindly pass
    // arguments.
    if bin == "mkfs.btrfs" {
        cmd.args(["-f", "-L", "ff-benchfs"]);
    } else {
        cmd.args(["-F", "-L", "ff-benchfs"]);
    };
    let out = cmd
        .arg(dev.as_ref())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context(format!("failed to run {bin}"))?;

    if !out.status.success() {
        return Err(anyhow!(
            "{bin} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    Ok(())
}

pub fn mount_ff_bench<P: AsRef<Path>, S: AsRef<str>>(
    dev: P,
    ff_dir: P,
    filesystem: S,
    flags: MsFlags,
    fs_data: P,
) -> Result<()> {
    mount(
        Some(dev.as_ref()),
        std::path::absolute(&ff_dir).unwrap().as_os_str(),
        Some(filesystem.as_ref()),
        flags,
        Some(fs_data.as_ref()),
    )
    .context(format!(
        "unable to mount `{:#?}` at `{:#?}`",
        dev.as_ref(),
        ff_dir.as_ref()
    ))?;

    Ok(())
}

pub fn create_ff_bench_dir() -> Result<PathBuf> {
    let ff_dir = std::path::Path::new(".ff-bench");
    std::fs::create_dir_all(ff_dir).context("unable to create `.ff-bench`")?;

    Ok(ff_dir.into())
}

/// Try to unmount `dir`.
pub fn unmount<P: AsRef<Path>>(dir: P) -> Result<()> {
    if mountpoint_exists(&dir)? {
        println!(
            "=> `{}` is already a mount point, unmounting",
            dir.as_ref().to_string_lossy()
        );
        umount2(dir.as_ref(), MntFlags::MNT_FORCE)
            .context(format!("unable to unmount `{:#?}`", dir.as_ref()))?;
    }

    Ok(())
}

pub fn unmount_new<P: AsRef<Path>>(device: P) -> Result<()> {
    if !device.as_ref().exists() {
        return Ok(());
    }

    #[allow(clippy::single_match)]
    match find_first_mountpoint(&device)? {
        Some(mountpoint) => {
            println!(
                "=> {} is already mounted at {}. unmnounting..",
                device.as_ref().to_string_lossy().dimmed(),
                &mountpoint.to_string_lossy().dimmed()
            );
            umount2(&mountpoint, MntFlags::MNT_FORCE)
                .context(format!("unable to unmount `{:#?}`", &mountpoint))?;
        }
        None => (),
    }

    Ok(())
}

/// Prepares `device` for testing by creating a new `filesystem`, unmounting `device`,
/// and mounting the fresh filesystem at `ff_dir` with the given `mount_options`.
///
/// Intended for use in binaries to provide a ready-to-use mount point.
///
/// # Examples
///
/// ```no_run
/// use ff::fs::setup_and_mount;
///
/// setup_and_mount(Some("/dev/bench-device"), "ext4", "sync,nodelalloc");
/// ```
pub fn setup_and_mount<S: AsRef<str>, P: Into<PathBuf>>(
    device: Option<P>,
    filesystem: S,
    mount_options: S,
) -> Result<(PathBuf, PathBuf)> {
    let (mnt_flags, fs_data) = msflags_from_mount_opts(mount_options.as_ref())?;
    let device: PathBuf = device
        .map(Into::into)
        .unwrap_or_else(|| ff_device());

    let device = std::fs::canonicalize(&device)
        .context(format!("{} is not a valid os path", device.display()))?;
    let ff_dir = create_ff_bench_dir()?;

    unmount_new(&device)?;
    println!("making filesystem");
    mkfs(&device, &filesystem)?;
    println!("mounting");
    mount_ff_bench(
        &device,
        &ff_dir,
        &filesystem,
        mnt_flags,
        &fs_data.join(",").into(),
    )?;

    Ok((device, ff_dir))
}

pub fn ff_device() -> PathBuf {
    if let Ok(path) = fs::canonicalize("/dev/disk/by-partlabel/ff-bench") {
        path
    } else {
        eprintln!("Error: /dev/disk/by-partlabel/ff-bench is not a valid path");
        eprintln!("unable to find ff-device");
        eprintln!();
        eprintln!("follow the instructions on how to create a parition for ff");
        std::process::exit(1);
    }
}
