//! Delete and trash operations, both local-FS and volume-aware.
//!
//! The local-FS walker uses `walkdir` + `fs::remove_file`. The volume-aware
//! variant uses the `Volume` trait so MTP / SMB / future remote backends
//! work the same way. Trash routes to the OS-native trash (macOS
//! `trashItemAtURL`, Linux `trash` crate).
//!
//! See `CLAUDE.md` in this directory for delete walker semantics, the
//! oracle-aware fast path, trash, and the volume-delete preview-reuse path.

pub(crate) mod trash;
mod walker;

pub(in crate::file_system::write_operations) use walker::{
    delete_files_with_progress, delete_volume_files_with_progress,
};

#[cfg(test)]
mod delete_integration_test;
#[cfg(test)]
mod delete_volume_reuse_tests;
#[cfg(test)]
mod hardlink_progress_tests;
#[cfg(test)]
mod volume_cancel_tests;
