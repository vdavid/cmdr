//! The entry builders the sorting suites share.
//!
//! `sorting_test`, `sorting_dir_mode_test`, and `collation_test` all need a
//! `FileEntry` with nothing interesting on it but the fields under test, and the
//! first two both build directories carrying a recursive size. Builders used by
//! ONE suite stay in that suite: the symlink in `sorting_test`, the honest-size
//! directory in `sorting_dir_mode_test`, the locale pin in `collation_test`.

use super::metadata::FileEntry;

/// Creates a test entry with the given name and properties.
pub(super) fn make_entry(name: &str, is_dir: bool, size: Option<u64>, modified: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        modified_at: modified,
        created_at: modified, // Use same value for simplicity
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("/{}", name), is_dir, false)
    }
}

/// A directory whose size key is the recursive size the drive index filled in.
///
/// Shared because both the dir-mode suite (which is about this key) and the
/// comparator-parity pair in `sorting_test` (which needs a dirs-first scenario
/// to compare the two readings over) build one.
pub(super) fn make_dir_with_recursive_size(
    name: &str,
    recursive_size: Option<u64>,
    modified: Option<u64>,
) -> FileEntry {
    let mut entry = make_entry(name, true, None, modified);
    entry.recursive_size = recursive_size;
    entry
}
