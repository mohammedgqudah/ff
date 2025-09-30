use anyhow::{Result, anyhow};
use nix::mount::MsFlags;

use crate::KernelVersion;

/// Parse a comma-separated `mount(8)` option string into MsFlags, unknown options
/// are treated as filesystem specific options - as passed to `data` in mount(2) - and are returned
/// as vec.
pub fn msflags_from_mount_opts(opts: &str) -> Result<(MsFlags, Vec<String>)> {
    let mut flags = MsFlags::empty();
    // filesystem specific key=value options passed to
    let mut fs_data: Vec<String> = Vec::new();

    if opts.is_empty() {
        return Ok((flags, fs_data));
    }

    for raw in opts.split(',') {
        let opt = raw.trim().to_lowercase();
        if opt.is_empty() {
            return Err(anyhow!("an empty mount option was passed"));
        }

        // key-value pairs are filesystem specific options
        if opt.contains('=') {
            fs_data.push(opt);
            continue;
        }

        match opt.as_str() {
            // Access mode
            "ro" => flags.set(MsFlags::MS_RDONLY, true),
            "rw" => flags.set(MsFlags::MS_RDONLY, false),

            // suid/sgid handling
            "nosuid" => flags.set(MsFlags::MS_NOSUID, true),
            "suid" => flags.set(MsFlags::MS_NOSUID, false),

            // device nodes
            "nodev" => flags.set(MsFlags::MS_NODEV, true),
            "dev" => flags.set(MsFlags::MS_NODEV, false),

            // exec
            "noexec" => flags.set(MsFlags::MS_NOEXEC, true),
            "exec" => flags.set(MsFlags::MS_NOEXEC, false),

            // sync/async
            "sync" => flags.set(MsFlags::MS_SYNCHRONOUS, true),
            "async" => flags.set(MsFlags::MS_SYNCHRONOUS, false),

            // remount/move/bind
            "remount" => flags.set(MsFlags::MS_REMOUNT, true),
            "move" => flags.set(MsFlags::MS_MOVE, true),
            "bind" => flags.set(MsFlags::MS_BIND, true),
            "rbind" => {
                flags.set(MsFlags::MS_BIND, true);
                flags.set(MsFlags::MS_REC, true);
            }

            // dirsync
            "dirsync" => flags.set(MsFlags::MS_DIRSYNC, true),

            // atime family
            "noatime" => flags.set(MsFlags::MS_NOATIME, true),
            "atime" => flags.set(MsFlags::MS_NOATIME, false),

            "nodiratime" => flags.set(MsFlags::MS_NODIRATIME, true),
            "diratime" => flags.set(MsFlags::MS_NODIRATIME, false),

            "relatime" => flags.set(MsFlags::MS_RELATIME, true),
            "norelatime" => flags.set(MsFlags::MS_RELATIME, false),

            "strictatime" => flags.set(MsFlags::MS_STRICTATIME, true),
            "nostrictatime" => flags.set(MsFlags::MS_STRICTATIME, false),

            "lazytime" => flags.set(MsFlags::MS_LAZYTIME, true),
            "nolazytime" => flags.set(MsFlags::MS_LAZYTIME, false),

            // mandatory locking
            "mand" => {
                // mandatory locking was removed in 5.15
                // linux commit: f7e33bdbd6d1bdf9c3df8bba5abcf3399f957ac3
                if KernelVersion::current().at_least(5, 15) {
                    return Err(anyhow!(
                        "mount option `mand` was removed starting with linux 5.15"
                    ));
                }
                flags.set(MsFlags::MS_MANDLOCK, true)
            }
            "nomand" => flags.set(MsFlags::MS_MANDLOCK, false),

            "acl" => flags.set(MsFlags::MS_POSIXACL, true),
            "noacl" => flags.set(MsFlags::MS_POSIXACL, false),

            "silent" => flags.set(MsFlags::MS_SILENT, true),

            "iversion" => flags.set(MsFlags::MS_I_VERSION, true),
            "noiversion" => flags.set(MsFlags::MS_I_VERSION, false),

            "slave" => flags.set(MsFlags::MS_SLAVE, true),
            "rslave" => {
                flags.set(MsFlags::MS_SLAVE, true);
                flags.set(MsFlags::MS_REC, true);
            }

            "unbindable" => flags.set(MsFlags::MS_UNBINDABLE, true),
            "runbindable" => {
                flags.set(MsFlags::MS_UNBINDABLE, true);
                flags.set(MsFlags::MS_REC, true);
            }

            // Expand "defaults" to the typical util-linux defaults:
            // rw,suid,dev,exec,relatime,async
            "defaults" => {
                flags.set(MsFlags::MS_RDONLY, false);
                flags.set(MsFlags::MS_NOSUID, false);
                flags.set(MsFlags::MS_NODEV, false);
                flags.set(MsFlags::MS_NOEXEC, false);
                flags.set(MsFlags::MS_SYNCHRONOUS, false);
                flags.set(MsFlags::MS_RELATIME, true);
            }

            _ => {
                fs_data.push(opt);
            }
        }
    }

    Ok((flags, fs_data))
}

#[cfg(test)]
mod test {
    use nix::mount::MsFlags;

    use super::msflags_from_mount_opts;

    #[test]
    pub fn test_parse_msflags() {
        assert_eq!(
            msflags_from_mount_opts("").unwrap(),
            (MsFlags::empty(), vec![])
        );
        assert_eq!(
            msflags_from_mount_opts("sync,silent").unwrap(),
            (
                MsFlags::empty() | MsFlags::MS_SILENT | MsFlags::MS_SYNCHRONOUS,
                vec![]
            )
        );
        assert_eq!(
            msflags_from_mount_opts("sync,silent,data=journal").unwrap(),
            (
                MsFlags::empty() | MsFlags::MS_SILENT | MsFlags::MS_SYNCHRONOUS,
                vec!["data=journal".into()]
            )
        );
        assert_eq!(
            msflags_from_mount_opts("sync,silent,data=journal,bigalloc").unwrap(),
            (
                MsFlags::empty() | MsFlags::MS_SILENT | MsFlags::MS_SYNCHRONOUS,
                vec!["data=journal".into(), "bigalloc".into()]
            )
        );
    }
}
