//! Pure type mapping: a PROPFIND entry into `FileEntry`.
use cmdr_fs::entry::FileEntry;

use crate::propfind::PropfindEntry;

/// Builds a [`FileEntry`] from one PROPFIND response. `remote_path` is the
/// root-relative remote path, which is also what the app addresses the entry
/// by. WebDAV has no symlinks, so the flag is always off.
pub(super) fn propfind_to_file_entry(name: &str, remote_path: &str, prop: &PropfindEntry) -> FileEntry {
    let mut entry = FileEntry::new(name.to_string(), remote_path.to_string(), prop.is_collection, false);
    entry.size = if prop.is_collection { None } else { prop.size };
    entry.modified_at = prop.modified_at;
    entry.created_at = prop.created_at;
    entry
}

/// The last path segment of a decoded `href`, trailing slash ignored.
pub(super) fn href_name(href: &str) -> &str {
    href.trim_end_matches('/').rsplit('/').next().unwrap_or_default()
}
