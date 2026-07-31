//! Shared metadata extraction for the indexing module.
//!
//! Consolidates all platform-specific logic for extracting file metadata
//! (sizes, mtime, inode, nlink) into a single struct and function.

/// Snapshot of filesystem metadata fields relevant to indexing.
#[derive(Debug, Clone)]
pub(crate) struct MetadataSnapshot {
    pub logical_size: Option<u64>,
    pub physical_size: Option<u64>,
    pub modified_at: Option<u64>,
    pub inode: Option<u64>,
    pub nlink: Option<u64>,
}

/// Extract indexing-relevant fields from `std::fs::Metadata`.
///
/// For directories and symlinks, sizes are `None` (they don't contribute
/// to disk usage accounting). For regular files, `logical_size` is
/// `meta.len()` and `physical_size` is `st_blocks * 512` on Unix.
///
/// Directories carry their inode (used for inode-based rename detection in
/// the live event loop on filesystems where the kernel preserves directory
/// inodes across rename (APFS/HFS+/ext4/btrfs/XFS/NTFS). Symlinks still get
/// `inode: None` since they aren't addressed by inode anywhere today.
pub(super) fn extract_metadata(metadata: &std::fs::Metadata, is_dir: bool, is_symlink: bool) -> MetadataSnapshot {
    let modified_at = extract_mtime(metadata);

    if is_symlink {
        return MetadataSnapshot {
            logical_size: None,
            physical_size: None,
            modified_at,
            inode: None,
            nlink: None,
        };
    }

    if is_dir {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let ino = metadata.ino();
            return MetadataSnapshot {
                logical_size: None,
                physical_size: None,
                modified_at,
                inode: if ino != 0 { Some(ino) } else { None },
                nlink: None,
            };
        }
        #[cfg(not(unix))]
        {
            return MetadataSnapshot {
                logical_size: None,
                physical_size: None,
                modified_at,
                inode: None,
                nlink: None,
            };
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let blocks = metadata.blocks();
        let physical_size = if blocks > 0 { blocks * 512 } else { 0 };
        metadata_from_raw(
            metadata.len(),
            physical_size,
            modified_at,
            metadata.ino(),
            metadata.nlink(),
            is_dir,
            is_symlink,
        )
    }

    #[cfg(not(unix))]
    {
        let size = metadata.len();
        MetadataSnapshot {
            logical_size: Some(size),
            physical_size: Some(size),
            modified_at,
            inode: None,
            nlink: None,
        }
    }
}

/// Build a [`MetadataSnapshot`] from already-extracted raw filesystem values,
/// applying the SAME directory/symlink size rules as [`extract_metadata`]. This is
/// the single source of those rules: `extract_metadata` (from a `std::fs::Metadata`)
/// and the macOS `getattrlistbulk` bulk reader (from packed attribute bytes) both
/// funnel through it, so a bulk-read entry and a `symlink_metadata` entry can't
/// diverge on how sizes/inode/nlink are mapped. `physical_size` is bytes (the caller
/// has already applied `st_blocks * 512` or read `ATTR_FILE_ALLOCSIZE`).
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS bulk reader calls this directly today")
)]
pub(super) fn metadata_from_raw(
    logical_size: u64,
    physical_size: u64,
    modified_at: Option<u64>,
    ino: u64,
    nlink: u64,
    is_dir: bool,
    is_symlink: bool,
) -> MetadataSnapshot {
    if is_symlink {
        return MetadataSnapshot {
            logical_size: None,
            physical_size: None,
            modified_at,
            inode: None,
            nlink: None,
        };
    }
    if is_dir {
        return MetadataSnapshot {
            logical_size: None,
            physical_size: None,
            modified_at,
            inode: if ino != 0 { Some(ino) } else { None },
            nlink: None,
        };
    }
    MetadataSnapshot {
        logical_size: Some(logical_size),
        physical_size: Some(physical_size),
        modified_at,
        inode: if ino != 0 { Some(ino) } else { None },
        nlink: Some(nlink),
    }
}

/// Extract mtime from metadata, platform-independently.
fn extract_mtime(metadata: &std::fs::Metadata) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mtime = metadata.mtime();
        if mtime >= 0 { Some(mtime as u64) } else { None }
    }

    #[cfg(not(unix))]
    {
        metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
    }
}
