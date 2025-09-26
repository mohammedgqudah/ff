use anyhow::{Context, Result, bail};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use tar::Archive;
use xz2::read::XzDecoder;

/// TODO: use host's arch
pub fn find_cached_kernel() -> Result<Option<PathBuf>> {
    let path = ff_cache()?.join("linux-6.16.5/arch/x86_64/boot/bzImage");

    if !path.exists() {
        Ok(None)
    } else {
        Ok(Some(path))
    }
}

/// ensure ff-cache exists and return its path.
pub fn ff_cache() -> Result<PathBuf> {
    let cache = PathBuf::from(std::env::var("HOME").context("reading $HOME")?).join(".cache/ff");
    fs::create_dir_all(&cache).context("creating ff-cache")?;
    Ok(cache)
}

/// download the file to ff's cache and return its path.
pub fn download(url: &str, filename: &str, use_cache: bool) -> Result<PathBuf> {
    let file_path = ff_cache()?.join(filename);
    if use_cache && file_path.exists() {
        println!("=> found cached version at {}", file_path.display());
        return Ok(file_path);
    }
    let response = reqwest::blocking::Client::new()
        .get(url)
        .send()
        .context(format!("sending GET request to {}", url))?
        .error_for_status()
        .context(format!("non-success status from {}", url))?;
    let total_size = response.content_length().expect("should have a length");

    let style = ProgressStyle::with_template(
        "{msg:.dim} {bar:30.green/dim} {binary_bytes:>7}/{binary_total_bytes:7}",
    )
    .expect("this should be a valid template")
    .progress_chars("--");

    let pb = ProgressBar::new(total_size);
    pb.set_style(style);
    pb.set_message(filename.to_string());

    let mut dest = File::create(&file_path).context(format!("creating {}", filename))?;
    let mut source = pb.wrap_read(response);
    io::copy(&mut source, &mut dest).context(format!("writing {}", filename))?;

    pb.finish();

    Ok(file_path)
}

pub fn decompress_tar_xz<P: AsRef<Path>, Q: AsRef<Path>>(
    tar_xz_path: P,
    dest_dir: Q,
) -> Result<()> {
    let tar_xz_path = tar_xz_path.as_ref();
    let dest_dir = dest_dir.as_ref();

    fs::create_dir_all(dest_dir).context(format!(
        "creating destination directory {}",
        dest_dir.display()
    ))?;

    let file = File::open(tar_xz_path).context(format!("opening {}", tar_xz_path.display()))?;

    let mp = MultiProgress::new();

    let pb_entry = mp.add(ProgressBar::new_spinner());
    pb_entry.set_style(ProgressStyle::with_template("{spinner:.dim} {msg:.dim}")?);
    pb_entry.enable_steady_tick(Duration::from_millis(100));

    // stream-decompress and extract
    let reader = BufReader::new(file);
    let reader = pb_entry.wrap_read(reader);
    let decoder = XzDecoder::new(reader);
    let mut archive = Archive::new(decoder);

    for entry_res in archive.entries().context("reading .tar entries")? {
        let mut entry = entry_res.context("reading a .tar entry")?;
        if let Ok(path) = entry.path() {
            pb_entry.set_message(path.display().to_string());
        }
        entry.unpack_in(dest_dir).context("extracting entry")?;
    }

    pb_entry.finish_and_clear();

    Ok(())
}

fn run_make_in<P: AsRef<Path>>(workdir: P, args: &[&str]) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.dim} {msg:.dim}")?);
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_message(format!("make {}", args.join(" ")));

    let mut child = Command::new("make")
        .args(args)
        .current_dir(workdir.as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning `make`")?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // stream stdout
    let pb_out = pb.clone();
    let t_out = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            pb_out.set_message(line.chars().take(80).collect::<String>());
        }
    });

    // stream stderr
    let pb_err = pb.clone();
    let t_err = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().flatten() {
            eprintln!("{line}");
            pb_err.set_message(line.chars().take(80).collect::<String>());
        }
    });

    let status = child.wait().context("waiting for `make` to finish")?;
    let _ = t_out.join();
    let _ = t_err.join();

    if status.success() {
        pb.finish_with_message("make finished successfully");
        Ok(())
    } else {
        pb.finish();
        bail!("make exited with status {}", status);
    }
}

/// Run `make defconfig` inside the kernel source tree.
pub fn make_defconfig<P: AsRef<Path>>(src_dir: P) -> Result<()> {
    let mut argv: Vec<String> = Vec::new();
    argv.push("defconfig".into());

    let args: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    run_make_in(src_dir, &args)
}

/// Run `make -j5`
pub fn make_build<P: AsRef<Path>>(src_dir: P) -> Result<()> {
    let argv: Vec<String> = vec![format!("-j9")];

    let args: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    run_make_in(src_dir, &args)
}

pub fn kernel_default() -> Result<PathBuf> {
    const URL: &str = "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.16.5.tar.xz";

    match find_cached_kernel()? {
        Some(bz_image) => {
            println!("=> cached kernel found: {}", bz_image.display());
        }
        None => {
            let tar_xz = download(URL, "linux-6.16.5.tar.xz", true)?;
            let kernel_source = ff_cache()?.join("linux-6.16.5");
            println!("=> extracting");
            decompress_tar_xz(tar_xz, ff_cache()?)?;
            println!("=> make defconfig");
            make_defconfig(&kernel_source)?;
            println!("=> compiling");
            make_build(&kernel_source)?;
        }
    };

    Ok(ff_cache()?.join("linux-6.16.5/arch/x86_64/boot/bzImage"))
}
