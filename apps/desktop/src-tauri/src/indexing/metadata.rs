//! Shared metadata extraction for the indexing module.
//!
//! Consolidates all platform-specific logic for extracting file metadata
//! (sizes, mtime, inode, nlink) into a single struct and function.

/// Snapshot of filesystem metadata fields relevant to indexing.
#[derive(Debug, Clone)]
pub(super) struct MetadataSnapshot {
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
pub(super) fn extract_metadata(metadata: &std::fs::Metadata, is_dir: bool, is_symlink: bool) -> MetadataSnapshot {
    let modified_at = extract_mtime(metadata);

    if is_dir || is_symlink {
        return MetadataSnapshot {
            logical_size: None,
            physical_size: None,
            modified_at,
            inode: None,
            nlink: None,
        };
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let logical_size = metadata.len();
        let blocks = metadata.blocks();
        let physical_size = if blocks > 0 { blocks * 512 } else { 0 };
        let ino = metadata.ino();
        let nlink = metadata.nlink();
        MetadataSnapshot {
            logical_size: Some(logical_size),
            physical_size: Some(physical_size),
            modified_at,
            inode: if ino != 0 { Some(ino) } else { None },
            nlink: Some(nlink),
        }
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
