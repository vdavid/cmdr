//! What this backend hands the shared tree walk: one `STAT` for a stat, one
//! `LIST` for a listing. The walk itself, the batch loop, the conflict matcher,
//! and the reasoning behind all three: `cmdr_fs::volume::scan_walk`.
//!
//! ❗ Both methods are the backend's OWN read path, ❌ never a listing-cache
//! lookup. There is no watcher here, so `listing_watch_coverage` is `None` and a
//! cached listing is only as fresh as the last time somebody looked. SMB's scan
//! may consult its cache because a watcher backs the claim; borrowing that
//! shortcut here is how a pre-flight conflict scan misses a file and a copy
//! overwrites it.
//!
//! ❗ The walk counts a symlinked directory as the one entry it is rather than
//! walking it, which this backend needs more than most: `/sdcard` is a symlink
//! to `/storage/emulated/0` on every modern Android, so following links would
//! count the same bytes twice.

use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::scan_walk::{ScanSource, Walking};

use super::AdbVolume;

impl ScanSource for AdbVolume {
    fn scan_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(self.get_metadata_impl(path))
    }

    fn scan_list<'a>(&'a self, path: &'a Path) -> Walking<'a, Vec<FileEntry>> {
        Box::pin(self.list_directory_impl(path, None, None))
    }
}
