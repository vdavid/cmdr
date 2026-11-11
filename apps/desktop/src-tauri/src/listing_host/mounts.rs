//! The kernel's mount table, answering what a stalled folder is mounted on.

use std::path::Path;

use cmdr_listing::host::{MountFacts, MountTable};

/// Reads the OS mount table (`getfsstat` on macOS, `/proc/mounts` on Linux), lexically, by path
/// prefix. ❌ Never a `statfs` on the path, which blocks on the very mount that
/// just stalled.
pub(crate) struct AppMountTable;

impl MountTable for AppMountTable {
    fn mount_of(&self, path: &Path) -> Option<MountFacts> {
        crate::volumes::mount_type_and_source_for(path).map(|(fs_type, source)| MountFacts { fs_type, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real table answers for a temp folder, and with a local disk: a dev
    /// Mac's APFS, a Linux box's ext4 or tmpfs. What the engine makes of that
    /// answer is its own cell (`stalled_on.rs`).
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_temp_folder_reads_as_a_drive() {
        let dir = tempfile::tempdir().unwrap();
        let resolved = std::fs::canonicalize(dir.path()).unwrap();
        let mount = AppMountTable
            .mount_of(&resolved)
            .expect("a temp folder is on some mount");
        assert!(
            cmdr_fs::filesystem_kind::mount_is_local_disk(Some(&mount.source), Some(&mount.fs_type)),
            "a temp folder sits on a local disk; got {} from {}",
            mount.fs_type,
            mount.source
        );
    }
}
