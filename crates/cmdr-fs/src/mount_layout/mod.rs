//! Where this OS mounts the volumes a person plugs in or connects, as path prefixes: pure
//! string knowledge, no syscall, so the index's hot paths and the importance classifier can ask
//! it. The mount TABLE (what's mounted right now) is `cmdr_local::mounts`, which re-exports this.

mod platform;

/// The prefixes under which one more path component names a mounted volume: `/Volumes/` on
/// macOS, `/mnt/` and `/media/` on Linux, none yet on Windows (a drive letter is its own root).
/// `/Volumes/backup` is a volume root, `/Volumes/backup/photos` a folder inside it.
pub fn user_mount_prefixes() -> &'static [&'static str] {
    platform::user_mount_prefixes()
}
