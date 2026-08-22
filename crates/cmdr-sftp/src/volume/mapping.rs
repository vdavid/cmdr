//! Pure type mapping: SFTP metadata into `FileEntry`.
use cmdr_fs::entry::FileEntry;
use openssh_sftp_client::metadata::MetaData;

/// Builds a [`FileEntry`] from one SFTP stat answer.
///
/// `remote_path` is the absolute server-side path, which is also what the app
/// addresses the entry by: there is no mount under this backend, so the two are
/// the same string.
pub(super) fn metadata_to_file_entry(name: &str, remote_path: &str, meta: &MetaData) -> FileEntry {
    let file_type = meta.file_type();
    let is_directory = file_type.is_some_and(|t| t.is_dir());
    let is_symlink = file_type.is_some_and(|t| t.is_symlink());
    let mut entry = FileEntry::new(name.to_string(), remote_path.to_string(), is_directory, is_symlink);
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
