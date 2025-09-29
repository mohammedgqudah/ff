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
        let random: String = (0..12).map(|_| fastrand::alphanumeric()).collect();
        let filename = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("test-{}.txt", random));
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

    // write a second page
    file.write_at("I don't like writing tests".as_bytes(), 0x1000)?;
    file.sync_all()?;

    // the two pages should be cached
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("2/2 4.03KiB/4.03KiB"));

    file.evict_pages()?;

    // touch the second page, bring it into cache
    let mut buf = [0u8; 1];
    file.read_exact_at(&mut buf, 0x1000)?;

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("1/2 4KiB/4.03KiB"));

    Ok(())
}

#[test]
fn test_verbose_output_without_root() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    let mut file = TestFile::new();
    file.write_all("I like writing tests".as_bytes())?;
    file.sync_all()?;

    cmd.arg(file.path()).arg("-v");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("1/1 20B/20B"))
        .stderr(predicate::str::contains(
            "The page is present but the PFN is hidden. Run again as root",
        ));

    Ok(())
}

#[test]
#[ignore]
fn test_verbose_output_run_as_root() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    let mut file = TestFile::new();
    file.write_all("I like writing tests".as_bytes())?;
    file.sync_all()?;

    cmd.arg(file.path()).arg("-v");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("1/1 20B/20B"))
        .stdout(predicate::str::contains(
            "kflags (KPageFlags)\t  UPTODATE | LRU | MMAP | 0x800000000",
        ))
        .stdout(predicate::str::contains(
            "pagemap (PageMapEntry)\t  SOFT_DIRTY | EXCL_MAP | FILE_PAGE_OR_SHARED_ANON | PRESENT",
        ));

    Ok(())
}

#[macro_export]
macro_rules! ff_assert_cmd {
    ($stdout:expr, $expected:expr $(,)?) => {{
        use regex::Regex;
        use similar_asserts::assert_eq;

        fn normalize(s: &str) -> String {
            let hex_addr = Regex::new(r"0x[0-9a-fA-F]+").unwrap();
            let ws = Regex::new(r"[ \t]+").unwrap();

            let s = hex_addr.replace_all(s, "<ADDR>");
            let s = ws.replace_all(&s, " ");
            let s = s.into_owned();
            let s = s.replace("DIRTY | LRU | MMAP", "DIRTY | <MAYBE_LRU> | MMAP");
            let s = s.replace("DIRTY | MMAP", "DIRTY | <MAYBE_LRU> | MMAP");
            let s = s.replace("UPTODATE | MMAP", "UPTODATE | <MAYBE_LRU> | MMAP");
            let s = s.replace("UPTODATE | LRU | MMAP", "UPTODATE | <MAYBE_LRU> | MMAP");
            s
        }

        let got = normalize($stdout).trim().to_string();
        let want = normalize($expected).trim().to_string();
        assert_eq!(got, want);
    }};
}

#[test]
#[ignore]
fn test_verbose_output_dirty_pages_run_as_root() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    let mut file = TestFile::new();
    file.write_all("I like writing tests".as_bytes())?;

    let output = cmd
        .arg(file.path())
        .arg("-v")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8(output).unwrap();
    ff_assert_cmd!(
        stdout.as_str(),
        r#"
Resident Pages: 1/1 20B/20B
PAGE 0
 pagemap (PageMapEntry)  SOFT_DIRTY | EXCL_MAP | FILE_PAGE_OR_SHARED_ANON | PRESENT | <ADDR>
 kflags (KPageFlags)   UPTODATE | DIRTY | <MAYBE_LRU> | MMAP | <ADDR>
"#
    );

    Ok(())
}

#[test]
#[ignore]
fn test_verbose_output_clean_pages_run_as_root() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    let mut file = TestFile::new();
    file.write_all("I like writing tests".as_bytes())?;
    file.sync_all()?;

    let output = cmd
        .arg(file.path())
        .arg("-v")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8(output).unwrap();
    ff_assert_cmd!(
        stdout.as_str(),
        r#"
Resident Pages: 1/1 20B/20B
PAGE 0
 pagemap (PageMapEntry)  SOFT_DIRTY | EXCL_MAP | FILE_PAGE_OR_SHARED_ANON | PRESENT | <ADDR>
 kflags (KPageFlags)   UPTODATE | <MAYBE_LRU> | MMAP | <ADDR>
"#
    );

    Ok(())
}

#[test]
#[ignore]
fn test_show_dirty_pages_run_as_root() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    let mut file = TestFile::new();
    let mut buf = [0u8];
    // fill 10 pages
    file.write(&[0u8; 4096 * 10])?;
    file.sync_all()?;
    file.evict_pages()?;

    // dirty three pages
    file.write_at("dirty".as_bytes(), 0x1000)?;
    file.write_at("dirty".as_bytes(), 0x2000)?;
    file.write_at("dirty".as_bytes(), 0x4000)?;
    // cache one page
    file.read_exact_at(&mut buf, 0x9000)?;

    let output = cmd
        .arg(file.path())
        .arg("--dirty")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8(output).unwrap();
    ff_assert_cmd!(
        stdout.as_str(),
        r###"
        Resident Pages: 4/10 16KiB/40KiB
        Dirty Pages: 3/10
        1-2, 4
        "###
    );

    Ok(())
}

#[test]
#[ignore]
fn test_show_zero_dirty_pages_run_as_root() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("ff-cache")?;
    let mut file = TestFile::new();
    file.write(&[0u8; 0x1000])?;
    file.sync_all()?;

    let output = cmd
        .arg(file.path())
        .arg("--dirty")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8(output).unwrap();
    ff_assert_cmd!(
        stdout.as_str(),
        r###"
        Resident Pages: 1/1 4KiB/4KiB
        Dirty Pages: 0/1
        "###
    );

    Ok(())
}
