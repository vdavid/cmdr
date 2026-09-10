//! Pure type mapping: a sync-service stat into `FileEntry`.
use cmdr_fs::entry::FileEntry;

use crate::sync::{SyncEntryKind, SyncStat};

/// Builds a [`FileEntry`] from one stat answer.
///
/// ❗ `app_path` is the APP spelling, prefix and all (`adb://R58M1/sdcard/a.txt`),
/// ❌ never the bare device path: a pane passes an entry's path straight back,
/// and a scheme-free `/sdcard/a.txt` resolves to the Mac's boot disk.
/// `AdbVolume::to_app_path` is where it is made. A symlink is reported as a
/// symlink; whether it points at a directory is the caller's to settle with a
/// following `stat` ([`with_link_target`]), because the sync service's listing
/// is an `lstat`.
pub(super) fn stat_to_file_entry(name: &str, app_path: &str, stat: &SyncStat) -> FileEntry {
    let kind = stat.kind();
    let is_directory = kind == SyncEntryKind::Directory;
    let is_symlink = kind == SyncEntryKind::Symlink;
    let mut entry = FileEntry::new(name.to_string(), app_path.to_string(), is_directory, is_symlink);
    entry.size = if is_directory { None } else { Some(stat.size) };
    entry.modified_at = u64::try_from(stat.mtime).ok();
    entry.permissions = stat.mode & 0o7777;
    entry
}

/// Folds what a symlink points at into its entry, so a link to a folder
/// navigates like one instead of showing as a file.
pub(super) fn with_link_target(mut entry: FileEntry, target: &SyncStat) -> FileEntry {
    if target.kind() == SyncEntryKind::Directory {
        entry.is_directory = true;
        entry.size = None;
    }
    entry
}
