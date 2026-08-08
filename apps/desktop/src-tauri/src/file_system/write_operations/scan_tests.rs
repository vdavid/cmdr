//! Unit tests for `scan.rs`, split out as a `#[path]` child so the module
//! itself stays readable. `super::` here is `scan`, exactly as it was when
//! these lived inline.

use super::super::state::FileInfo;
use super::*;

fn make_file_info(path: &str, source_root: &str) -> FileInfo {
    FileInfo {
        path: PathBuf::from(path),
        source_root: PathBuf::from(source_root),
        size: 100,
        progress_bytes: 100,
        modified: 0,
        created: 0,
        is_symlink: false,
    }
}

/// A `WalkContext` with no oracle, no cancellation, and no progress: the
/// per-source accounting is what's under test, not the callbacks.
fn plain_walk_context<'a>() -> WalkContext<'a, String> {
    WalkContext {
        progress_interval: Duration::from_secs(3600),
        is_cancelled: &|| false,
        on_io_error: &|_, e| e.to_string(),
        on_cancelled: &|| "Cancelled".to_string(),
        on_symlink_loop: &|path| format!("Symlink loop detected: {}", path.display()),
        on_progress: &|_, _, _, _, _| {},
        on_file: None,
    }
}

fn walk_for_test(sources: &[PathBuf]) -> Vec<(PathBuf, CopyScanResult)> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut total_bytes = 0u64;
    let mut dedup_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();
    let mut seen_inodes = HashSet::new();
    walk_sources_with_per_path(
        sources,
        &mut files,
        &mut dirs,
        &mut total_bytes,
        &mut dedup_bytes,
        &mut last_progress_time,
        &mut visited,
        &mut seen_inodes,
        None,
        &plain_walk_context(),
    )
    .expect("the fixture tree walks cleanly")
}

/// The local walk must report each top-level source's own type and totals.
///
/// A completed preview with no per-source data leaves the cross-volume copy
/// drivers guessing, and their guess used to be "file" — which streamed a
/// directory and let a failed copy sweep the destination folder.
#[test]
fn walk_reports_type_and_totals_per_top_level_source() {
    let dir = crate::test_support::TestDir::new("scan-per-path");
    let root: &Path = &dir;
    fs::write(root.join("loose.txt"), b"12345").unwrap();
    fs::create_dir(root.join("album")).unwrap();
    fs::write(root.join("album/one.bin"), b"aaaa").unwrap();
    fs::create_dir(root.join("album/inner")).unwrap();
    fs::write(root.join("album/inner/two.bin"), b"bbbbbb").unwrap();

    let sources = vec![root.join("loose.txt"), root.join("album")];
    let per_path = walk_for_test(&sources);

    assert_eq!(per_path.len(), 2, "one entry per top-level source, in input order");

    let (loose_path, loose) = &per_path[0];
    assert_eq!(loose_path, &sources[0]);
    assert!(!loose.top_level_is_directory);
    assert_eq!(loose.file_count, 1);
    assert_eq!(loose.dir_count, 0);
    // The real size, not the 0 a directory reports. SMB's one-round-trip
    // compound write only engages above 0.
    assert_eq!(loose.total_bytes, 5);

    let (album_path, album) = &per_path[1];
    assert_eq!(album_path, &sources[1]);
    assert!(album.top_level_is_directory);
    assert_eq!(album.file_count, 2);
    // Descendants only: `inner`, never `album` itself.
    assert_eq!(album.dir_count, 1);
    assert_eq!(album.total_bytes, 10);
}

/// A symlink is copied as a link, never dereferenced, so it counts as a
/// FILE source even when it points at a directory.
#[test]
fn walk_reports_a_symlinked_directory_source_as_a_file() {
    let dir = crate::test_support::TestDir::new("scan-per-path-symlink");
    let root: &Path = &dir;
    fs::create_dir(root.join("real")).unwrap();
    fs::write(root.join("real/inside.txt"), b"xyz").unwrap();
    std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();

    let per_path = walk_for_test(&[root.join("link")]);

    assert_eq!(per_path.len(), 1);
    assert!(!per_path[0].1.top_level_is_directory);
    assert_eq!(per_path[0].1.file_count, 1);
    assert_eq!(per_path[0].1.dir_count, 0);
}

#[test]
fn test_top_level_source_path_file() {
    let fi = make_file_info("/home/user/docs/file.txt", "/home/user/docs");
    assert_eq!(top_level_source_path(&fi), PathBuf::from("/home/user/docs/file.txt"));
}

#[test]
fn test_top_level_source_path_nested() {
    let fi = make_file_info("/home/user/docs/mydir/sub/file.txt", "/home/user/docs");
    assert_eq!(top_level_source_path(&fi), PathBuf::from("/home/user/docs/mydir"));
}

#[test]
fn test_build_source_file_counts_mixed() {
    let files = vec![
        make_file_info("/home/docs/file1.txt", "/home/docs"),
        make_file_info("/home/docs/mydir/a.txt", "/home/docs"),
        make_file_info("/home/docs/mydir/b.txt", "/home/docs"),
        make_file_info("/home/docs/mydir/sub/c.txt", "/home/docs"),
        make_file_info("/home/docs/other/x.txt", "/home/docs"),
    ];
    let counts = build_source_file_counts(&files);
    assert_eq!(counts.len(), 3);
    assert_eq!(counts[&PathBuf::from("/home/docs/file1.txt")], 1);
    assert_eq!(counts[&PathBuf::from("/home/docs/mydir")], 3);
    assert_eq!(counts[&PathBuf::from("/home/docs/other")], 1);
}

#[test]
fn test_build_source_file_counts_empty() {
    let counts = build_source_file_counts(&[]);
    assert!(counts.is_empty());
}

#[test]
fn test_build_source_file_counts_single_file() {
    let files = vec![make_file_info("/tmp/a.txt", "/tmp")];
    let counts = build_source_file_counts(&files);
    assert_eq!(counts.len(), 1);
    assert_eq!(counts[&PathBuf::from("/tmp/a.txt")], 1);
}

// ── Walker integration tests ─────────────────────────────────────────

/// Result bundle from `run_walker` / `run_walker_with_sources`. Named
/// fields avoid `clippy::type_complexity` on the helper's return type.
struct WalkOutcome {
    files: Vec<FileInfo>,
    /// Write footprint (every file at full size).
    bytes: u64,
    /// `du`-equivalent source footprint (each inode once).
    dedup_bytes: u64,
    /// Captured `(current_file, current_dir)` pairs from each `on_progress` call.
    progress: Vec<(Option<String>, Option<String>)>,
}

/// Run the walker over `root`, with `progress_interval = 0` so the
/// callback fires on every entry. Captures progress payloads for assertions.
fn run_walker(root: &Path) -> WalkOutcome {
    run_walker_with_sources(&[root.to_path_buf()])
}

fn run_walker_with_sources(sources: &[PathBuf]) -> WalkOutcome {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut total_bytes = 0u64;
    let mut dedup_bytes = 0u64;
    let mut last_progress = Instant::now() - Duration::from_secs(60);
    let mut visited = HashSet::new();
    let mut seen_inodes = HashSet::new();
    let captured = std::cell::RefCell::new(Vec::new());
    let ctx = WalkContext::<'_, String> {
        progress_interval: Duration::from_millis(0),
        is_cancelled: &|| false,
        on_io_error: &|p, e| format!("io: {} {}", p.display(), e),
        on_cancelled: &|| "cancelled".to_string(),
        on_symlink_loop: &|p| format!("loop: {}", p.display()),
        on_progress: &|_, _, _, cur_file, cur_dir| {
            captured.borrow_mut().push((cur_file, cur_dir));
        },
        on_file: None,
    };
    for source in sources {
        let source_root = source.parent().unwrap_or(source);
        walk_dir_recursive(
            source,
            source_root,
            &mut files,
            &mut dirs,
            &mut total_bytes,
            &mut dedup_bytes,
            &mut last_progress,
            &mut visited,
            &mut seen_inodes,
            None,
            &ctx,
        )
        .expect("walk should succeed");
    }
    WalkOutcome {
        files,
        bytes: total_bytes,
        dedup_bytes,
        progress: captured.into_inner(),
    }
}

#[test]
fn walker_emits_current_dir_for_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let subdir = root.join("inner");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("a.txt"), b"hello").unwrap();

    let outcome = run_walker(root);

    // Find the progress event for a.txt. Its parent dir should be `inner`.
    let a_event = outcome
        .progress
        .iter()
        .find(|(f, _)| f.as_deref() == Some("a.txt"))
        .expect("walker should have emitted progress for a.txt");
    let dir = a_event.1.as_deref().expect("current_dir should be set for a file");
    assert!(dir.ends_with("inner"), "expected dir to end with 'inner', got: {dir}");
}

#[test]
fn walker_sums_bytes_for_unique_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("a.bin"), vec![0u8; 1000]).unwrap();
    fs::write(root.join("b.bin"), vec![0u8; 2000]).unwrap();

    let outcome = run_walker(root);
    assert_eq!(outcome.files.len(), 2);
    assert_eq!(outcome.bytes, 3000);
}

#[cfg(unix)]
#[test]
fn walker_dedupes_hardlinks_by_inode() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let original = root.join("original.bin");
    let link = root.join("link.bin");
    fs::write(&original, vec![0u8; 1000]).unwrap();
    fs::hard_link(&original, &link).unwrap();

    let outcome = run_walker(root);
    // Both directory entries are visited (the delete/copy op must unlink both)…
    assert_eq!(outcome.files.len(), 2, "both hardlinked entries should be enumerated");
    // …the write footprint counts both (a cross-volume copy writes both)…
    assert_eq!(
        outcome.bytes, 2000,
        "write footprint should count both hardlinked entries"
    );
    // …but the source footprint counts the inode once (what delete frees).
    assert_eq!(
        outcome.dedup_bytes, 1000,
        "source footprint should count the shared inode once"
    );
}

#[cfg(unix)]
#[test]
fn walker_dedupes_hardlinks_across_separate_sources() {
    // A file hardlinked into two different source roots in one scan
    // should still contribute its bytes exactly once.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    fs::create_dir(&dir_a).unwrap();
    fs::create_dir(&dir_b).unwrap();
    let original = dir_a.join("file.bin");
    fs::write(&original, vec![0u8; 5000]).unwrap();
    fs::hard_link(&original, dir_b.join("file.bin")).unwrap();

    let outcome = run_walker_with_sources(&[dir_a.clone(), dir_b.clone()]);
    assert_eq!(outcome.files.len(), 2);
    assert_eq!(
        outcome.bytes, 10000,
        "write footprint counts both copies (cross-volume copy writes both)"
    );
    assert_eq!(
        outcome.dedup_bytes, 5000,
        "source footprint counts the shared inode once across source roots"
    );
}

#[cfg(unix)]
#[test]
fn walker_does_not_dedupe_distinct_inodes_with_same_size() {
    // Sanity: two unrelated 1000-byte files (distinct inodes) should
    // sum to 2000, not 1000.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("a.bin"), vec![0u8; 1000]).unwrap();
    fs::write(root.join("b.bin"), vec![1u8; 1000]).unwrap();

    let outcome = run_walker(root);
    assert_eq!(outcome.bytes, 2000);
}

// ── WriteProgressEvent constructor / builder tests ───────────────────

#[test]
fn write_progress_new_defaults_scan_meta_to_none() {
    use super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
    let event = WriteProgressEvent::new(
        "op-1".to_string(),
        WriteOperationType::Delete,
        WriteOperationPhase::Scanning,
        Some("foo.txt".to_string()),
        10,
        0,
        1234,
        0,
    );
    assert_eq!(event.current_dir, None);
    assert_eq!(event.dirs_done, 0);
    assert_eq!(event.expected_files_total, None);
    assert_eq!(event.expected_bytes_total, None);
}

#[test]
fn with_scan_meta_populates_all_fields() {
    use super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
    use cmdr_index::ExpectedTotals;
    let event = WriteProgressEvent::new(
        "op-1".to_string(),
        WriteOperationType::Copy,
        WriteOperationPhase::Scanning,
        Some("foo.txt".to_string()),
        10,
        0,
        500,
        0,
    )
    .with_scan_meta(
        Some("/some/dir".to_string()),
        3,
        Some(ExpectedTotals {
            files: 100,
            bytes: 5000,
        }),
    );
    assert_eq!(event.current_dir.as_deref(), Some("/some/dir"));
    assert_eq!(event.dirs_done, 3);
    assert_eq!(event.expected_files_total, Some(100));
    assert_eq!(event.expected_bytes_total, Some(5000));
}

#[test]
fn with_scan_meta_handles_missing_expected_totals() {
    use super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
    let event = WriteProgressEvent::new(
        "op-1".to_string(),
        WriteOperationType::Copy,
        WriteOperationPhase::Scanning,
        None,
        0,
        0,
        0,
        0,
    )
    .with_scan_meta(Some("/x".to_string()), 2, None);
    assert_eq!(event.current_dir.as_deref(), Some("/x"));
    assert_eq!(event.dirs_done, 2);
    // No expected totals → fields stay None so the FE falls back to tallies-only.
    assert_eq!(event.expected_files_total, None);
    assert_eq!(event.expected_bytes_total, None);
}
