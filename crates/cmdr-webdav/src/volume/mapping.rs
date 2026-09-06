//! Pure type mapping: a PROPFIND entry into `FileEntry`.
use cmdr_fs::entry::FileEntry;

use crate::propfind::PropfindEntry;

/// Builds a [`FileEntry`] from one PROPFIND response.
///
/// ❗ `app_path` is the APP spelling, prefix and all
/// (`webdav://ada@nas.local:443/Photos/a.jpg`), ❌ never the bare remote path.
/// A pane holds what a listing hands it and passes it straight back, and the app
/// anchors it against the volume root on the way (`root_anchored`), so a bare
/// remote path would come back doubled. `RemoteRoot::to_app_path` is where it is
/// made. WebDAV has no symlinks, so the flag is always off.
pub(super) fn propfind_to_file_entry(name: &str, app_path: &str, prop: &PropfindEntry) -> FileEntry {
    let mut entry = FileEntry::new(name.to_string(), app_path.to_string(), prop.is_collection, false);
    entry.size = if prop.is_collection { None } else { prop.size };
    entry.modified_at = prop.modified_at;
    entry.created_at = prop.created_at;
    entry
}

/// The last path segment of a decoded `href`, trailing slash ignored.
pub(super) fn href_name(href: &str) -> &str {
    href.trim_end_matches('/').rsplit('/').next().unwrap_or_default()
}
