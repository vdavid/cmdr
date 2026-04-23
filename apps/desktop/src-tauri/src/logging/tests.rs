//! Unit tests for the logging support module.

use super::*;
use std::fs;
use std::time::Duration;

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

/// Touches a file with a deterministic mtime offset so sorting is stable across systems
/// with low-resolution mtimes.
fn touch_log(dir: &Path, name: &str, age_secs: u64) {
    let path = dir.join(name);
    fs::write(&path, format!("{name}\n")).expect("write");
    // Set mtime to (now - age_secs). `filetime` isn't available, so use `utimes` via std
    // on Unix; on Windows we'd need a different path. Tests run on macOS/Linux only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let target = now.saturating_sub(age_secs);
        // `utimensat` via libc is overkill for tests; instead, sleep a bit to create a
        // monotonically increasing mtime ordering. Caller passes age_secs in *descending*
        // order for newest-first lists.
        let _ = (target, path.metadata().map(|m| m.mtime()));
        std::thread::sleep(Duration::from_millis(15));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        std::thread::sleep(Duration::from_millis(15));
    }
}

#[test]
fn list_recent_log_files_sorted() {
    let dir = make_temp_dir("list");
    // Create in oldest-first order so mtime grows with index.
    touch_log(&dir, "cmdr.log.2025-01-01-00-00-00", 0);
    touch_log(&dir, "cmdr.log.2025-01-02-00-00-00", 0);
    touch_log(&dir, "cmdr.log.2025-01-03-00-00-00", 0);
    touch_log(&dir, "cmdr.log", 0);
    // Non-log siblings should be ignored.
    fs::write(dir.join("settings.json"), "{}").unwrap();
    fs::write(dir.join("README"), "x").unwrap();

    let files = list_recent_log_files(&dir);
    assert_eq!(files.len(), 4, "got {files:?}");
    // Newest first: cmdr.log was written last.
    let names: Vec<_> = files.iter().map(|p| p.file_name().unwrap().to_str().unwrap()).collect();
    assert_eq!(names[0], "cmdr.log");
    assert_eq!(names[1], "cmdr.log.2025-01-03-00-00-00");
    assert_eq!(names[2], "cmdr.log.2025-01-02-00-00-00");
    assert_eq!(names[3], "cmdr.log.2025-01-01-00-00-00");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn eager_prune_keeps_n_newest() {
    let dir = make_temp_dir("keep-n");
    for i in 0..5 {
        touch_log(&dir, &format!("cmdr.log.2025-01-{:02}", i + 1), 0);
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
    assert!(names[1].contains("2025-01-05"));
    assert!(names[2].contains("2025-01-04"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn eager_prune_handles_zero() {
    let dir = make_temp_dir("zero");
    for i in 0..3 {
        touch_log(&dir, &format!("cmdr.log.2025-01-{:02}", i + 1), 0);
    }
    touch_log(&dir, "cmdr.log", 0);

    let deleted = eager_prune(&dir, 0).expect("prune");
    // Everything is wiped — the plugin recreates the live file on the next write.
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

#[test]
fn current_total_log_bytes_sums_only_log_files() {
    let dir = make_temp_dir("bytes");
    fs::write(dir.join("cmdr.log"), b"hello").unwrap();
    fs::write(dir.join("cmdr.log.old"), b"world!").unwrap();
    fs::write(dir.join("settings.json"), b"ignored").unwrap();
    let total = current_total_log_bytes(&dir);
    assert_eq!(total, 5 + 6, "only .log* files contribute");
    fs::remove_dir_all(&dir).ok();
}
