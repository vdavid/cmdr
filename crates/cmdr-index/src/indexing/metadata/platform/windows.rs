//! Windows: std's metadata carries no allocated size (that's `GetFileInformationByHandleEx`
//! with `FileStandardInfo`), so the logical size stands in; the modified time is std's.

use std::fs::Metadata;

pub(crate) fn allocated_bytes(_metadata: &Metadata) -> Option<u64> {
    None
}

pub(crate) fn modified_secs(metadata: &Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}
