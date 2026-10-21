//! The JSON file under a recents list: a schema-versioned envelope, a durable
//! write, both through `crate::state_recovery`, which sets aside a file we can't
//! read and never lets one be saved over.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::RecentEntry;

/// On-disk shape. `_schemaVersion` lets a later version recognize a file it can't
/// read. Generic over the entry container so a write can serialize a borrowed
/// slice while a read owns a `Vec`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<C> {
    #[serde(rename = "_schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    entries: C,
}

/// Reads the list at `path`, newest first, saying what it found, so a caller can tell
/// no file (`Missing`, a new install) from one it set aside.
///
/// A file we can't read or parse (non-UTF-8 bytes included), or one stamped with a
/// schema version we don't know, is set aside and we start empty
/// (`crate::state_recovery::read_state_file`): the user keeps using the dialog, hears
/// once that it reset, and the unreadable file survives. When the set-aside can't
/// happen, [`write`](fn@write) refuses to save over the file for the session. There's only ever
/// been one version, so a migrator would be speculative; when v2 lands, this version
/// check becomes a `match`.
pub(super) fn read<E: RecentEntry>(path: &Path) -> crate::state_recovery::StateRead<Vec<E>> {
    cleanup_tmp_file(path);
    crate::state_recovery::read_state_file(path, E::STATE_FILE, |bytes| {
        match serde_json::from_slice::<Envelope<Vec<E>>>(bytes) {
            Ok(envelope) if envelope.schema_version == E::SCHEMA_VERSION => Ok(envelope.entries),
            Ok(envelope) => Err(format!(
                "schema version {}, expected {}",
                envelope.schema_version,
                E::SCHEMA_VERSION
            )),
            Err(e) => Err(e.to_string()),
        }
    })
}

/// Writes `entries` durably: temp file + fsync + rename + parent-dir fsync, so the
/// list survives a power loss and not only a process death. See
/// `crate::state_recovery::write_state_file`.
pub(super) fn write<E: RecentEntry>(path: &Path, entries: &[E]) -> Result<(), crate::state_recovery::StateWriteError> {
    let envelope = Envelope {
        schema_version: E::SCHEMA_VERSION,
        entries,
    };
    let json = serde_json::to_string_pretty(&envelope).map_err(std::io::Error::other)?;
    crate::state_recovery::write_state_file(path, &json)
}

/// Drops a temp file a killed write left behind, so it can't shadow a later one.
fn cleanup_tmp_file(path: &Path) {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recents::test_support::{TestEntry, entry};

    fn read_list(path: &Path) -> Vec<TestEntry> {
        read(path).into_value().unwrap_or_default()
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        write(&path, &[entry("b"), entry("a")]).expect("write");

        let loaded: Vec<TestEntry> = read_list(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].key, "b");
        assert_eq!(loaded[1].key, "a");
    }

    #[test]
    fn write_creates_the_parent_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested/deeper").join(TestEntry::FILENAME);

        write(&path, &[entry("a")]).expect("write");

        assert!(path.exists(), "expected the write to create {path:?}");
    }

    #[test]
    fn stored_file_carries_the_schema_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        write(&path, &[entry("a")]).expect("write");

        let raw = fs::read_to_string(&path).expect("read back");
        assert!(raw.contains("\"_schemaVersion\": 1"), "got {raw}");
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let entries: Vec<TestEntry> = read_list(&dir.path().join(TestEntry::FILENAME));
        assert!(entries.is_empty());
    }

    #[test]
    fn corrupted_json_quarantines_and_starts_fresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        fs::write(&path, "{not valid json at all").expect("write garbage");

        let notices = crate::state_recovery::testing::capture_recoveries();
        let entries: Vec<TestEntry> = read_list(&path);
        assert!(entries.is_empty());

        let kept = crate::state_recovery::testing::kept_copies(&path);
        assert_eq!(kept.len(), 1, "expected one set-aside copy: {kept:?}");
        assert!(!path.exists(), "original file should be gone");
        let recoveries = notices.recoveries();
        assert_eq!(recoveries.len(), 1, "one notice");
        assert_eq!(recoveries[0].file, TestEntry::STATE_FILE);
    }

    #[test]
    fn schema_version_mismatch_quarantines_and_starts_fresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        // Hand-write a v2 file that we don't know how to parse.
        fs::write(&path, r#"{"_schemaVersion": 2, "entries": []}"#).expect("write");

        let notices = crate::state_recovery::testing::capture_recoveries();
        let entries: Vec<TestEntry> = read_list(&path);
        assert!(entries.is_empty());

        assert_eq!(
            crate::state_recovery::testing::kept_copies(&path).len(),
            1,
            "version mismatch should set the file aside"
        );
        assert_eq!(notices.recoveries().len(), 1);
    }

    #[test]
    fn a_second_corruption_keeps_both_copies() {
        let _notices = crate::state_recovery::testing::capture_recoveries();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        fs::write(&path, "first garbage").expect("write");
        let _: Vec<TestEntry> = read_list(&path);
        fs::write(&path, "second garbage").expect("write");
        let _: Vec<TestEntry> = read_list(&path);

        let kept = crate::state_recovery::testing::kept_copies(&path);
        let contents: Vec<String> = kept
            .iter()
            .map(|copy| fs::read_to_string(copy).expect("read a copy"))
            .collect();
        assert_eq!(contents, vec!["first garbage", "second garbage"]);
    }

    /// A file that's there but can't be read (mode 000) is set aside before the next add writes
    /// a one-entry list over it.
    #[test]
    fn an_unreadable_file_survives_the_next_add() {
        use std::os::unix::fs::PermissionsExt;
        // Root reads a mode-000 file anyway (the Linux Docker lane runs as root).
        // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let notices = crate::state_recovery::testing::capture_recoveries();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        write(&path, &[entry("b"), entry("a")]).expect("write the real list");
        let original = fs::read(&path).expect("read it back");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("make it unreadable");

        let loaded: Vec<TestEntry> = read_list(&path);
        assert!(loaded.is_empty());
        write(&path, &[entry("c")]).expect("the next add saves");

        let kept = crate::state_recovery::testing::kept_copies(&path);
        assert_eq!(kept.len(), 1, "the real list is set aside: {kept:?}");
        fs::set_permissions(&kept[0], fs::Permissions::from_mode(0o644)).expect("read the copy");
        assert_eq!(fs::read(&kept[0]).expect("read the copy"), original);
        assert_eq!(notices.recoveries().len(), 1);
    }

    /// Bytes that aren't UTF-8 don't parse, so they're set aside like any other broken file.
    #[test]
    fn a_non_utf8_file_survives_the_next_add() {
        let _notices = crate::state_recovery::testing::capture_recoveries();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        fs::write(&path, b"{\"_schemaVersion\": 1, \"entries\": [\xff\xfe]}").expect("write");

        let loaded: Vec<TestEntry> = read_list(&path);
        assert!(loaded.is_empty());
        write(&path, &[entry("c")]).expect("the next add saves");

        let kept = crate::state_recovery::testing::kept_copies(&path);
        assert_eq!(kept.len(), 1, "set aside: {kept:?}");
        assert_eq!(
            fs::read(&kept[0]).expect("read the copy"),
            b"{\"_schemaVersion\": 1, \"entries\": [\xff\xfe]}"
        );
    }

    #[test]
    fn stale_tmp_file_is_cleaned_up_on_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, "stale").expect("write tmp");

        // Reading cleans it up even when the real file doesn't exist.
        let _: Vec<TestEntry> = read_list(&path);
        assert!(!tmp.exists(), "stale tmp should be removed");
    }
}
