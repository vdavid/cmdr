//! The coverage grid: the intersections nobody thinks to write by hand.
//!
//! Every cell asserts through `safety_oracle.rs`. The value here is not the
//! count, it's that the grid covers the exact intersection the cross-volume copy
//! bug lived in — a directory, merging into the user's own folder, with a
//! half-populated scan cache, interrupted.
//!
//! ## Tier A — exhaustive, the blast-radius cell (27)
//!
//! `{copy, move, delete}` × cache state `{miss, hit-with-per-path,
//! hit-without-per-path}` × outcome `{clean, fail-mid-op, cancel-mid-op}`. One
//! test function per (op × outcome), each looping the three cache states, so a
//! failure names itself and stays greppable.
//!
//! **The item kind is per-op, not one label.** For copy and move it's
//! `dir-onto-an-existing-dir (merge)`. "Merge" has no meaning for DELETE, which
//! has no destination: delete's nine cells are
//! `dir-with-mixed-contents-plus-a-sibling-the-op-was-not-asked-to-touch`, and
//! their oracle degenerates to clause 1 alone, read as "nothing outside the
//! requested set is gone". Nine cells on a different axis under the same column
//! heading would be a silent cap, so delete's driver states its own assertions
//! rather than handing the shared helper an empty list and calling it covered.
//!
//! **Pipeline**: cross-volume-serial throughout. That's the honest volume axis
//! for in-memory doubles — what forks the production code is
//! `operations_are_local()`, `max_concurrent_ops() > 1`, and
//! `Arc::ptr_eq(src, dst)`, not a backend name. A "local→SMB" cell against
//! doubles is the same cell as "local→MTP" wearing a different name.
//!
//! ## Tier B — sampled, the shape axis (12)
//!
//! `{copy, move}` × item kind `{file, dir-onto-fresh-dest,
//! dir-onto-an-existing-file}` × driver `{serial, concurrent}`, with the cache
//! pinned to `hit-without-per-path`: the other two states are already covered by
//! the existing suites, and that one is the shape that used to be a lie.
//!
//! ## Tier C — already covered, not rebuilt
//!
//! The full conflict-policy matrix stays in `merge_tests.rs` and
//! `move_merge_tests.rs`, now running through the same oracle.
//!
//! ## Explicitly NOT covered, and why
//!
//! - **rename, compress, and trash × cache state.** They consume no preview
//!   cache at all (`insert_scan_result` has two production call sites, both in
//!   `scan_preview.rs`; `preview_id` never reaches `archive_edit/`,
//!   `delete/trash.rs`, or `rename.rs`). Their safety properties belong to their
//!   own suites.
//! - **`hit-with-per-path` versus `hit-without-per-path` on the LOCAL copy and
//!   move pipelines.** Neither `transfer/copy/mod.rs` nor `transfer/move_op.rs`
//!   reads `per_path` anywhere, so those two would be one cell wearing two
//!   names. The distinction is real only on the volume pipelines, which is where
//!   this grid puts it. ❌ Don't add a local cell that "asserts the miss and the
//!   fresh rescan" either — a per-path-less local cache is still a HIT.
//! - **remote↔remote against real backends.** Four cells where a real share
//!   genuinely can't be stood in for live in `backends/smb_transfer_safety_test.rs`.
//! - **MTP.** The virtual device is an E2E-only fixture, and the driver
//!   difference MTP exercises (`max_concurrent_ops() == 1`) IS the
//!   cross-volume-serial column, which is covered.
//! - **symlink-to-directory.** `InMemoryVolume` has no symlinks, and the rule
//!   ("a transfer copies the LINK, never dereferences it") lives in the LOCAL
//!   walker, `scan.rs`, where the local scan tests pin it. Faking it on a double
//!   would assert the double, not the code.
//! - **hardlinks and inode fidelity.** Same reason: no inodes in the double.
//! - **permission-denied and quota faults.** The backend decides those, not the
//!   driver; injecting them tests `FaultyVolume`, not the code under it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use super::faulty_volume::{FaultyOp, FaultyVolume};
use super::safety_oracle::{SafetySpec, assert_operation_was_safe, collect_contents, try_read_all};
use super::{copy_volumes_with_progress, move_volumes_with_progress};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{CopyScanResult, InMemoryVolume, Volume, VolumeError};
use crate::file_system::write_operations::delete::delete_volume_files_with_progress_inner;
use crate::file_system::write_operations::event_sinks::{CollectorEventSink, OperationEventSink};
use crate::file_system::write_operations::scan_cache::seed_incoherent_scan_result_for_test;
use crate::file_system::write_operations::state::{
    CachedScanResult, WriteOperationState, cancel_write_operation, insert_scan_result,
};
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{
    ConflictResolution, VolumeCopyConfig, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationPhase, WriteProgressEvent,
    WriteSourceItemDoneEvent,
};

// ============================================================================
// The axes
// ============================================================================

/// What the scan-preview cache holds when the operation starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CacheState {
    /// No `preview_id`: the operation scans for itself.
    Miss,
    /// A completed volume-batch preview carrying one `CopyScanResult` per
    /// source — the shape the drivers are happiest with.
    HitWithPerPath,
    /// A completed preview that counted files but recorded NO per-source
    /// result. This is the shape the LOCAL walk emitted for three months, and
    /// the one that made every driver read "no information" as a confident
    /// `is_directory: false`.
    HitWithoutPerPath,
}

impl CacheState {
    fn label(self) -> &'static str {
        match self {
            CacheState::Miss => "cache-miss",
            CacheState::HitWithPerPath => "cache-hit-with-per-path",
            CacheState::HitWithoutPerPath => "cache-hit-without-per-path",
        }
    }
}

const CACHE_STATES: &[CacheState] = &[
    CacheState::Miss,
    CacheState::HitWithPerPath,
    CacheState::HitWithoutPerPath,
];

// ============================================================================
// Fixtures
// ============================================================================

/// A zero progress interval, so EVERY file emits a progress event.
///
/// ❗ Load-bearing for the cancel cells, not cosmetic: the delete walker gates
/// its emission on `state.progress_interval`, so at the 50 ms default a small
/// fixture finishes without ever emitting one, the cancelling sink never fires,
/// and nine "cancel-mid-op" cells quietly run to completion.
fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

/// A unique preview id per cell, so no two cells share a cache entry.
fn preview_id_for(cell: &str) -> String {
    format!("safety-grid-{cell}-{}", uuid::Uuid::new_v4())
}

/// Seeds the cache for `state` and returns the `preview_id` to configure with.
///
/// `HitWithoutPerPath` goes through the narrowly-named incoherent seeder on
/// purpose: `insert_scan_result`'s canary is a `debug_assert!`, so a release
/// build still admits the shape and the drivers still have to survive it.
fn seed_cache(state: CacheState, cell: &str, sources: &[&str], file_count: usize, bytes: u64) -> Option<String> {
    match state {
        CacheState::Miss => None,
        CacheState::HitWithPerPath => {
            let id = preview_id_for(cell);
            let paths: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
            let per_path = paths
                .iter()
                .map(|p| {
                    (
                        p.clone(),
                        CopyScanResult {
                            file_count,
                            dir_count: 1,
                            total_bytes: bytes,
                            dedup_bytes: bytes,
                            top_level_is_directory: true,
                        },
                    )
                })
                .collect();
            insert_scan_result(
                id.clone(),
                CachedScanResult::from_volume_batch(paths, file_count, bytes, bytes, per_path),
            );
            Some(id)
        }
        CacheState::HitWithoutPerPath => {
            let id = preview_id_for(cell);
            seed_incoherent_scan_result_for_test(
                id.clone(),
                sources.iter().map(PathBuf::from).collect(),
                file_count,
                bytes,
            );
            Some(id)
        }
    }
}

/// The Tier A transfer fixture: a source `/album` merging onto a destination
/// `/album` the user already had, with a dest-only sentinel at each level that
/// no outcome may touch.
async fn tier_a_source() -> Arc<InMemoryVolume> {
    let vol = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/one.bin"), &vec![0xA1; 2048])
        .await
        .unwrap();
    vol.create_file(Path::new("/album/two.bin"), &vec![0xA2; 2048])
        .await
        .unwrap();
    vol.create_directory(Path::new("/album/inner")).await.unwrap();
    vol.create_file(Path::new("/album/inner/three.bin"), &vec![0xA3; 2048])
        .await
        .unwrap();
    vol
}

async fn tier_a_dest() -> Arc<dyn Volume> {
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/keep.txt"), b"DEST-keep")
        .await
        .unwrap();
    vol.create_directory(Path::new("/album/inner")).await.unwrap();
    vol.create_file(Path::new("/album/inner/keep2.txt"), b"DEST-keep2")
        .await
        .unwrap();
    vol
}

/// Every source file the Tier A fixture holds, relative to `/album`.
const TIER_A_SOURCE_FILES: &[(&str, &[u8])] = &[
    ("/one.bin", &[0xA1; 2048]),
    ("/two.bin", &[0xA2; 2048]),
    ("/inner/three.bin", &[0xA3; 2048]),
];

/// The dest-only files, which NO cell may touch. This is the blast radius the
/// original bug destroyed.
const TIER_A_UNTOUCHED_DEST: &[(&str, &[u8])] = &[("/keep.txt", b"DEST-keep"), ("/inner/keep2.txt", b"DEST-keep2")];

/// A sink that cancels its operation through the PUBLIC path the moment the
/// first byte moves, so the cell interrupts real work rather than racing a
/// timer. Mirrors `merge_tests.rs::CancelOnByteSink`; kept local because the
/// grid also needs it for the Deleting phase.
struct CancelOnFirstProgressSink {
    inner: CollectorEventSink,
    op_id: String,
    fired: AtomicU8,
}

impl OperationEventSink for CancelOnFirstProgressSink {
    fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_progress(&self, event: WriteProgressEvent) {
        let working = matches!(
            event.phase,
            WriteOperationPhase::Copying | WriteOperationPhase::Deleting
        );
        if working && (event.bytes_done > 0 || event.files_done > 0) && self.fired.swap(1, Ordering::Relaxed) == 0 {
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
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
}

/// The read failure a fail-mid-op cell injects: the first source read gives up,
/// so a DIRECTORY source dies partway through its own subtree — the only way to
/// reach the post-loop partial sweep with a directory in play.
fn read_gave_up() -> VolumeError {
    VolumeError::IoError {
        message: "Injected read failure".into(),
        raw_os_error: Some(5), // EIO
    }
}

/// A cell that arms a fault or a cancel has to actually get one: a grid whose
/// interrupted cells quietly ran to completion would be 18 real cells and nine
/// shrugs, which is the silent cap this whole tier exists to avoid.
fn assert_interruption_landed(label: &str, interrupted: bool, failed: bool) {
    if interrupted {
        assert!(
            failed,
            "{label}: the cell armed an interruption and the operation completed anyway"
        );
    } else {
        assert!(!failed, "{label}: a clean cell should complete");
    }
}

/// The Tier A oracle spec. `delivered` is empty for the interrupted outcomes:
/// a cancelled or failed transfer legitimately doesn't deliver everything, and
/// claiming otherwise would be a test asserting the wrong promise. What must
/// hold in EVERY outcome is clause 1 (nothing gone from both sides) and clause 3
/// (the dest-only files are untouched).
fn tier_a_spec<'a>(label: &'a str, expect_delivery: bool) -> SafetySpec<'a> {
    SafetySpec {
        label,
        source_root: "/album",
        dest_root: "/album",
        source_files: TIER_A_SOURCE_FILES,
        delivered: if expect_delivery { TIER_A_SOURCE_FILES } else { &[] },
        untouched_dest: TIER_A_UNTOUCHED_DEST,
    }
}

// ============================================================================
// Tier A — copy
// ============================================================================

/// Runs one Tier A COPY cell and asserts the oracle over it.
async fn run_tier_a_copy(cache: CacheState, cell: &str, faulty: bool, cancel: bool) {
    let inner_source = tier_a_source().await;
    let source: Arc<dyn Volume> = if faulty {
        FaultyVolume::wrapping(Arc::clone(&inner_source))
            .failing_call(FaultyOp::OpenReadStream, 1, read_gave_up())
            .arc()
    } else {
        Arc::clone(&inner_source) as Arc<dyn Volume>
    };
    let dest = tier_a_dest().await;

    let label = format!("{cell}/{}", cache.label());
    let preview_id = seed_cache(cache, cell, &["/album"], 3, 6144);
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        preview_id,
        ..VolumeCopyConfig::default()
    };

    let op_id = format!("grid-{cell}-{}", cache.label());
    let guard = cancel.then(|| TestOperationGuard::register_state(&op_id, Arc::clone(&state)));
    let events = Arc::new(CancelOnFirstProgressSink {
        inner: CollectorEventSink::new(),
        op_id: guard.as_ref().map_or_else(|| op_id.clone(), |g| g.id().to_string()),
        fired: AtomicU8::new(if cancel { 0 } else { 1 }),
    });

    let result = copy_volumes_with_progress(
        events,
        guard.as_ref().map_or(op_id.as_str(), |g| g.id()),
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert_interruption_landed(&label, faulty || cancel, result.is_err());

    // The source side of clause 1 reads the REAL volume, not the lying wrapper.
    let real_source: Arc<dyn Volume> = Arc::clone(&inner_source) as Arc<dyn Volume>;
    assert_operation_was_safe(&real_source, &dest, &tier_a_spec(&label, !faulty && !cancel)).await;
}

/// Tier A, copy, CLEAN: a merge onto the user's own folder delivers everything
/// and touches none of their files, whatever the cache held.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_a_copy_merge_is_safe_when_it_completes() {
    for cache in CACHE_STATES {
        run_tier_a_copy(*cache, "copy-clean", false, false).await;
    }
}

/// Tier A, copy, FAIL MID-OP. This is the exact cell the production bug lived
/// in: a directory source, merging into the user's folder, whose subtree stream
/// dies partway — and whose cleanup then decided what to sweep from a belief
/// about the source's type that a half-populated cache had corrupted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_a_copy_merge_is_safe_when_it_fails_mid_op() {
    for cache in CACHE_STATES {
        run_tier_a_copy(*cache, "copy-fail", true, false).await;
    }
}

/// Tier A, copy, CANCEL MID-OP. Cancel keeps partials by design, so the promise
/// under test is the other one: it must never take a dest-only file with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_a_copy_merge_is_safe_when_it_is_cancelled_mid_op() {
    for cache in CACHE_STATES {
        run_tier_a_copy(*cache, "copy-cancel", false, true).await;
    }
}

// ============================================================================
// Tier A — move
// ============================================================================

/// Runs one Tier A MOVE cell. A move is copy-then-delete-source, so clause 1
/// carries real weight here: a file gone from both sides is destroyed data.
async fn run_tier_a_move(cache: CacheState, cell: &str, faulty: bool, cancel: bool) {
    let inner_source = tier_a_source().await;
    let source: Arc<dyn Volume> = if faulty {
        FaultyVolume::wrapping(Arc::clone(&inner_source))
            .failing_call(FaultyOp::OpenReadStream, 1, read_gave_up())
            .arc()
    } else {
        Arc::clone(&inner_source) as Arc<dyn Volume>
    };
    let dest = tier_a_dest().await;

    let label = format!("{cell}/{}", cache.label());
    let preview_id = seed_cache(cache, cell, &["/album"], 3, 6144);
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        preview_id,
        ..VolumeCopyConfig::default()
    };

    let op_id = format!("grid-{cell}-{}", cache.label());
    let guard = cancel.then(|| TestOperationGuard::register_state(&op_id, Arc::clone(&state)));
    let events = Arc::new(CancelOnFirstProgressSink {
        inner: CollectorEventSink::new(),
        op_id: guard.as_ref().map_or_else(|| op_id.clone(), |g| g.id().to_string()),
        fired: AtomicU8::new(if cancel { 0 } else { 1 }),
    });

    let result = move_volumes_with_progress(
        events,
        guard.as_ref().map_or(op_id.as_str(), |g| g.id()),
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert_interruption_landed(&label, faulty || cancel, result.is_err());

    let real_source: Arc<dyn Volume> = Arc::clone(&inner_source) as Arc<dyn Volume>;
    assert_operation_was_safe(&real_source, &dest, &tier_a_spec(&label, !faulty && !cancel)).await;
}

/// Tier A, move, CLEAN.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_a_move_merge_is_safe_when_it_completes() {
    for cache in CACHE_STATES {
        run_tier_a_move(*cache, "move-clean", false, false).await;
    }
}

/// Tier A, move, FAIL MID-OP: the source sweep must not run ahead of what
/// actually landed, whatever the cache claimed about the source's shape.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_a_move_merge_is_safe_when_it_fails_mid_op() {
    for cache in CACHE_STATES {
        run_tier_a_move(*cache, "move-fail", true, false).await;
    }
}

/// Tier A, move, CANCEL MID-OP.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_a_move_merge_is_safe_when_it_is_cancelled_mid_op() {
    for cache in CACHE_STATES {
        run_tier_a_move(*cache, "move-cancel", false, true).await;
    }
}

// ============================================================================
// Tier A — delete
// ============================================================================
//
// ❗ Delete has NO DESTINATION, so the oracle's clause 2 ("every byte the user
// approved is at the destination") has nothing to mean here, and clause 3 (the
// merge invariant) has no merge. What's left is clause 1, read as "nothing
// outside the requested set is gone" — which is why these cells assert it
// directly instead of handing the shared helper two empty lists and calling
// nine cells covered.

/// The delete fixture: a requested tree with mixed contents, plus `/other`, a
/// sibling the operation was NEVER asked to touch. `/other` is the assertion.
async fn tier_a_delete_volume() -> Arc<InMemoryVolume> {
    let vol = Arc::new(InMemoryVolume::new("DeleteTarget").with_space_info(10_000_000, 10_000_000));
    vol.create_directory(Path::new("/requested")).await.unwrap();
    vol.create_directory(Path::new("/requested/nested")).await.unwrap();
    // Enough leaves that a cancel landing after the first progress event still
    // has work left to refuse; a two-file tree would be gone either way.
    for i in 0..8 {
        vol.create_file(&PathBuf::from(format!("/requested/a{i}.bin")), &vec![0xD1; 1024])
            .await
            .unwrap();
        vol.create_file(&PathBuf::from(format!("/requested/nested/b{i}.bin")), &vec![0xD2; 1024])
            .await
            .unwrap();
    }

    // The sibling the op was not asked to touch, at two depths.
    vol.create_directory(Path::new("/other")).await.unwrap();
    vol.create_file(Path::new("/other/untouched.txt"), b"OTHER-untouched")
        .await
        .unwrap();
    vol.create_directory(Path::new("/other/deep")).await.unwrap();
    vol.create_file(Path::new("/other/deep/untouched2.txt"), b"OTHER-untouched2")
        .await
        .unwrap();
    vol
}

/// Runs one Tier A DELETE cell and asserts the degenerate oracle over it.
async fn run_tier_a_delete(cache: CacheState, cell: &str, faulty: bool, cancel: bool) {
    let inner = tier_a_delete_volume().await;
    let volume: Arc<dyn Volume> = if faulty {
        FaultyVolume::wrapping(Arc::clone(&inner))
            .failing_call(FaultyOp::Delete, 1, read_gave_up())
            .arc()
    } else {
        Arc::clone(&inner) as Arc<dyn Volume>
    };

    let label = format!("{cell}/{}", cache.label());
    let volume_id = format!("safety-grid-{cell}-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(&volume_id, Arc::clone(&volume));

    let preview_id = seed_cache(cache, cell, &["/requested"], 16, 16384);
    let state = make_state();
    let config = WriteOperationConfig {
        progress_interval_ms: 0,
        preview_id,
        ..WriteOperationConfig::default()
    };

    let op_id = format!("grid-{cell}-{}", cache.label());
    let guard = cancel.then(|| TestOperationGuard::register_state(&op_id, Arc::clone(&state)));
    let events = Arc::new(CancelOnFirstProgressSink {
        inner: CollectorEventSink::new(),
        op_id: guard.as_ref().map_or_else(|| op_id.clone(), |g| g.id().to_string()),
        fired: AtomicU8::new(if cancel { 0 } else { 1 }),
    });

    let result = delete_volume_files_with_progress_inner(
        Arc::clone(&volume),
        &volume_id,
        events.as_ref(),
        guard.as_ref().map_or(op_id.as_str(), |g| g.id()),
        &state,
        &[PathBuf::from("/requested")],
        &config,
    )
    .await;
    assert_interruption_landed(&label, faulty || cancel, result.is_err());

    get_volume_manager().unregister(&volume_id);

    // ❗ THE ONLY CLAUSE THAT MEANS ANYTHING FOR DELETE: nothing outside the
    // requested set is gone. Read on the real volume, not the lying wrapper.
    let real: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    assert_eq!(
        try_read_all(&real, "/other/untouched.txt").await.as_deref(),
        Some(&b"OTHER-untouched"[..]),
        "{label}: a delete removed a sibling it was never asked to touch"
    );
    assert_eq!(
        try_read_all(&real, "/other/deep/untouched2.txt").await.as_deref(),
        Some(&b"OTHER-untouched2"[..]),
        "{label}: a delete reached into a sibling's subtree"
    );
    assert_eq!(
        collect_contents(&real, "/other").await.len(),
        2,
        "{label}: the untouched sibling gained or lost a file"
    );

    // A clean run also has to actually do the work it was asked for. The
    // interrupted cells make no such promise, by design.
    if !faulty && !cancel {
        assert!(
            !real.exists(Path::new("/requested/a0.bin")).await,
            "{label}: a clean delete left the requested tree behind"
        );
        assert!(
            !real.exists(Path::new("/requested/nested/b7.bin")).await,
            "{label}: a clean delete left a nested requested file behind"
        );
    }
}

/// Tier A, delete, CLEAN.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_a_delete_is_safe_when_it_completes() {
    for cache in CACHE_STATES {
        run_tier_a_delete(*cache, "delete-clean", false, false).await;
    }
}

/// Tier A, delete, FAIL MID-OP. Delete is the operation with no rollback, so a
/// wrong or missing fact costs the most here.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_a_delete_is_safe_when_it_fails_mid_op() {
    for cache in CACHE_STATES {
        run_tier_a_delete(*cache, "delete-fail", true, false).await;
    }
}

/// Tier A, delete, CANCEL MID-OP.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_a_delete_is_safe_when_it_is_cancelled_mid_op() {
    for cache in CACHE_STATES {
        run_tier_a_delete(*cache, "delete-cancel", false, true).await;
    }
}

// ============================================================================
// Tier B — the shape axis, against a per-path-less cache
// ============================================================================

/// The three source shapes Tier B drives, each paired with the destination
/// state it lands on.
#[derive(Clone, Copy, Debug)]
enum ItemKind {
    /// A plain file onto a fresh destination.
    File,
    /// A directory onto a destination that has no such name yet.
    DirOntoFreshDest,
    /// A directory onto a destination FILE of the same name: the cross-type
    /// clash, where a wrong answer about the source's type picks the branch
    /// that replaces the destination wholesale.
    DirOntoExistingFile,
}

impl ItemKind {
    fn label(self) -> &'static str {
        match self {
            ItemKind::File => "file",
            ItemKind::DirOntoFreshDest => "dir-onto-fresh-dest",
            ItemKind::DirOntoExistingFile => "dir-onto-existing-file",
        }
    }
}

const ITEM_KINDS: &[ItemKind] = &[
    ItemKind::File,
    ItemKind::DirOntoFreshDest,
    ItemKind::DirOntoExistingFile,
];

/// Seeds one Tier B shape and returns the sources to hand the driver.
///
/// `filler` extra top-level sources go in for the concurrent driver, which only
/// engages from three sources up; they're plain files so they add no shape.
async fn tier_b_fixture(
    kind: ItemKind,
    concurrent: bool,
) -> (
    Arc<dyn Volume>,
    Arc<dyn Volume>,
    Vec<PathBuf>,
    Vec<(&'static str, &'static [u8])>,
) {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    let mut sources = Vec::new();
    let expected: Vec<(&'static str, &'static [u8])>;

    match kind {
        ItemKind::File => {
            source.create_file(Path::new("/solo.bin"), b"SRC-solo").await.unwrap();
            sources.push(PathBuf::from("/solo.bin"));
            expected = vec![("/solo.bin", b"SRC-solo")];
        }
        ItemKind::DirOntoFreshDest => {
            source.create_directory(Path::new("/album")).await.unwrap();
            source
                .create_file(Path::new("/album/one.bin"), b"SRC-one")
                .await
                .unwrap();
            source.create_directory(Path::new("/album/inner")).await.unwrap();
            source
                .create_file(Path::new("/album/inner/two.bin"), b"SRC-two")
                .await
                .unwrap();
            sources.push(PathBuf::from("/album"));
            expected = vec![("/album/one.bin", b"SRC-one"), ("/album/inner/two.bin", b"SRC-two")];
        }
        ItemKind::DirOntoExistingFile => {
            source.create_directory(Path::new("/album")).await.unwrap();
            source
                .create_file(Path::new("/album/one.bin"), b"SRC-one")
                .await
                .unwrap();
            dest.create_file(Path::new("/album"), b"DEST-was-a-file").await.unwrap();
            sources.push(PathBuf::from("/album"));
            expected = vec![("/album/one.bin", b"SRC-one")];
        }
    }

    if concurrent {
        for name in ["/filler_a.bin", "/filler_b.bin"] {
            source.create_file(Path::new(name), b"filler").await.unwrap();
            sources.push(PathBuf::from(name));
        }
    }

    (source, dest, sources, expected)
}

/// Drives one Tier B cell through the copy or move pipeline against a cache
/// entry that counted files but recorded no per-source result.
async fn run_tier_b(kind: ItemKind, concurrent: bool, is_move: bool) {
    let (source, dest, sources, expected) = tier_b_fixture(kind, concurrent).await;
    let driver = if concurrent { "concurrent" } else { "serial" };
    let op = if is_move { "move" } else { "copy" };
    let cell = format!("tier-b-{op}-{driver}-{}", kind.label());
    let label = cell.clone();

    let source_strs: Vec<String> = sources.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let source_refs: Vec<&str> = source_strs.iter().map(String::as_str).collect();
    let preview_id = seed_cache(CacheState::HitWithoutPerPath, &cell, &source_refs, sources.len(), 64);

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        preview_id,
        ..VolumeCopyConfig::default()
    };

    let result = if is_move {
        move_volumes_with_progress(
            events,
            &cell,
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await
    } else {
        copy_volumes_with_progress(
            events,
            &cell,
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await
    };
    assert!(result.is_ok(), "{label}: the transfer should complete, got {result:?}");

    // Every shape must arrive intact: a per-path-less cache is missing
    // information, ❌ never license to guess that a directory is a file and
    // stream it as one.
    for (path, content) in &expected {
        assert_eq!(
            try_read_all(&dest, path).await.as_deref(),
            Some(*content),
            "{label}: {path} didn't arrive intact"
        );
    }

    // A move's sources are gone; a copy's are still there. Either way no byte
    // is missing from both sides.
    if !is_move {
        // Source and destination roots are both `/` here, so a delivered path is
        // also the source path it came from.
        for (path, content) in &expected {
            assert_eq!(
                try_read_all(&source, path).await.as_deref(),
                Some(*content),
                "{label}: a COPY took {path} from the source"
            );
        }
    }
}

/// Tier B: every source shape survives a per-path-less cache on the SERIAL
/// copy driver.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_b_copy_serial_survives_a_cache_with_no_per_path() {
    for kind in ITEM_KINDS {
        run_tier_b(*kind, false, false).await;
    }
}

/// Tier B: the same three shapes on the CONCURRENT copy driver, which resolves
/// each source's type on its own task.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tier_b_copy_concurrent_survives_a_cache_with_no_per_path() {
    for kind in ITEM_KINDS {
        run_tier_b(*kind, true, false).await;
    }
}

/// Tier B: the same three shapes on the SERIAL move pipeline, whose source
/// sweep is what a wrong type answer turns destructive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_b_move_serial_survives_a_cache_with_no_per_path() {
    for kind in ITEM_KINDS {
        run_tier_b(*kind, false, true).await;
    }
}

/// Tier B: the same three shapes on the CONCURRENT move pipeline.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tier_b_move_concurrent_survives_a_cache_with_no_per_path() {
    for kind in ITEM_KINDS {
        run_tier_b(*kind, true, true).await;
    }
}
