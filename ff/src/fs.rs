use anyhow::{Context, Result, anyhow};
use nix::mount::{MntFlags, MsFlags, mount, umount2};
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
    let device = std::fs::canonicalize(device).expect("this is a valid os path");
    Ok(std::fs::read_to_string("/proc/self/mounts")
        .context("unable to read /proc/self/mounts")?
        .lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let dev = parts.next().expect("/proc/self/mounts should be valid");
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
    let out = Command::new(&bin)
        .args(["-F", "-L", "ff-benchfs"])
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
    #[allow(clippy::single_match)]
    match find_first_mountpoint(&device)? {
        Some(mountpoint) => {
            println!(
                "=> {:#?} is already mounted at {:#?}. unmnounting..",
                device.as_ref(),
                &mountpoint
            );
            umount2(&mountpoint, MntFlags::MNT_FORCE)
                .context(format!("unable to unmount `{:#?}`", &mountpoint))?;
        }
        None => (),
    }

    Ok(())
}

/// Prepares `device` for testing by creating a new `filesystem`, unmounting any existing mounts
/// on `ff_dir`, and mounting the fresh filesystem at `ff_dir` with the given `mount_options`.
///
/// Intended for use in binaries to provide a ready-to-use mount point.
pub fn setup_and_mount<S: AsRef<str>>(
    filesystem: S,
    mount_options: S,
) -> Result<(PathBuf, PathBuf)> {
    let (mnt_flags, fs_data) = msflags_from_mount_opts(mount_options.as_ref())?;
    let device = std::fs::canonicalize("/dev/disk/by-partlabel/ff-bench")
        .expect("this should be a valid os path with no NUL");
    let ff_dir = create_ff_bench_dir()?;

    unmount_new(&device)?;
    mkfs(&device, &filesystem)?;
    mount_ff_bench(
        &device,
        &ff_dir,
        &filesystem,
        mnt_flags,
        &fs_data.join(",").into(),
    )?;

    Ok((device, ff_dir))
}
