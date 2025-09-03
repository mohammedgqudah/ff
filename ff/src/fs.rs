use anyhow::{Context, Result, anyhow};
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Check if a mountpoint exists.
pub fn mount_exists<P: AsRef<Path>>(path: P) -> Result<bool> {
    let path = std::fs::canonicalize(path).expect("this is a valid os path");
    Ok(std::fs::read_to_string("/proc/self/mounts")
        .context("unable to read /proc/self/mounts")?
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .any(|p| p == path.as_os_str()))
}

/// Forcibly make a filesystem.
pub fn mkfs<P: AsRef<Path>>(dev: P) -> Result<()> {
    if !dev.as_ref().exists() {
        return Err(anyhow!("device `{:#?}` does not exist", dev.as_ref()));
    }

    let out = Command::new("mkfs.ext4")
        .args(["-F", "-L", "ff-benchfs"])
        .arg(dev.as_ref())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run mkfs.ext4")?;

    if !out.status.success() {
        eprintln!("mkfs.ext4 failed: {}", String::from_utf8_lossy(&out.stderr));
        std::process::exit(1);
    }

    Ok(())
}

pub fn mount_ff_bench<P: AsRef<Path>>(dev: P, ff_dir: P, flags: MsFlags) -> Result<()> {
    mount(
        Some(dev.as_ref()),
        std::path::absolute(&ff_dir).unwrap().as_os_str(),
        Some("ext4"),
        flags,
        None::<&Path>,
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
    if mount_exists(&dir)? {
        println!("=> device is already mounted, unmounting");
        umount2(dir.as_ref(), MntFlags::MNT_FORCE)
            .context(format!("unable to unmount `{:#?}`", dir.as_ref()))?;
    }

    Ok(())
}
