//! Shared fixtures for the same-volume rename-merge suites: an unthrottled
//! operation state, a `LocalPosixVolume` over a fresh tempdir, and the four
//! path helpers each test builds and inspects its fixture tree with.
//!
//! Reached by `rename_merge_tests.rs` (merge semantics),
//! `rename_merge_cancel_tests.rs`, `rename_merge_case_fold_tests.rs`,
//! `rename_merge_walk_tests.rs`, and `rename_merge_stat_tests.rs`. Each of those
//! brings its own `Volume` wrapper where it needs one; only the plain
//! `LocalPosixVolume` lives here.
//!
//! ## Why `LocalPosixVolume` over a tempdir, not `InMemoryVolume`
//!
//! The rename-merge depends on two real backend semantics that `InMemoryVolume`
//! does NOT model: `rename` of a directory moves its WHOLE subtree in one call,
//! and `delete` of a non-empty directory FAILS (empty-only). `InMemoryVolume`'s
//! `rename` moves only the single keyed entry (orphaning children) and its
//! `delete` removes any entry unconditionally. `LocalPosixVolume` over a tempdir
//! gives the real POSIX semantics the rename-merge is built on, on both Linux
//! (CI) and macOS. The case-fold suite wraps it in a case-insensitive volume so
//! it's portable regardless of the host filesystem's case sensitivity.

use crate::file_system::volume::{LocalPosixVolume, Volume};
use crate::file_system::write_operations::state::WriteOperationState;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

pub(super) fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

/// A `LocalPosixVolume` rooted at a fresh tempdir. The `TempDir` is returned so
/// the caller keeps it alive for the test's duration.
pub(super) fn local_volume() -> (Arc<dyn Volume>, TempDir) {
    let dir = TempDir::new().unwrap();
    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("V", dir.path().to_path_buf()));
    (vol, dir)
}

/// Writes a file at a volume-relative path, creating parents on disk.
pub(super) fn write_file(root: &Path, rel: &str, content: &[u8]) {
    let abs = root.join(rel);
    std::fs::create_dir_all(abs.parent().expect("path has a parent")).expect("create parents");
    std::fs::write(abs, content).expect("write file");
}

pub(super) fn mkdir(root: &Path, rel: &str) {
    std::fs::create_dir_all(root.join(rel)).expect("create dir");
}

pub(super) fn read(root: &Path, rel: &str) -> Vec<u8> {
    std::fs::read(root.join(rel)).expect("read file")
}

pub(super) fn exists(root: &Path, rel: &str) -> bool {
    root.join(rel).exists()
}
