//! Both Unixes: `st_blocks * 512` and `st_mtime`.

use std::fs::Metadata;
use std::os::unix::fs::MetadataExt as _;

pub(crate) fn allocated_bytes(metadata: &Metadata) -> Option<u64> {
    let blocks = metadata.blocks();
    // Saturating: release builds don't check overflow, so a bogus block
    // count from a network filesystem would wrap into a small, wrong size.
    Some(if blocks > 0 { blocks.saturating_mul(512) } else { 0 })
}

pub(crate) fn modified_secs(metadata: &Metadata) -> Option<u64> {
    let mtime = metadata.mtime();
    if mtime >= 0 { Some(mtime as u64) } else { None }
}
