//! The scan preview lifecycle API: what `get_scan_preview_totals` answers.

use std::path::PathBuf;

use uuid::Uuid;

use super::scan_cache::{insert_scan_result, release_preview};
use super::scan_preview::get_scan_preview_totals;
use super::state::{CachedScanResult, FileInfo};
use crate::file_system::volume::CopyScanResult;

/// `get_scan_preview_totals` must return the cached counters when a scan
/// has completed. Pins the contract the FE relies on to recover its
/// display state when scan events fire before listeners attach (the
/// regression that flaked `mtp-copy-preflight-uses-cache.spec.ts` after
/// M2a's watcher-backed oracle made scans nearly instant).
#[test]
fn get_scan_preview_totals_returns_cached_counts_after_complete() {
    let preview_id = format!("test-{}", Uuid::new_v4());
    let source = PathBuf::from("/src");
    // A real local-walk shape: seven files, two directories, and one
    // per-source result. Building it through `from_local_walk` is what
    // keeps the fixture honest — `file_count` comes from the file list, so
    // the test can't quietly assert a count no walk would emit.
    let files: Vec<FileInfo> = (0..7)
        .map(|i| FileInfo {
            path: source.join(format!("f{i}.bin")),
            source_root: PathBuf::from("/"),
            size: 1_763,
            progress_bytes: 1_763,
            modified: 0,
            created: 0,
            is_symlink: false,
        })
        .collect();
    insert_scan_result(
        preview_id.clone(),
        CachedScanResult::from_local_walk(
            vec![source.clone()],
            files,
            vec![source.join("d1"), source.join("d2")],
            12_345,
            12_345,
            vec![(
                source,
                CopyScanResult {
                    file_count: 7,
                    dir_count: 2,
                    total_bytes: 12_345,
                    dedup_bytes: 12_345,
                    top_level_is_directory: true,
                    top_level_modified_at: None,
                },
            )],
            None,
        ),
    );

    let totals = get_scan_preview_totals(&preview_id).expect("totals should be present");
    assert_eq!(totals.files_total, 7);
    assert_eq!(totals.dirs_total, 2);
    assert_eq!(totals.bytes_total, 12_345);

    release_preview(&preview_id);
}

/// `get_scan_preview_totals` returns `None` while the scan is still
/// running (the cache is keyed by preview_id; absence == not complete).
#[test]
fn get_scan_preview_totals_returns_none_for_unknown_preview() {
    let unknown = format!("nonexistent-{}", Uuid::new_v4());
    assert!(get_scan_preview_totals(&unknown).is_none());
}
