//! What this backend hands the shared tree walk: one PROPFIND for a stat, one
//! for a listing. The walk itself, the batch loop, the conflict matcher, and the
//! reasoning behind all three: `cmdr_fs::volume::scan_walk`.
//!
//! ❗ Both methods are the backend's OWN read path, ❌ never a listing-cache
//! lookup. This backend has no watcher, so its `listing_watch_coverage` is
//! `None` and a cached listing is only as fresh as the last time somebody
//! looked. SMB's scan may consult the cache because its watcher backs the claim;
//! borrowing that shortcut here is how a pre-flight conflict scan misses a file
//! and a copy overwrites it.

use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::scan_walk::{ScanSource, Walking};

use super::WebdavVolume;

impl ScanSource for WebdavVolume {
    fn scan_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(self.get_metadata_impl(path))
    }

    fn scan_list<'a>(&'a self, path: &'a Path) -> Walking<'a, Vec<FileEntry>> {
        Box::pin(self.list_directory_impl(path, None, None))
    }
}
