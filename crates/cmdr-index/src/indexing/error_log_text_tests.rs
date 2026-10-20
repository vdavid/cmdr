//! The index's errors that name a folder mark it in a log line (`cmdr_fs::log_error::LogError`),
//! and print it plain everywhere else. A path flattened into one of their message strings ships
//! through the report's regex fallback only, which a share-relative path slips past whole.

use std::path::Path;

use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;

use super::network_scanner::VolumeScanError;
use super::scanner::ScanError;
use super::watch::watcher::WatcherError;

/// A share-relative folder: no leading `/`, so no regex claims it in prose.
const ROOT: &str = "Trips/Summer (Lake House) 2026";

/// `marked` holds `ROOT` as one folder span, and reads as `plain` once that span is plain again.
fn assert_marks_root_as_a_folder(marked: &str, plain: &str) {
    let span = LogPrivate::dir_path(ROOT).to_string();
    assert!(marked.contains(&span), "{ROOT} isn't marked as a folder: {marked:?}");
    assert_eq!(marked.replace(&span, ROOT), plain);
    assert!(plain.contains(ROOT), "{plain:?}");
}

#[test]
fn a_network_scan_root_outside_the_index_marks_the_root() {
    let error = VolumeScanError::OutsideIndex(Path::new(ROOT).to_path_buf());
    assert_marks_root_as_a_folder(&LogError(&error).to_string(), &error.to_string());
}

#[test]
fn a_local_scan_root_outside_the_index_marks_the_root() {
    let error = ScanError::OutsideIndex(Path::new(ROOT).to_path_buf());
    assert_marks_root_as_a_folder(&LogError(&error).to_string(), &error.to_string());
}

#[test]
fn a_refused_watch_marks_its_root() {
    let error = WatcherError::WatchRefused {
        root: Path::new(ROOT).to_path_buf(),
        cause: "No space left on device (os error 28)".to_string(),
    };
    assert_marks_root_as_a_folder(&LogError(&error).to_string(), &error.to_string());
}
