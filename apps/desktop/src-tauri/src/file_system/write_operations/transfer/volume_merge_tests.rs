//! Folder-merge engine tests: per-file conflict resolution INSIDE a directory
//! merge (scan-as-you-merge), the dir-vs-dir "always merge, never prompt"
//! contract, and the conflict-dispatch mutex that serializes the human across
//! concurrent / nested merges.
//!
//! These drive the real `copy_volumes_with_progress` pipeline against
//! `InMemoryVolume` pairs + `CollectorEventSink`, so the whole stack — preflight,
//! the serial/concurrent split, `copy_directory_streaming`, the resolver — runs
//! exactly as in production. Shared fixtures `make_state` / `make_volumes` live in
//! `volume_copy_tests.rs` (`super::tests`).

use super::super::conflict_responder_test_support::{
    ConflictResponderSink, file_conflict_count, folder_conflict_count_both_dirs,
};
use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::state::cancel_write_operation;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{
    CollectorEventSink, ConflictResolution, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteErrorEvent, WriteSourceItemDoneEvent,
};
use std::sync::atomic::AtomicU8;

// ============================================================================
// Helpers
// ============================================================================

/// Reads a whole file from a volume into a `Vec<u8>`.
async fn read_all(vol: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
    let mut stream = vol.open_read_stream(Path::new(path)).await.unwrap();
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    out
}

/// A merge fixture: a source tree and a dest tree of the same top-level name
/// (`/album`) with overlapping AND non-overlapping content at several depths.
///
/// Dest-only files (`keep*`) must survive every policy. Source-only files
/// (`fresh*`) must always arrive. Clashing files (`clash*`) follow the policy.
/// `/album/sub` is a nested merge with its own clash + dest-only file. There's
/// also a type mismatch: source `/album/swap` is a FILE, dest `/album/swap` is a
/// DIRECTORY.
async fn make_rich_merge() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    let (source, dest) = make_volumes();

    // Source tree.
    source.create_directory(Path::new("/album")).await.unwrap();
    source
        .create_file(Path::new("/album/fresh.txt"), b"SRC-fresh")
        .await
        .unwrap();
    source
        .create_file(Path::new("/album/clash.txt"), b"SRC-clash-larger")
        .await
        .unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/fresh2.txt"), b"SRC-fresh2")
        .await
        .unwrap();
    source
        .create_file(Path::new("/album/sub/clash2.txt"), b"SRC-clash2")
        .await
        .unwrap();
    // Type mismatch: source FILE named `swap`.
    source
        .create_file(Path::new("/album/swap"), b"SRC-swap-file")
        .await
        .unwrap();

    // Dest tree (pre-existing).
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/keep.txt"), b"DEST-keep")
        .await
        .unwrap();
    dest.create_file(Path::new("/album/clash.txt"), b"DEST-clash")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album/sub")).await.unwrap();
    dest.create_file(Path::new("/album/sub/keep2.txt"), b"DEST-keep2")
        .await
        .unwrap();
    dest.create_file(Path::new("/album/sub/clash2.txt"), b"DEST-clash2")
        .await
        .unwrap();
    // Type mismatch: dest DIR named `swap` with a file inside it.
    dest.create_directory(Path::new("/album/swap")).await.unwrap();
    dest.create_file(Path::new("/album/swap/inner.txt"), b"DEST-swap-inner")
        .await
        .unwrap();

    (source, dest)
}

// ============================================================================
// The invariant property test
// ============================================================================

/// THE INVARIANT: a merge never deletes or overwrites a dest file the source
/// doesn't shadow — under EVERY file policy, including ask-mode with scripted
/// answers.
///
/// We enumerate every file policy over the rich merge fixture and assert that
/// every dest-only file (`keep*`) is byte-identical afterward, every time. The
/// cancel / rollback mid-merge slice of the same invariant lives in the sibling
/// tests `merge_cancel_mid_stream_preserves_unshadowed_dest_files` (this file)
/// and the `cancel_mid_merge_stream_*` / `rollback_mid_merge_stream_*` cases in
/// `volume_copy_rollback_tests.rs`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn merge_never_deletes_unshadowed_dest_files_under_every_policy() {
    // (policy, scripted Stop answer or None, apply_to_all)
    let cases: &[(ConflictResolution, Option<ConflictResolution>)] = &[
        (ConflictResolution::Skip, None),
        (ConflictResolution::Overwrite, None),
        (ConflictResolution::Rename, None),
        (ConflictResolution::OverwriteSmaller, None),
        (ConflictResolution::OverwriteOlder, None),
        (ConflictResolution::Stop, Some(ConflictResolution::Skip)),
        (ConflictResolution::Stop, Some(ConflictResolution::Overwrite)),
        (ConflictResolution::Stop, Some(ConflictResolution::Rename)),
    ];

    for (policy, scripted) in cases {
        let (source, dest) = make_rich_merge().await;
        let state = make_state();

        // The responder sink IS the events sink: it forwards every event to its
        // inner collector and auto-answers any Stop-mode prompt. For non-Stop
        // policies no prompt is ever emitted, so the scripted answer (defaulted
        // to Skip here) never fires — the sink is a plain collector in that case.
        let events = Arc::new(ConflictResponderSink::new(
            &state,
            scripted.unwrap_or(ConflictResolution::Skip),
            true,
        ));
        let config = VolumeCopyConfig {
            conflict_resolution: *policy,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        };

        let result = copy_volumes_with_progress(
            events.clone(),
            &format!("op-invariant-{policy:?}-{scripted:?}"),
            &state,
            Arc::clone(&source),
            &[PathBuf::from("/album")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        assert!(
            result.is_ok(),
            "policy {policy:?}/{scripted:?} should complete, got {result:?}"
        );

        // THE INVARIANT: every dest-only file is byte-identical, every time.
        assert_eq!(
            read_all(&dest, "/album/keep.txt").await,
            b"DEST-keep",
            "policy {policy:?}/{scripted:?}: dest-only /album/keep.txt was clobbered"
        );
        assert_eq!(
            read_all(&dest, "/album/sub/keep2.txt").await,
            b"DEST-keep2",
            "policy {policy:?}/{scripted:?}: dest-only /album/sub/keep2.txt was clobbered"
        );

        // Source-only files always arrive (the merge's whole point).
        assert_eq!(
            read_all(&dest, "/album/fresh.txt").await,
            b"SRC-fresh",
            "policy {policy:?}/{scripted:?}: source-only /album/fresh.txt didn't arrive"
        );
        assert_eq!(
            read_all(&dest, "/album/sub/fresh2.txt").await,
            b"SRC-fresh2",
            "policy {policy:?}/{scripted:?}: source-only /album/sub/fresh2.txt didn't arrive"
        );

        // Zero folder-level prompts under EVERY policy, even Stop.
        assert_eq!(
            folder_conflict_count_both_dirs(&events.inner),
            0,
            "policy {policy:?}/{scripted:?}: a dir-vs-dir merge wrongly emitted a folder conflict"
        );
    }
}

/// Cancel-mid-merge variant of the invariant: flip intent partway through (via
/// the public cancel path, never `state.intent.store` directly), and the
/// dest-only sentinel must still survive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn merge_cancel_mid_stream_preserves_unshadowed_dest_files() {
    let (source, dest) = make_rich_merge().await;
    let state = make_state();

    // Register the op in the global cache so `cancel_write_operation` (the
    // public path) can find and transition it — exercising the real cancel
    // machinery, not a direct intent store.
    let op = TestOperationGuard::register_state("merge-cancel-public", Arc::clone(&state));
    let op_id = op.id();

    // A sink that cancels (public path) once any byte has been copied.
    struct CancelOnByteSink {
        inner: CollectorEventSink,
        op_id: String,
        fired: AtomicU8,
    }
    impl OperationEventSink for CancelOnByteSink {
        fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
            self.inner.emit_settled(e);
        }
        fn emit_progress(&self, event: WriteProgressEvent) {
            if event.phase == WriteOperationPhase::Copying
                && event.bytes_done > 0
                && self.fired.swap(1, Ordering::Relaxed) == 0
            {
                // Public cancel path (Stopped, keep partials).
                cancel_write_operation(&self.op_id, false);
            }
            self.inner.emit_progress(event);
        }
        fn emit_complete(&self, e: WriteCompleteEvent) {
            self.inner.emit_complete(e);
        }
        fn emit_cancelled(&self, e: WriteCancelledEvent) {
            self.inner.emit_cancelled(e);
        }
        fn emit_error(&self, e: WriteErrorEvent) {
            self.inner.emit_error(e);
        }
        fn emit_conflict(&self, e: WriteConflictEvent) {
            self.inner.emit_conflict(e);
        }
        fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
        fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
        fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
    }

    let events = Arc::new(CancelOnByteSink {
        inner: CollectorEventSink::new(),
        op_id: op_id.to_string(),
        fired: AtomicU8::new(0),
    });
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        op_id,
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "expected a cancelled result, got {result:?}");
    // Cancel keeps partials but must never destroy a dest-only file.
    assert_eq!(
        read_all(&dest, "/album/keep.txt").await,
        b"DEST-keep",
        "cancel mid-merge clobbered a dest-only file"
    );
}

// ============================================================================
// Dir-vs-dir always merges, never prompts (top-level AND deep), every policy
// ============================================================================

/// Top-level AND deep dir-vs-dir merges emit ZERO folder conflicts, under every
/// file policy including Stop. Pins that folders merge silently and only files
/// can ever prompt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dir_vs_dir_never_prompts_top_level_or_deep_under_every_policy() {
    for policy in [
        ConflictResolution::Skip,
        ConflictResolution::Overwrite,
        ConflictResolution::Rename,
        ConflictResolution::Stop,
    ] {
        let (source, dest) = make_volumes();
        // Nested dir-vs-dir with NO clashing files anywhere — only folders clash.
        source.create_directory(Path::new("/a")).await.unwrap();
        source.create_directory(Path::new("/a/b")).await.unwrap();
        source.create_file(Path::new("/a/b/only-src.txt"), b"S").await.unwrap();
        dest.create_directory(Path::new("/a")).await.unwrap();
        dest.create_directory(Path::new("/a/b")).await.unwrap();
        dest.create_file(Path::new("/a/b/only-dest.txt"), b"D").await.unwrap();

        let state = make_state();
        let events = Arc::new(CollectorEventSink::new());
        let config = VolumeCopyConfig {
            conflict_resolution: policy,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        };

        let result = copy_volumes_with_progress(
            events.clone(),
            &format!("op-dirdir-{policy:?}"),
            &state,
            Arc::clone(&source),
            &[PathBuf::from("/a")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        assert!(
            result.is_ok(),
            "policy {policy:?}: dir-only merge should complete, got {result:?}"
        );
        // No write-conflict at all — there are no FILE clashes, and folders never prompt.
        assert_eq!(
            events.conflicts.lock().unwrap().len(),
            0,
            "policy {policy:?}: dir-vs-dir merge wrongly emitted a conflict"
        );
        // Both the source-only and dest-only file coexist in the merged tree.
        assert!(
            dest.exists(Path::new("/a/b/only-src.txt")).await,
            "policy {policy:?}: src file missing"
        );
        assert!(
            dest.exists(Path::new("/a/b/only-dest.txt")).await,
            "policy {policy:?}: dest-only file destroyed"
        );
    }
}

// ============================================================================
// "Skip all" merges folders, skips only clashing files (old behavior GONE)
// ============================================================================

/// THE GOTCHA FIX: under Skip, a top-level dir-vs-dir clash used to skip the
/// ENTIRE subtree (the documented `transfer/CLAUDE.md` gotcha). Now it merges:
/// the folder is merged, only the clashing FILE is skipped, and non-clashing
/// source files still arrive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skip_all_merges_folder_and_skips_only_clashing_files() {
    let (source, dest) = make_rich_merge().await;
    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-skip-all-merge",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // The folder MERGED (old behavior would have skipped the whole subtree):
    // non-clashing source files arrived at both depths.
    assert_eq!(read_all(&dest, "/album/fresh.txt").await, b"SRC-fresh");
    assert_eq!(read_all(&dest, "/album/sub/fresh2.txt").await, b"SRC-fresh2");

    // Clashing files were SKIPPED — dest keeps its own bytes.
    assert_eq!(read_all(&dest, "/album/clash.txt").await, b"DEST-clash");
    assert_eq!(read_all(&dest, "/album/sub/clash2.txt").await, b"DEST-clash2");

    // Dest-only files untouched.
    assert_eq!(read_all(&dest, "/album/keep.txt").await, b"DEST-keep");
    assert_eq!(read_all(&dest, "/album/sub/keep2.txt").await, b"DEST-keep2");
}

// ============================================================================
// Stop-mode deep file clash emits a conflict with correct paths/flags, resumes
// ============================================================================

/// A deep file clash under Stop emits a `write-conflict` carrying the right
/// per-file paths (file, not folder) and resumes on response. We answer the deep
/// clash with Overwrite (no apply-to-all) and assert only that one deep file
/// changed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_mode_deep_file_clash_emits_conflict_and_resumes() {
    let (source, dest) = make_volumes();
    // One deep clash, plus a dest-only sibling that must survive.
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/clash.txt"), b"SRC-deep")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_directory(Path::new("/album/sub")).await.unwrap();
    dest.create_file(Path::new("/album/sub/clash.txt"), b"DEST-deep")
        .await
        .unwrap();
    dest.create_file(Path::new("/album/sub/keep.txt"), b"DEST-keep")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-stop-deep",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
    // The sink-recorded file prompts ARE the race-free count once the op future
    // has completed — exactly one Stop prompt for the deep clash.
    assert_eq!(
        file_conflict_count(&events.inner),
        1,
        "exactly one Stop prompt expected for the deep clash"
    );

    // Exactly one conflict, and it's a FILE clash with the right deep paths.
    // Clone the fields we need out of the guarded vec in a tight scope so the
    // lock guard is fully dropped before the awaits below (clippy
    // `await_holding_lock` flags a guard alive across `.await` even with an
    // explicit `drop`, so we end the borrow by leaving the block instead).
    let (src_path, dst_path, src_is_dir, dst_is_dir, n_conflicts) = {
        let conflicts = events.inner.conflicts.lock().unwrap();
        let c = conflicts.first().expect("exactly one deep file conflict");
        (
            c.source_path.clone(),
            c.destination_path.clone(),
            c.source_is_directory,
            c.destination_is_directory,
            conflicts.len(),
        )
    };
    assert_eq!(n_conflicts, 1, "exactly one deep file conflict");
    assert!(src_path.ends_with("clash.txt"), "conflict source path: {src_path}");
    assert!(dst_path.ends_with("clash.txt"), "conflict dest path: {dst_path}");
    assert!(!src_is_dir && !dst_is_dir, "deep clash is file-vs-file");

    // Overwrite applied to the deep clash; dest-only sibling untouched.
    assert_eq!(read_all(&dest, "/album/sub/clash.txt").await, b"SRC-deep");
    assert_eq!(read_all(&dest, "/album/sub/keep.txt").await, b"DEST-keep");
}

// ============================================================================
// Byte-total accounting through the merge recursion
// ============================================================================

/// The merge's returned byte total — which flows into the complete event's
/// `bytes_processed` — must be the exact sum of every file written across all
/// merged levels. Distinct, non-trivial per-file sizes make any
/// accumulation-operator corruption (`+=` → `*=` / `-=`) produce a wrong total.
/// Overwrite so every clashing file is also written (counts toward the sum).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn merge_reports_exact_byte_total_across_levels() {
    let (source, dest) = make_volumes();
    // Source: top-level file (7), a deep file (11), and a deeper file (13).
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_file(Path::new("/album/a.txt"), &[1u8; 7]).await.unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/b.txt"), &[2u8; 11])
        .await
        .unwrap();
    source.create_directory(Path::new("/album/sub/deep")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/deep/c.txt"), &[3u8; 13])
        .await
        .unwrap();
    // Dest pre-exists at every level so all three levels MERGE (each takes the
    // AlreadyExists branch and the byte total accumulates through recursion).
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_directory(Path::new("/album/sub")).await.unwrap();
    dest.create_directory(Path::new("/album/sub/deep")).await.unwrap();
    // A clashing dest file at the deepest level, Overwrite ⇒ written ⇒ counted.
    dest.create_file(Path::new("/album/sub/deep/c.txt"), b"old")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-byte-total",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    let complete = events.complete.lock().unwrap();
    let total = complete.first().expect("a complete event").bytes_processed;
    assert_eq!(
        total,
        7 + 11 + 13,
        "merge must report the exact summed byte total, got {total}"
    );
}

// ============================================================================
// Type mismatch: source DIR vs dest FILE inside a merge
// ============================================================================

/// A source SUBDIRECTORY clashing with a same-named dest FILE is a type
/// mismatch, NOT a dir-vs-dir merge: it routes through the resolver. Under
/// Overwrite the dest file is replaced by the incoming directory; the dest-only
/// sibling survives. Pins the `dir_clashes_with_file` branch (source-dir-vs-
/// dest-file) distinct from the dir-vs-dir recurse path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_dir_over_dest_file_overwrite_replaces_file_with_dir() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    // source `D` is a DIRECTORY holding a file.
    source.create_directory(Path::new("/album/D")).await.unwrap();
    source
        .create_file(Path::new("/album/D/inner.txt"), b"SRC-inner")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    // dest `D` is a FILE (the type mismatch), plus a dest-only sibling.
    dest.create_file(Path::new("/album/D"), b"DEST-file").await.unwrap();
    dest.create_file(Path::new("/album/keep.txt"), b"DEST-keep")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-dir-over-file-overwrite",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // The dest FILE was replaced by the incoming DIRECTORY (type-mismatch
    // Overwrite), and the directory's content landed.
    assert!(
        dest.is_directory(Path::new("/album/D")).await.unwrap_or(false),
        "dest `D` should now be a directory"
    );
    assert_eq!(read_all(&dest, "/album/D/inner.txt").await, b"SRC-inner");
    // Dest-only sibling untouched.
    assert_eq!(read_all(&dest, "/album/keep.txt").await, b"DEST-keep");

    // The byte total flows through the type-mismatch dir recurse branch:
    // `inner.txt` is 9 bytes ("SRC-inner"). Asserting the exact total pins that
    // branch's accumulation (a `*=`/`-=` corruption would zero/wrap it).
    let total = events
        .complete
        .lock()
        .unwrap()
        .first()
        .expect("a complete event")
        .bytes_processed;
    assert_eq!(
        total, 9,
        "type-mismatch dir merge must report the directory's byte total, got {total}"
    );
}

/// Same source-dir-vs-dest-file type mismatch, but under Skip: the dest FILE
/// stays untouched (the directory is NOT merged over it). Pins the other side of
/// the `dir_clashes_with_file` branch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_dir_over_dest_file_skip_keeps_dest_file() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/D")).await.unwrap();
    source
        .create_file(Path::new("/album/D/inner.txt"), b"SRC-inner")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/D"), b"DEST-file").await.unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-dir-over-file-skip",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // Skip honored: dest `D` is still the original FILE, unchanged.
    assert!(
        !dest.is_directory(Path::new("/album/D")).await.unwrap_or(false),
        "dest `D` must remain a file under Skip"
    );
    assert_eq!(read_all(&dest, "/album/D").await, b"DEST-file");
}

// ============================================================================
// Conflict-dispatch mutex
// ============================================================================

/// Concurrent merge with two deep clashes (across two top-level sources, taking
/// the FuturesUnordered concurrent path) under Stop: the dispatch mutex
/// serializes the prompts so each one gets answered through the single oneshot
/// slot. Both clashes resolve, both dest-only files survive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_merge_with_two_deep_clashes_serializes_prompts() {
    let (source, dest) = make_volumes();

    // 3 top-level sources ⇒ concurrent path (>=3 and InMemory max_concurrent=32).
    // Two of them are merging dirs that each hide a deep file clash; the third is
    // a plain fresh file so the batch is unambiguously concurrent.
    for d in ["one", "two"] {
        source.create_directory(Path::new(&format!("/{d}"))).await.unwrap();
        source
            .create_file(Path::new(&format!("/{d}/clash.bin")), &vec![1u8; 50_000])
            .await
            .unwrap();
        dest.create_directory(Path::new(&format!("/{d}"))).await.unwrap();
        dest.create_file(Path::new(&format!("/{d}/clash.bin")), &vec![9u8; 50_000])
            .await
            .unwrap();
        dest.create_file(Path::new(&format!("/{d}/keep.txt")), b"KEEP")
            .await
            .unwrap();
    }
    source.create_file(Path::new("/three.txt"), b"THREE").await.unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-concurrent-two-clashes",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/one"),
            PathBuf::from("/two"),
            PathBuf::from("/three.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
    // Sink-derived: both deep clashes prompted (the dispatch mutex serialized
    // them through the single oneshot slot, each answered in turn).
    let n = file_conflict_count(&events.inner);
    assert_eq!(n, 2, "both deep clashes should prompt and be answered, got {n}");

    // Both clashes overwritten (50_000 bytes of 1u8), both dest-only files kept.
    for d in ["one", "two"] {
        assert_eq!(read_all(&dest, &format!("/{d}/clash.bin")).await, vec![1u8; 50_000]);
        assert_eq!(read_all(&dest, &format!("/{d}/keep.txt")).await, b"KEEP");
    }
    assert!(dest.exists(Path::new("/three.txt")).await);
}

/// Top-level vs deep race: a top-level file clash and a deep file clash (inside a
/// merging dir) both surface under Stop on the concurrent path. The SAME dispatch
/// mutex guards both, so neither clobbers the other's oneshot. We answer both
/// with "…all" Overwrite; everything overwrites, dest-only file survives.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn top_level_and_deep_clash_share_the_dispatch_mutex() {
    let (source, dest) = make_volumes();

    // Top-level file clash.
    source.create_file(Path::new("/top.txt"), b"SRC-top").await.unwrap();
    dest.create_file(Path::new("/top.txt"), b"DEST-top").await.unwrap();
    // A merging dir with a deep file clash + dest-only file.
    source.create_directory(Path::new("/dir")).await.unwrap();
    source
        .create_file(Path::new("/dir/clash.txt"), b"SRC-deep")
        .await
        .unwrap();
    dest.create_directory(Path::new("/dir")).await.unwrap();
    dest.create_file(Path::new("/dir/clash.txt"), b"DEST-deep")
        .await
        .unwrap();
    dest.create_file(Path::new("/dir/keep.txt"), b"KEEP").await.unwrap();
    // Third source to force the concurrent path.
    source.create_file(Path::new("/extra.txt"), b"EXTRA").await.unwrap();

    let state = make_state();
    // One "…all" answer collapses any queued prompt via the latch double-check,
    // so at most 2 prompts ever emit (top + deep), often just 1.
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, true));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-top-vs-deep",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/top.txt"),
            PathBuf::from("/dir"),
            PathBuf::from("/extra.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // Both clashes overwritten by the "…all" choice; dest-only file survives.
    assert_eq!(read_all(&dest, "/top.txt").await, b"SRC-top");
    assert_eq!(read_all(&dest, "/dir/clash.txt").await, b"SRC-deep");
    assert_eq!(read_all(&dest, "/dir/keep.txt").await, b"KEEP");
}

/// Cancel-while-queued: task A is awaiting a Stop prompt while task B is parked
/// on the dispatch mutex. Cancel must unblock BOTH and return without a hang
/// (task B's cancel-check inside the resolver bails before emitting a prompt no
/// one would answer). The op terminates (the test simply completing is the
/// no-hang assertion).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_while_queued_unblocks_both_no_hang() {
    let (source, dest) = make_volumes();

    // Two merging dirs that each hide a deep file clash, plus a third source for
    // the concurrent path. Both deep clashes hit Stop; one task ends up parked on
    // the dispatch mutex behind the other.
    for d in ["one", "two"] {
        source.create_directory(Path::new(&format!("/{d}"))).await.unwrap();
        source
            .create_file(Path::new(&format!("/{d}/clash.bin")), &vec![1u8; 50_000])
            .await
            .unwrap();
        dest.create_directory(Path::new(&format!("/{d}"))).await.unwrap();
        dest.create_file(Path::new(&format!("/{d}/clash.bin")), &vec![9u8; 50_000])
            .await
            .unwrap();
    }
    source.create_file(Path::new("/three.txt"), b"THREE").await.unwrap();

    let state = make_state();
    let op = TestOperationGuard::register_state("cancel-while-queued", Arc::clone(&state));
    let op_id = op.id();

    // Wait until the FIRST prompt is installed (proving task A is awaiting and
    // task B is queued on the mutex), then cancel via the public path. Cancelling
    // drops the oneshot sender (unblocking A) and flips intent (so B's in-resolver
    // cancel-check bails). Neither re-emits.
    let state_for_cancel = Arc::clone(&state);
    let op_id_for_cancel = op_id.to_string();
    let canceller = tokio::spawn(async move {
        for _ in 0..400 {
            // A sender is installed while a task awaits the prompt.
            if state_for_cancel.conflict_resolution_tx.lock().unwrap().is_some() {
                tokio::time::sleep(Duration::from_millis(20)).await; // let task B park on the mutex
                cancel_write_operation(&op_id_for_cancel, false);
                return true;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        false
    });

    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    // The whole op must terminate. A 20 s timeout converts a hang into a test
    // failure instead of wedging the suite.
    let driven = tokio::time::timeout(
        Duration::from_secs(20),
        copy_volumes_with_progress(
            events.clone(),
            op_id,
            &state,
            Arc::clone(&source),
            &[
                PathBuf::from("/one"),
                PathBuf::from("/two"),
                PathBuf::from("/three.txt"),
            ],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        ),
    )
    .await;

    let installed = canceller.await.unwrap();
    assert!(installed, "a Stop prompt should have been installed before cancel");
    assert!(
        driven.is_ok(),
        "operation hung after cancel-while-queued (dispatch-mutex deadlock)"
    );
    // Cancelled → the op returns Err(Cancelled). Dest-only nothing here; the
    // no-hang completion is the assertion that matters.
    let result = driven.unwrap();
    assert!(result.is_err(), "cancelled op should return Err, got {result:?}");
}

/// A type mismatch inside a merge (source FILE vs dest DIRECTORY) is NOT a merge:
/// it routes through the resolver. Under Skip it leaves the dest dir untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn type_mismatch_inside_merge_routes_through_resolver_and_skip_keeps_dest() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    // source `swap` is a FILE.
    source.create_file(Path::new("/album/swap"), b"SRC-file").await.unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    // dest `swap` is a DIR holding a file.
    dest.create_directory(Path::new("/album/swap")).await.unwrap();
    dest.create_file(Path::new("/album/swap/inner.txt"), b"DEST-inner")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-type-mismatch-skip",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // Skip honored: the dest DIR and its inner file survive untouched.
    assert!(dest.is_directory(Path::new("/album/swap")).await.unwrap_or(false));
    assert_eq!(read_all(&dest, "/album/swap/inner.txt").await, b"DEST-inner");
}

/// A freshly-created dest level (no pre-existing dir) skips the dest listing and
/// streams every child straight in — proving the `Ok(())` create branch never
/// lists or prompts. Asserts via a counting volume that `list_directory` is
/// called on the DEST only for source enumeration parity, never for a clash map
/// on a fresh level.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fresh_dest_level_streams_without_listing_or_prompting() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/brand-new")).await.unwrap();
    source.create_file(Path::new("/brand-new/a.txt"), b"A").await.unwrap();
    source.create_directory(Path::new("/brand-new/sub")).await.unwrap();
    source
        .create_file(Path::new("/brand-new/sub/b.txt"), b"B")
        .await
        .unwrap();
    // dest has NO `/brand-new`.

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        // Even under Stop, a fresh level can't clash, so nothing prompts.
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-fresh-level",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/brand-new")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        events.conflicts.lock().unwrap().len(),
        0,
        "a fresh level must never prompt"
    );
    assert_eq!(read_all(&dest, "/brand-new/a.txt").await, b"A");
    assert_eq!(read_all(&dest, "/brand-new/sub/b.txt").await, b"B");
}
