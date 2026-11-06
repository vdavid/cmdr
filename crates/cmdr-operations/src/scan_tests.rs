//! Unit tests for the scan family, a `#[path]` child of `scan.rs` so the module
//! itself stays readable. `super::` here is `scan` and `super::super::` is
//! `cmdr-operations`.
//!
//! It reaches across to `scan.rs`'s three siblings by name: the walk itself
//! (`scan_walker.rs`), the per-source bookkeeping (`scan_source_tracker.rs`),
//! and the write-nothing preview (`scan_dry_run.rs`).

use super::super::scan_dry_run::dry_run_scan;
use super::super::scan_source_tracker::{build_source_file_counts, top_level_source_path};
use super::super::scan_walker::{WalkContext, walk_sources_with_per_path};
use super::super::state::FileInfo;
use super::*;
use crate::host::testing::PANE_ORDER_LISTINGS;
use crate::testing::TestOperations;
use cmdr_fs::volume::CopyScanResult;
use std::fs;
use std::time::Instant;

fn make_file_info(path: &str, source_root: &str) -> FileInfo {
    FileInfo {
        path: PathBuf::from(path),
        source_root: PathBuf::from(source_root),
        size: 100,
        progress_bytes: 100,
        is_symlink: false,
        tree_node: None,
    }
}

/// A `WalkContext` with no oracle, no cancellation, and no progress: the
/// per-source accounting is what's under test, not the callbacks.
fn plain_walk_context<'a>() -> WalkContext<'a, String> {
    WalkContext {
        listings: &PANE_ORDER_LISTINGS,
        // Nothing owns these walks, so there is nothing to park on.
        park_while_paused: &|| {},
        progress_interval: Duration::from_secs(3600),
        is_cancelled: &|| false,
        on_io_error: &|_, e| e.to_string(),
        on_cancelled: &|| "Cancelled".to_string(),
        on_symlink_loop: &|path| format!("Symlink loop detected: {}", path.display()),
        on_mount_boundary: None,
        on_progress: &|_, _, _, _, _| {},
        on_file: None,
        online_only: None,
        sidecars: &NO_SIDECARS,
        mac_only: cmdr_fs::mac_only_files::MacOnlyFiles::Include,
    }
}

/// A volume with native xattrs, where no `._` file is anyone's sidecar.
pub(super) static NO_SIDECARS: std::sync::LazyLock<cmdr_fs::apple_double::SidecarRule> =
    std::sync::LazyLock::new(|| cmdr_fs::apple_double::SidecarRule::with_probe(|_| false));

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
    let dir = cmdr_fs::testing::TestDir::new("scan-per-path");
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
    let dir = cmdr_fs::testing::TestDir::new("scan-per-path-symlink");
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
        listings: &PANE_ORDER_LISTINGS,
        park_while_paused: &|| {},
        progress_interval: Duration::from_millis(0),
        is_cancelled: &|| false,
        on_io_error: &|p, e| format!("io: {} {}", p.display(), e),
        on_cancelled: &|| "cancelled".to_string(),
        on_symlink_loop: &|p| format!("loop: {}", p.display()),
        on_mount_boundary: None,
        on_progress: &|_, _, _, cur_file, cur_dir| {
            captured.borrow_mut().push((cur_file, cur_dir));
        },
        on_file: None,
        online_only: None,
        sidecars: &NO_SIDECARS,
        mac_only: cmdr_fs::mac_only_files::MacOnlyFiles::Include,
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
    use super::super::host::ExpectedTotals;
    use super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
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

/// A dry run of a same-folder copy must report no conflicts.
///
/// The engines answer a source landing on ITSELF by duplicating it under a free
/// name (`transfer/DETAILS.md` § "Self-collision (duplicating in place)"), so
/// there is nothing in the way and nothing to ask about. A dry run that reports
/// one conflict per file describes an operation that never happens.
#[test]
fn a_dry_run_of_a_same_folder_copy_reports_no_conflicts() {
    let engine = TestOperations::new();
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("photo.jpg"), b"pixels").expect("write file");
    fs::create_dir(dir.path().join("docs")).expect("create dir");
    fs::write(dir.path().join("docs/a.txt"), b"notes").expect("write nested file");

    let events = crate::event_sinks::CollectorEventSink::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));
    let result = dry_run_scan(
        &[dir.path().join("photo.jpg"), dir.path().join("docs")],
        dir.path(),
        &state,
        &events,
        "dry-run-self-collision",
        WriteOperationType::Copy,
        Duration::from_secs(3600),
    )
    .expect("the dry run completes");

    assert!(
        result.conflicts.is_empty(),
        "duplicating in place clashes with nothing, but the dry run reported {:?}",
        result.conflicts
    );
    assert_eq!(result.file_count, 2, "both files are still copied, and still counted");
}

/// The other half: a genuine clash at a different destination is still reported.
#[test]
fn a_dry_run_of_a_copy_onto_a_different_file_still_reports_the_conflict() {
    let engine = TestOperations::new();
    let source_dir = tempfile::tempdir().expect("source dir");
    let dest_dir = tempfile::tempdir().expect("dest dir");
    fs::write(source_dir.path().join("photo.jpg"), b"mine").expect("write source");
    fs::write(dest_dir.path().join("photo.jpg"), b"theirs").expect("write dest");

    let events = crate::event_sinks::CollectorEventSink::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));
    let result = dry_run_scan(
        &[source_dir.path().join("photo.jpg")],
        dest_dir.path(),
        &state,
        &events,
        "dry-run-real-conflict",
        WriteOperationType::Copy,
        Duration::from_secs(3600),
    )
    .expect("the dry run completes");

    assert_eq!(result.conflicts.len(), 1, "a real clash still reaches the caller");
}

/// The online-only tally (`OnlineOnlyWatch`), which is how a selected FOLDER in a
/// cloud drive gets answered: `SF_DATALESS` lives on files, so only the walk the
/// delete confirmation already runs can see one.
///
/// ❗ The probe is injected in all three. Only a File Provider can set that flag,
/// so no test can build a tree that really carries it; these pin the WIRING
/// (which entries get asked, and that one hit sticks) rather than the flag read.
mod online_only_tally {
    use super::super::super::scan_walker::OnlineOnlyWatch;
    use super::*;
    use cmdr_fs::testing::TestDir;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Walks `sources` with the tally armed, treating every entry named `evicted`
    /// as online-only, and answers what the walk concluded.
    fn walk_finds_online_only(sources: &[PathBuf], evicted: &str) -> bool {
        let seen = AtomicBool::new(false);
        let probe = |path: &Path, _: Option<&fs::Metadata>| path.file_name().is_some_and(|name| name == evicted);
        let watch = OnlineOnlyWatch {
            probe: &probe,
            seen: &seen,
        };
        let ctx = WalkContext {
            listings: &PANE_ORDER_LISTINGS,
            online_only: Some(&watch),
            ..plain_walk_context()
        };

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
            &ctx,
        )
        .expect("the fixture tree walks cleanly");
        seen.load(Ordering::Relaxed)
    }

    /// One evicted file taints the whole folder: trashing it would pull that file
    /// back down from the provider first.
    #[test]
    fn a_folder_holding_one_online_only_file_is_tainted() {
        let scratch = TestDir::new("walk_online_only_hit");
        let folder = scratch.join("Work");
        fs::create_dir_all(folder.join("deep")).expect("fixture tree");
        fs::write(folder.join("here.txt"), b"local").expect("a materialized file");
        fs::write(folder.join("deep/evicted.txt"), b"stub").expect("the evicted one");

        assert!(walk_finds_online_only(&[folder], "evicted.txt"));
    }

    /// A fully materialized folder trashes exactly as it does today: nothing gets
    /// downloaded, so nothing is routed.
    #[test]
    fn a_fully_materialized_folder_is_not_tainted() {
        let scratch = TestDir::new("walk_online_only_miss");
        let folder = scratch.join("Work");
        fs::create_dir_all(folder.join("deep")).expect("fixture tree");
        fs::write(folder.join("here.txt"), b"local").expect("a materialized file");
        fs::write(folder.join("deep/also-here.txt"), b"local").expect("another");

        assert!(!walk_finds_online_only(&[folder], "evicted.txt"));
    }

    /// A symlink's own flags say nothing about the file it points at, and trashing
    /// a link moves the link, so the walk never asks about one.
    #[test]
    fn a_symlink_in_the_tree_is_never_asked_about() {
        let scratch = TestDir::new("walk_online_only_symlink");
        let folder = scratch.join("Work");
        fs::create_dir_all(&folder).expect("fixture tree");
        fs::write(folder.join("real.txt"), b"local").expect("the target");
        std::os::unix::fs::symlink(folder.join("real.txt"), folder.join("evicted.txt")).expect("the link");

        assert!(
            !walk_finds_online_only(&[folder], "evicted.txt"),
            "the link is named like the evicted file, and must still not taint the walk"
        );
    }
}
