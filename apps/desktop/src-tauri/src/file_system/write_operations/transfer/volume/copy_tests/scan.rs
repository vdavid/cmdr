//! Config defaults, the skipped-count suffix, and the scan phase:
//! `scan_for_volume_copy` over `InMemoryVolume` and `LocalPosixVolume`, plus
//! the `scan_for_copy_batch` progress callback.

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
    // Humanized via search::query::format_size (binary GiB labeled GB, per
    // the existing project convention there).
    assert_eq!(
        format_skipped_suffix(1, 3_100_000_000),
        " (of which skipped 1 file, 2.9 GB)"
    );
}

#[test]
fn test_format_skipped_suffix_plural() {
    assert_eq!(format_skipped_suffix(2, 200), " (of which skipped 2 files, 200 B)");
    assert_eq!(
        format_skipped_suffix(821, 17_500_000_000),
        " (of which skipped 821 files, 16.3 GB)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_with_in_memory_volumes() {
    let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
    source.create_file(Path::new("/file1.txt"), b"Hello").await.unwrap();
    source.create_file(Path::new("/file2.txt"), b"World").await.unwrap();
    let source = Arc::new(source);

    let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000));

    let paths = vec![PathBuf::from("/file1.txt"), PathBuf::from("/file2.txt")];
    let result = scan_for_volume_copy(source.as_ref(), &paths, dest.as_ref(), Path::new("/"), 10)
        .await
        .unwrap();

    assert_eq!(result.file_count, 2);
    assert_eq!(result.total_bytes, 10); // "Hello" + "World"
    assert!(result.conflicts.is_empty());
    let dest_space = result.dest_space.expect("this destination reports its space");
    assert!(dest_space.available_bytes().expect("bounded") >= result.total_bytes);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_detects_conflicts_in_memory() {
    let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
    source
        .create_file(Path::new("/report.txt"), b"new content")
        .await
        .unwrap();
    let source = Arc::new(source);

    let dest = InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000);
    dest.create_file(Path::new("/report.txt"), b"old content")
        .await
        .unwrap();
    let dest = Arc::new(dest);

    let result = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/report.txt")],
        dest.as_ref(),
        Path::new("/"),
        10,
    )
    .await
    .unwrap();

    assert_eq!(result.file_count, 1);
    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].source_path, "report.txt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_directory_tree() {
    let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
    source.create_directory(Path::new("/docs")).await.unwrap();
    source
        .create_file(Path::new("/docs/readme.txt"), b"Read me")
        .await
        .unwrap();
    source
        .create_file(Path::new("/docs/notes.txt"), b"Notes here")
        .await
        .unwrap();
    let source = Arc::new(source);

    let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000));

    let result = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/docs")],
        dest.as_ref(),
        Path::new("/"),
        10,
    )
    .await
    .unwrap();

    assert_eq!(result.file_count, 2);
    assert_eq!(result.total_bytes, 17); // 7 + 10
}

// ========================================
// LocalPosixVolume integration tests
// ========================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_with_local_volumes() {
    use std::fs;

    let src_dir = TestDir::new("volume_scan_src");
    let dst_dir = TestDir::new("volume_scan_dst");

    // Create source files
    fs::write(src_dir.join("file1.txt"), "Hello").unwrap();
    fs::write(src_dir.join("file2.txt"), "World").unwrap();

    let source = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let paths = vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")];
    let scan = scan_for_volume_copy(source.as_ref(), &paths, dest.as_ref(), Path::new(""), 10)
        .await
        .unwrap();
    assert_eq!(scan.file_count, 2);
    assert_eq!(scan.total_bytes, 10); // "Hello" + "World"
    assert!(scan.conflicts.is_empty());
    assert!(
        scan.dest_space
            .expect("this destination reports its space")
            .total_bytes()
            .expect("bounded")
            > 0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_detects_conflicts() {
    use std::fs;

    let src_dir = TestDir::new("volume_conflict_src");
    let dst_dir = TestDir::new("volume_conflict_dst");

    // Create source file
    fs::write(src_dir.join("conflict.txt"), "New content").unwrap();

    // Create existing file at destination
    fs::write(dst_dir.join("conflict.txt"), "Old content").unwrap();

    let source = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let scan = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("conflict.txt")],
        dest.as_ref(),
        Path::new(""),
        10,
    )
    .await
    .unwrap();
    assert_eq!(scan.file_count, 1);
    assert_eq!(scan.conflicts.len(), 1);
    assert_eq!(scan.conflicts[0].source_path, "conflict.txt");
    assert_eq!(scan.conflicts[0].source_size, 11); // "New content"
    assert_eq!(scan.conflicts[0].dest_size, 11); // "Old content"
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_max_conflicts() {
    use std::fs;

    let src_dir = TestDir::new("volume_max_conflicts_src");
    let dst_dir = TestDir::new("volume_max_conflicts_dst");

    // Create 5 conflicting files
    let mut source_paths = Vec::new();
    for i in 0..5 {
        let name = format!("file{}.txt", i);
        fs::write(src_dir.join(&name), "new").unwrap();
        fs::write(dst_dir.join(&name), "old").unwrap();
        source_paths.push(PathBuf::from(&name));
    }

    let source = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    // Request max 3 conflicts
    let scan = scan_for_volume_copy(source.as_ref(), &source_paths, dest.as_ref(), Path::new(""), 3)
        .await
        .unwrap();
    assert_eq!(scan.conflicts.len(), 3); // Limited to max
}

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
/// (it's still called by `copy_volumes_with_progress` and `scan_for_volume_copy`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_copy_batch_without_progress_still_works() {
    let vol = InMemoryVolume::new("V").with_space_info(1_000_000, 500_000);
    vol.create_file(Path::new("/x.txt"), b"hello").await.unwrap();
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let result = vol.scan_for_copy_batch(&[PathBuf::from("/x.txt")]).await.unwrap();
    assert_eq!(result.aggregate.file_count, 1);
    assert_eq!(result.aggregate.total_bytes, 5);
}
