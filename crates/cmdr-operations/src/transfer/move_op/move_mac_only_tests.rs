//! A local copy or cross-filesystem move that leaves Mac-only files out (#314):
//! `.DS_Store` and `._*` never reach the destination or the totals, and a move
//! takes them with the folders that moved, so it never leaves a husk behind.

use super::cross_fs::move_with_staging;
use super::test_support::outcomes_for;
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::transfer::copy::copy_files_with_progress_inner;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::mac_only_files::MacOnlyFiles;

/// `work/` with Finder metadata at every level, and one folder holding nothing else.
fn seed(src_root: &Path) -> PathBuf {
    let work = src_root.join("work");
    fs::create_dir_all(work.join("notes")).unwrap();
    fs::create_dir_all(work.join("only-metadata")).unwrap();
    fs::write(work.join(".DS_Store"), vec![0u8; 6_148]).unwrap();
    fs::write(work.join("._report.txt"), vec![0u8; 4_096]).unwrap();
    fs::write(work.join("report.txt"), b"one").unwrap();
    fs::write(work.join("notes/.DS_Store"), vec![0u8; 6_148]).unwrap();
    fs::write(work.join("notes/todo.md"), b"two").unwrap();
    fs::write(work.join("only-metadata/.DS_Store"), vec![0u8; 6_148]).unwrap();
    work
}

fn state_leaving_out(engine: &TestOperations, mac_only: MacOnlyFiles) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(std::time::Duration::from_millis(0))
            .leaving_out(mac_only),
    )
}

fn assert_no_mac_only_files_under(root: &Path) {
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.unwrap();
        assert!(
            !cmdr_fs::mac_only_files::is_mac_only_file_name(entry.file_name()),
            "{} reached the destination",
            entry.path().display()
        );
    }
}

#[test]
fn a_copy_that_skips_mac_only_files_writes_and_counts_none_of_them() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().unwrap();
    let work = seed(&tmp.path().join("src"));
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&dst).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = state_leaving_out(&engine, MacOnlyFiles::Skip);
    copy_files_with_progress_inner(
        &*events,
        "op-copy-skip-mac",
        &state,
        std::slice::from_ref(&work),
        &dst,
        &WriteOperationConfig::default(),
    )
    .expect("the copy");

    assert_eq!(fs::read(dst.join("work/report.txt")).unwrap(), b"one");
    assert_eq!(fs::read(dst.join("work/notes/todo.md")).unwrap(), b"two");
    assert!(
        dst.join("work/only-metadata").is_dir(),
        "the folder itself is the person's"
    );
    assert_no_mac_only_files_under(&dst);
    let complete = events.complete.lock_ignore_poison();
    assert_eq!(complete[0].files_processed, 2, "the totals leave them out too");
    assert_eq!(complete[0].files_skipped, 0, "left out is not a skip to report");
    assert!(work.join(".DS_Store").exists(), "a copy never touches the source");
}

#[test]
fn a_copy_that_includes_mac_only_files_writes_them_like_any_other() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().unwrap();
    let work = seed(&tmp.path().join("src"));
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&dst).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = state_leaving_out(&engine, MacOnlyFiles::Include);
    copy_files_with_progress_inner(
        &*events,
        "op-copy-include-mac",
        &state,
        std::slice::from_ref(&work),
        &dst,
        &WriteOperationConfig::default(),
    )
    .expect("the copy");

    assert!(dst.join("work/.DS_Store").is_file());
    assert!(dst.join("work/._report.txt").is_file());
    assert!(dst.join("work/only-metadata/.DS_Store").is_file());
    assert_eq!(events.complete.lock_ignore_poison()[0].files_processed, 6);
}

/// The folders moved, so their Finder metadata goes with them: no husk of
/// `.DS_Store`-only folders stays in the source.
#[test]
fn a_cross_fs_move_that_skips_mac_only_files_leaves_no_husk_in_the_source() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().unwrap();
    let work = seed(&tmp.path().join("src"));
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&dst).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = state_leaving_out(&engine, MacOnlyFiles::Skip);
    move_with_staging(
        &*events,
        "op-move-skip-mac",
        &state,
        std::slice::from_ref(&work),
        &dst,
        &WriteOperationConfig::default(),
        0,
    )
    .expect("the move");

    assert!(!work.exists(), "the whole source went, metadata and all");
    assert_eq!(fs::read(dst.join("work/notes/todo.md")).unwrap(), b"two");
    assert!(dst.join("work/only-metadata").is_dir());
    assert_no_mac_only_files_under(&dst);
    assert_eq!(
        outcomes_for(&events, &work).last().copied(),
        Some(SourceItemOutcome::Done)
    );
    assert!(
        events.complete.lock_ignore_poison()[0].appeared_during_move.is_none(),
        "the metadata the move left out is no newcomer"
    );
}

/// A folder that stays for a real reason keeps its metadata: it still
/// describes that folder, and only a folder that GOES takes it along.
#[test]
fn a_source_folder_kept_by_a_newcomer_keeps_its_mac_only_files() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().unwrap();
    let work = seed(&tmp.path().join("src"));
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&dst).unwrap();
    // A preview walked before `late.txt` arrived, so the move never carries it.
    let previews = engine.stores().previews();
    let state = state_leaving_out(&engine, MacOnlyFiles::Skip);
    let scan = crate::scan::scan_sources(
        std::slice::from_ref(&work),
        &state,
        &CollectorEventSink::new(),
        "op-scan",
        WriteOperationType::Move,
    )
    .unwrap();
    fs::write(work.join("notes/late.txt"), b"late").unwrap();
    previews.insert_scan_result(
        "preview-mac-only".into(),
        crate::state::CachedScanResult::from_local_walk(
            crate::types::ScanPreviewPurpose::Transfer,
            vec![work.clone()],
            scan.files,
            scan.dirs,
            scan.total_bytes,
            scan.dedup_bytes,
            vec![(
                work.clone(),
                cmdr_fs::volume::CopyScanResult {
                    file_count: 2,
                    dir_count: 2,
                    total_bytes: 6,
                    dedup_bytes: 6,
                    top_level_is_directory: true,
                    top_level_modified_at: None,
                    top_level_created_at: None,
                },
            )],
            None,
        )
        .leaving_out(MacOnlyFiles::Skip),
    );

    let events = Arc::new(CollectorEventSink::new());
    let config = WriteOperationConfig {
        preview_id: Some("preview-mac-only".into()),
        ..WriteOperationConfig::default()
    };
    move_with_staging(
        &*events,
        "op-move-kept",
        &state,
        std::slice::from_ref(&work),
        &dst,
        &config,
        0,
    )
    .expect("the move");

    assert!(work.join("notes/late.txt").is_file(), "the newcomer stays");
    assert!(
        work.join("notes/.DS_Store").is_file(),
        "and so does its folder's metadata"
    );
    assert!(
        !work.join("only-metadata").exists(),
        "a folder that went took its metadata"
    );
    let complete = events.complete.lock_ignore_poison();
    let appeared = complete[0]
        .appeared_during_move
        .as_ref()
        .expect("the newcomer is reported");
    assert_eq!(appeared.item_count, 1, "only the newcomer is news, never the metadata");
}
