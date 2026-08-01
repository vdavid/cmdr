//! The naming convention every Cmdr scratch file follows, and the predicate
//! that recognizes one.
//!
//! Cmdr never writes a file at its final name until the last byte has arrived.
//! Every write lands on a sibling carrying one of the markers below and takes
//! its real name by a rename, so a crash mid-transfer leaves something nobody
//! mistakes for their data (`write_operations/transfer/staged_write.rs`).
//!
//! Two markers, both ours:
//!
//! - [`STAGING_TEMP_MARKER`] (`.cmdr-tmp-`) carries the NEW bytes on their way in.
//! - [`STAGING_ASIDE_MARKER`] (`.cmdr-temp-`) holds the ORIGINAL file a
//!   safe-overwrite renamed out of the way, so it survives until the replacement
//!   is complete (`write_operations/overwrite.rs`).
//!
//! Both are infixes, not prefixes: the temp for `photo.jpg` is
//! `photo.jpg.cmdr-tmp-<uuid>`, keeping the original name legible in a crash
//! leftover. A leading dot would have hidden them from the dotfile filter for
//! free, but it would also hide them from everyone browsing with hidden files
//! shown, which is where a leftover most needs to be seen.

/// Marks a file holding bytes on their way IN: the staging sibling a write
/// streams into before the rename that gives it its real name.
pub const STAGING_TEMP_MARKER: &str = ".cmdr-tmp-";

/// Marks a file holding the ORIGINAL bytes of a safe-overwrite: the file that
/// was already at the destination, renamed aside so it survives until the
/// replacement is fully written.
pub const STAGING_ASIDE_MARKER: &str = ".cmdr-temp-";

/// Whether `name` is one of Cmdr's scratch files.
///
/// Matches on the file NAME, so pass `path.file_name()`, never a whole path: a
/// directory somewhere up the tree could otherwise carry a marker and make
/// everything under it look like scratch.
///
/// A `true` here says only "Cmdr's naming convention", NOT "safe to delete" and
/// NOT "hide it". A leftover from an interrupted transfer wears the same name as
/// a live one, and telling those apart takes the operation state
/// (`write_operations::is_staging_temp_in_flight`), not the name.
pub fn is_staging_temp_name(name: &str) -> bool {
    name.contains(STAGING_TEMP_MARKER) || name.contains(STAGING_ASIDE_MARKER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_both_markers() {
        assert!(is_staging_temp_name("photo.jpg.cmdr-tmp-3f2a"));
        assert!(is_staging_temp_name("photo.jpg.cmdr-temp-3f2a"));
    }

    /// The markers are infixes, so the original name stays legible in front of
    /// them and a leftover tells the user which file it came from.
    #[test]
    fn matches_mid_name_not_only_at_the_start() {
        assert!(is_staging_temp_name("a.very.long.name.tar.gz.cmdr-tmp-3f2a"));
    }

    #[test]
    fn leaves_ordinary_names_alone() {
        assert!(!is_staging_temp_name("photo.jpg"));
        assert!(!is_staging_temp_name(".hidden"));
        // Close, but not ours: no trailing separator before the uuid.
        assert!(!is_staging_temp_name("notes.cmdr-tmp"));
        assert!(!is_staging_temp_name("cmdr-tmp-notes"));
    }
}
