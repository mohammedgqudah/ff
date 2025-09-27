use assert_cmd::prelude::*;
use ff::pagemap::PageMapExt;
use predicates::prelude::*;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    ops::{Deref, DerefMut},
    os::unix::fs::FileExt,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct TestFile(std::path::PathBuf, File);

impl TestFile {
    /// Create a temp file in the target directory that will automatically get deleted
    /// on drop.
    ///
    /// The reason for using this instead of using a crate like `tempfile` or directly creating a
    /// file in `/tmp` is that it does not make sense to write tests against `tmpfs`.
    pub fn new() -> Self {
        // generate a random filename
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let filename = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("test-{}.txt", nanos));
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&filename)
            .unwrap();

        Self(filename.into(), file)
    }

    pub fn path(&self) -> &Path {
        return self.0.as_path();
    }
}

impl Deref for TestFile {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        return &self.1;
    }
}

impl DerefMut for TestFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.1
    }
}

impl Drop for TestFile {
    // cleanup
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.as_path());
    }
}

#[test]
fn test_non_existent_file() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    cmd.arg("foobar");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("failed to open foobar"));

    Ok(())
}

#[test]
fn test_empty_file() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    let file = TestFile::new();
    cmd.arg(file.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0/0 0B/0B"));

    drop(file);
    Ok(())
}

#[test]
fn test_non_cached_file() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    let mut file = TestFile::new();
    file.write_all("I like writing tests".as_bytes())?;
    file.sync_all()?;
    file.evict_pages()?;

    cmd.arg(file.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0/1 0B/20B"));

    // write a second page
    file.write_at("I don't like writing tests".as_bytes(), 0x1000)?;
    file.sync_all()?;
    file.evict_pages()?;

    let mut cmd = Command::cargo_bin("ff-cache")?;
    cmd.arg(file.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0/2 0B/4.03KiB"));

    Ok(())
}

#[test]
fn test_cached_file() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    let mut file = TestFile::new();
    file.write_all("I like writing tests".as_bytes())?;
    file.sync_all()?;

    cmd.arg(file.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("1/1 20B/20B"));

    Ok(())
}
