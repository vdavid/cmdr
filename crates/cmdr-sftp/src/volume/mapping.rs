//! Pure type mapping: SFTP metadata into `FileEntry`.
use cmdr_fs::entry::FileEntry;
use openssh_sftp_client::metadata::MetaData;

/// Builds a [`FileEntry`] from one SFTP stat answer.
///
/// ❗ `app_path` is the APP spelling, prefix and all
/// (`sftp://ada@nas.local:22/srv/data/photos`), ❌ never the bare server path.
/// A pane holds what a listing hands it and passes it straight back, and the
/// app anchors it against the volume root on the way (`root_anchored`), so a
/// bare server path would come back doubled. `RemoteRoot::to_app_path` is where
/// it is made.
pub(super) fn metadata_to_file_entry(name: &str, app_path: &str, meta: &MetaData) -> FileEntry {
    let file_type = meta.file_type();
    let is_directory = file_type.is_some_and(|t| t.is_dir());
    let is_symlink = file_type.is_some_and(|t| t.is_symlink());
    let mut entry = FileEntry::new(name.to_string(), app_path.to_string(), is_directory, is_symlink);
    entry.size = if is_directory { None } else { meta.len() };
    entry.modified_at = meta.modified().and_then(unix_secs);
    // SFTP v3 carries access and modify times and no creation time, so this stays
    // `None` rather than repeating the modify time and calling it a birth date.
    entry
}

/// Seconds since the epoch, matching `FileEntry`'s own unit.
fn unix_secs(stamp: openssh_sftp_client::UnixTimeStamp) -> Option<u64> {
    stamp
        .as_system_time()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}
