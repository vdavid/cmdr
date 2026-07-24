//! Unit tests for the logging support module.

use super::*;
use std::fs;
use std::time::{Duration, SystemTime};

/// Creates a unique temp dir under the OS temp root and returns its path. Tests are
/// responsible for cleaning up. We deliberately avoid the `tempfile` crate to keep this
/// module dependency-free.
fn make_temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("cmdr-logging-test-{label}-{nanos}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Writes a log file and stamps its mtime `age_secs` into the past.
///
/// Sorting is by mtime, so the tests need an exact age per file. Stamping gives that on any
/// filesystem, however coarse its timestamp resolution, and creation order stops mattering.
fn touch_log(dir: &Path, name: &str, age_secs: u64) {
    let path = dir.join(name);
    fs::write(&path, format!("{name}\n")).expect("write");
    let aged = SystemTime::now() - Duration::from_secs(age_secs);
    filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(aged)).expect("stamp mtime");
}

#[test]
fn list_recent_log_files_sorted() {
    let dir = make_temp_dir("list");
    // file-rotate uses numeric suffixes (`cmdr.log.1`, `.2`, ...), with the live file having no
    // suffix, so a higher suffix is an older file.
    touch_log(&dir, "cmdr.log.3", 30);
    touch_log(&dir, "cmdr.log.2", 20);
    touch_log(&dir, "cmdr.log.1", 10);
    touch_log(&dir, "cmdr.log", 0);
    // Non-log siblings should be ignored.
    fs::write(dir.join("settings.json"), "{}").unwrap();
    fs::write(dir.join("README"), "x").unwrap();

    let files = list_recent_log_files(&dir);
    assert_eq!(files.len(), 4, "got {files:?}");
    // Newest first: cmdr.log was written last.
    let names: Vec<_> = files.iter().map(|p| p.file_name().unwrap().to_str().unwrap()).collect();
    assert_eq!(names[0], "cmdr.log");
    assert_eq!(names[1], "cmdr.log.1");
    assert_eq!(names[2], "cmdr.log.2");
    assert_eq!(names[3], "cmdr.log.3");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn eager_prune_keeps_n_newest() {
    let dir = make_temp_dir("keep-n");
    // A higher suffix is an older file.
    for i in 1..=5 {
        touch_log(&dir, &format!("cmdr.log.{i}"), i * 10);
    }
    touch_log(&dir, "cmdr.log", 0);
    assert_eq!(list_recent_log_files(&dir).len(), 6);

    let deleted = eager_prune(&dir, 3).expect("prune");
    assert_eq!(deleted, 3);

    let remaining = list_recent_log_files(&dir);
    assert_eq!(remaining.len(), 3);
    let names: Vec<_> = remaining
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names[0], "cmdr.log", "live file must survive");
    assert_eq!(names[1], "cmdr.log.1");
    assert_eq!(names[2], "cmdr.log.2");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn eager_prune_handles_zero() {
    let dir = make_temp_dir("zero");
    for i in 1..=3 {
        touch_log(&dir, &format!("cmdr.log.{i}"), i * 10);
    }
    touch_log(&dir, "cmdr.log", 0);

    let deleted = eager_prune(&dir, 0).expect("prune");
    // Everything is wiped: the plugin recreates the live file on the next write.
    assert_eq!(deleted, 4);
    assert_eq!(list_recent_log_files(&dir).len(), 0);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn eager_prune_handles_missing_dir() {
    let dir = make_temp_dir("missing");
    fs::remove_dir_all(&dir).ok();
    assert!(!dir.exists());
    let deleted = eager_prune(&dir, 4).expect("prune missing dir");
    assert_eq!(deleted, 0);
}

/// Fix #2 (pattern half): the pre-`319d5d37` `tauri-plugin-log` rotation files
/// (`Cmdr_<timestamp>.log`) must NOT appear in `list_recent_log_files`. They're
/// cleaned up at startup by `cleanup_legacy_log_files`, but until that sweep runs
/// we still don't want them polluting bundles.
#[test]
fn list_recent_log_files_rejects_legacy_naming() {
    let dir = make_temp_dir("legacy-pattern");
    fs::write(dir.join("cmdr.log"), b"live").unwrap();
    fs::write(dir.join("cmdr.log.1"), b"rotated-1").unwrap();
    fs::write(dir.join("Cmdr_2026-03-14_20-54-41.log"), b"legacy").unwrap();
    fs::write(dir.join("Cmdr_old.log"), b"legacy-2").unwrap();
    fs::write(dir.join("cmdr.logsy"), b"weird suffix").unwrap();
    fs::write(dir.join("notes.log"), b"unrelated").unwrap();

    let files = list_recent_log_files(&dir);
    let names: Vec<_> = files.iter().map(|p| p.file_name().unwrap().to_str().unwrap()).collect();

    assert!(names.iter().any(|n| n.eq_ignore_ascii_case("cmdr.log")));
    assert!(names.iter().any(|n| n.eq_ignore_ascii_case("cmdr.log.1")));
    assert!(
        !names.iter().any(|n| n.starts_with("Cmdr_")),
        "legacy `Cmdr_*.log` must not appear in active list (got {names:?})"
    );
    assert!(!names.contains(&"cmdr.logsy"));
    assert!(!names.contains(&"notes.log"));

    fs::remove_dir_all(&dir).ok();
}

/// Fix #2 (cleanup half): `cleanup_legacy_log_files` removes legacy rotation files
/// and leaves the active `cmdr.log` family alone. Idempotent.
#[test]
fn cleanup_legacy_log_files_removes_only_legacy() {
    let dir = make_temp_dir("legacy-cleanup");
    let live = dir.join("cmdr.log");
    let rotated = dir.join("cmdr.log.1");
    let legacy = dir.join("Cmdr_2026-03-14_20-54-41.log");
    let unrelated = dir.join("notes.log");
    fs::write(&live, b"live").unwrap();
    fs::write(&rotated, b"rotated").unwrap();
    fs::write(&legacy, b"legacy").unwrap();
    fs::write(&unrelated, b"unrelated").unwrap();

    let removed = cleanup_legacy_log_files(&dir);
    assert_eq!(removed, 1, "exactly one legacy file should be removed");
    assert!(live.exists(), "live cmdr.log must survive");
    assert!(rotated.exists(), "rotated cmdr.log.1 must survive");
    assert!(!legacy.exists(), "legacy file must be gone");
    assert!(unrelated.exists(), "unrelated files are not touched");

    // Idempotent: a second sweep removes nothing.
    let again = cleanup_legacy_log_files(&dir);
    assert_eq!(again, 0, "second sweep must find nothing");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn is_active_log_file_predicate() {
    assert!(is_active_log_file("cmdr.log"));
    assert!(is_active_log_file("Cmdr.log"));
    assert!(is_active_log_file("cmdr.log.1"));
    assert!(is_active_log_file("cmdr.log.42"));
    assert!(!is_active_log_file("cmdr.log."));
    assert!(!is_active_log_file("cmdr.log.abc"));
    assert!(!is_active_log_file("Cmdr_2026.log"));
    assert!(!is_active_log_file("notes.log"));
    assert!(!is_active_log_file("cmdr.logsy"));
}

#[test]
fn is_legacy_log_file_predicate() {
    assert!(is_legacy_log_file("Cmdr_2026-03-14_20-54-41.log"));
    assert!(is_legacy_log_file("cmdr_old.log"));
    assert!(!is_legacy_log_file("cmdr.log"));
    assert!(!is_legacy_log_file("cmdr.log.1"));
    assert!(!is_legacy_log_file("Cmdr_.log"));
    assert!(!is_legacy_log_file("notes.log"));
}
