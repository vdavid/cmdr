//! Config defaults, the skipped-count suffix, and the scan phase's
//! `scan_for_copy_batch` progress callback.

use super::*;

#[test]
fn test_volume_copy_config_default() {
    let config = VolumeCopyConfig::default();
    assert_eq!(config.progress_interval_ms, 200);
    assert_eq!(config.max_conflicts_to_show, 100);
}

#[test]
fn test_format_skipped_suffix_zero_is_empty() {
    // The annotation is only present when something was actually skipped, so
    // the happy-path completion log stays terse.
    assert_eq!(format_skipped_suffix(0, 0), "");
    // Stray byte count without any files: still empty (treat files as the
    // truth, bytes is just metadata).
    assert_eq!(format_skipped_suffix(0, 12345), "");
}

#[test]
fn test_format_skipped_suffix_singular() {
    assert_eq!(format_skipped_suffix(1, 0), " (of which skipped 1 file, 0 B)");
    // Humanized via search::query::format_size (base 1024, so IEC symbols).
    assert_eq!(
        format_skipped_suffix(1, 3_100_000_000),
        " (of which skipped 1 file, 2.9 GiB)"
    );
}

#[test]
fn test_format_skipped_suffix_plural() {
    assert_eq!(format_skipped_suffix(2, 200), " (of which skipped 2 files, 200 B)");
    assert_eq!(
        format_skipped_suffix(821, 17_500_000_000),
        " (of which skipped 821 files, 16.3 GiB)"
    );
}

// ========================================
// LocalPosixVolume integration tests
// ========================================

/// `scan_for_copy_batch_with_progress` must invoke the callback as it discovers
/// entries so the FE's scan-preview dialog can show a climbing count instead of
/// a frozen 0/0/0 spinner. The default trait implementation (used by
/// `InMemoryVolume` and `LocalPosixVolume`) fires the callback once per scanned
/// path with the running total; `MtpVolume` overrides to thread it through
/// `list_directory_with_progress` for per-entry granularity.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_copy_batch_with_progress_fires_callback() {
    use std::sync::Mutex;

    let vol = InMemoryVolume::new("V").with_space_info(1_000_000, 500_000);
    vol.create_file(Path::new("/a.txt"), b"AA").await.unwrap();
    vol.create_file(Path::new("/b.txt"), b"BBBB").await.unwrap();
    vol.create_file(Path::new("/c.txt"), b"CCCCCC").await.unwrap();
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let calls = Arc::new(Mutex::new(Vec::<usize>::new()));
    let calls_for_cb = Arc::clone(&calls);
    let on_progress = move |p: ListingProgress| {
        calls_for_cb.lock().unwrap().push(p.files);
    };

    let paths = vec![
        PathBuf::from("/a.txt"),
        PathBuf::from("/b.txt"),
        PathBuf::from("/c.txt"),
    ];
    let boundary = crate::file_system::volume::ScanBoundary::new(Some(&on_progress));
    let result = vol.scan_for_copy_batch_with_boundary(&paths, &boundary).await.unwrap();

    assert_eq!(result.aggregate.file_count, 3);
    assert_eq!(result.aggregate.total_bytes, 12); // 2 + 4 + 6

    // Callback must have fired with a monotonically growing count, ending at 3.
    let recorded = calls.lock().unwrap();
    assert!(!recorded.is_empty(), "on_progress must fire at least once");
    assert!(
        recorded.windows(2).all(|w| w[0] <= w[1]),
        "progress counts must be monotonic; saw {recorded:?}",
    );
    assert_eq!(
        *recorded.last().unwrap(),
        3,
        "final progress callback should report the full file count",
    );
}

/// Backwards-compat: the no-progress `scan_for_copy_batch` must keep working
/// (it's still called by `copy_volumes_with_progress`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_copy_batch_without_progress_still_works() {
    let vol = InMemoryVolume::new("V").with_space_info(1_000_000, 500_000);
    vol.create_file(Path::new("/x.txt"), b"hello").await.unwrap();
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let result = vol.scan_for_copy_batch(&[PathBuf::from("/x.txt")]).await.unwrap();
    assert_eq!(result.aggregate.file_count, 1);
    assert_eq!(result.aggregate.total_bytes, 5);
}
