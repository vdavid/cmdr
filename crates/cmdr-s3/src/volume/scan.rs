//! What this backend hands the shared tree walk a copy's scan runs on: one
//! HEAD (or folder probe) for the top, one listing per folder. The walk, the
//! batch loop, and the reasoning behind both: `cmdr_fs::volume::scan_walk`.
//!
//! ❗ Both methods are the backend's OWN read path, ❌ never a listing-cache
//! lookup: there's no watcher, so a cached listing is only as fresh as the last
//! time somebody looked.

use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::scan_walk::{ScanSource, Walking};

use super::S3Volume;

impl ScanSource for S3Volume {
    fn scan_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(self.get_metadata_impl(path))
    }

    fn scan_list<'a>(&'a self, path: &'a Path) -> Walking<'a, Vec<FileEntry>> {
        Box::pin(self.list_directory_impl(path, None, None))
    }

    /// Every request is billed, so the dialog prices the operation from the
    /// scan's own sizes and upload dates (`crate::cost`), never from new
    /// requests. A listing's date is `LastModified`, the upload time, which is
    /// what a minimum storage duration counts from.
    fn keeps_files(&self) -> bool {
        true
    }
}
