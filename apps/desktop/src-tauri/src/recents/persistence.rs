//! The JSON file under a recents list: a schema-versioned envelope, a durable
//! write, and the quarantine path for a file we can't read.

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

/// Reads the list at `path`, newest first.
///
/// A file we can't parse, or one stamped with a schema version we don't know, is
/// quarantined and we start empty: the user keeps using the dialog, and the
/// unreadable file survives one rotation in case we want to look at it. There's
/// only ever been one version, so a migrator would be speculative; when v2 lands,
/// this version check becomes a `match`.
pub(super) fn read<E: RecentEntry>(path: &Path) -> Vec<E> {
    cleanup_tmp_file(path);

    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    match serde_json::from_str::<Envelope<Vec<E>>>(&contents) {
        Ok(envelope) if envelope.schema_version == E::SCHEMA_VERSION => envelope.entries,
        Ok(envelope) => {
            log::warn!(
                target: E::LOG_TARGET,
                "Schema mismatch in {} (file: {}, expected: {}); quarantining and starting fresh",
                E::LOG_NAME, envelope.schema_version, E::SCHEMA_VERSION
            );
            quarantine_broken::<E>(path);
            Vec::new()
        }
        Err(e) => {
            log::warn!(
                target: E::LOG_TARGET,
                "Couldn't parse {} at {:?}: {e}",
                E::LOG_NAME, path
            );
            quarantine_broken::<E>(path);
            Vec::new()
        }
    }
}

/// Writes `entries` durably: temp file + fsync + rename + parent-dir fsync, so the
/// list survives a power loss and not only a process death. See
/// `crate::config::durable_write_json`.
pub(super) fn write<E: RecentEntry>(path: &Path, entries: &[E]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let envelope = Envelope {
        schema_version: E::SCHEMA_VERSION,
        entries,
    };
    let json = serde_json::to_string_pretty(&envelope).map_err(std::io::Error::other)?;
    crate::config::durable_write_json(path, &path.with_extension("json.tmp"), &json)
}

/// Drops a temp file a killed write left behind, so it can't shadow a later one.
fn cleanup_tmp_file(path: &Path) {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Renames the file to a `.broken` sibling so one unreadable snapshot survives for
/// debugging without leaving the user blocked. If the rename itself fails, drop the
/// file outright: a deleted history beats a broken one.
fn quarantine_broken<E: RecentEntry>(path: &Path) {
    let broken = path.with_extension("json.broken");
    if broken.exists() {
        // Overwrite any previous quarantine; only the most recent corruption matters.
        let _ = fs::remove_file(&broken);
    }
    if let Err(e) = fs::rename(path, &broken) {
        log::warn!(
            target: E::LOG_TARGET,
            "Couldn't quarantine corrupted {} at {:?} (will delete instead): {e}",
            E::LOG_NAME, path
        );
        let _ = fs::remove_file(path);
    } else {
        log::warn!(
            target: E::LOG_TARGET,
            "Quarantined corrupted {} to {:?}",
            E::LOG_NAME, broken
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recents::test_support::{TestEntry, entry};

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);

        write(&path, &[entry("b"), entry("a")]).expect("write");

        let loaded: Vec<TestEntry> = read(&path);
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
        let entries: Vec<TestEntry> = read(&dir.path().join(TestEntry::FILENAME));
        assert!(entries.is_empty());
    }

    #[test]
    fn corrupted_json_quarantines_and_starts_fresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        fs::write(&path, "{not valid json at all").expect("write garbage");

        let entries: Vec<TestEntry> = read(&path);
        assert!(entries.is_empty());

        let broken = path.with_extension("json.broken");
        assert!(broken.exists(), "expected quarantine at {broken:?}");
        assert!(!path.exists(), "original file should be gone");
    }

    #[test]
    fn schema_version_mismatch_quarantines_and_starts_fresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        // Hand-write a v2 file that we don't know how to parse.
        fs::write(&path, r#"{"_schemaVersion": 2, "entries": []}"#).expect("write");

        let entries: Vec<TestEntry> = read(&path);
        assert!(entries.is_empty());

        let broken = path.with_extension("json.broken");
        assert!(broken.exists(), "version mismatch should quarantine");
    }

    #[test]
    fn a_second_corruption_replaces_the_quarantined_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        let broken = path.with_extension("json.broken");

        fs::write(&path, "first garbage").expect("write");
        let _: Vec<TestEntry> = read(&path);
        fs::write(&path, "second garbage").expect("write");
        let _: Vec<TestEntry> = read(&path);

        assert_eq!(fs::read_to_string(&broken).expect("read broken"), "second garbage");
    }

    #[test]
    fn stale_tmp_file_is_cleaned_up_on_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(TestEntry::FILENAME);
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, "stale").expect("write tmp");

        // Reading cleans it up even when the real file doesn't exist.
        let _: Vec<TestEntry> = read(&path);
        assert!(!tmp.exists(), "stale tmp should be removed");
    }
}
