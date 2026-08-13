//! Merging one directory tree into another, level by level.
//!
//! The tree walk half of the cross-volume engine: [`copy_directory_streaming`]
//! recurses a source directory into a destination, resolving deep conflicts
//! inline as it discovers them. Its sibling `strategy.rs` owns the other half —
//! how ONE file's bytes get from A to B (staging, the write itself, and the two
//! cancel tiers) — and the two call into each other: a directory child comes
//! back here, a file child goes there.
//!
//! Shared vocabulary (`MergeCtx`, `CreatedPaths`, `copy_single_path`) lives in
//! `strategy.rs` because both halves and `sequential_extract.rs` speak it.
//!
//! The walk DISCOVERS serially and COPIES concurrently: one walker descends the
//! tree in listing order and resolves every conflict on itself, exactly as it
//! always did, while each file's byte copy joins the operation-wide
//! `strategy.rs::FileWindow` and overlaps its siblings. `DETAILS.md`
//! § "One window for the whole operation".
//!
//! The merge invariant this file has to keep: a merge never deletes or
//! overwrites a destination file the source doesn't shadow, under every policy,
//! backend, and cancel/rollback/retry mid-merge. Assert it through
//! `safety_oracle.rs`, never fresh inline asserts. See `CLAUDE.md` § Merge and
//! conflicts, and `DETAILS.md` § "Scan-as-you-merge".

use std::collections::HashMap;
use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use super::super::super::state::WriteOperationState;
use super::super::super::types::WriteOperationError;
use super::super::transfer_probe::{CURRENT_TASK_PROBE, OperationProbe, TaskProbeHandle};
use super::conflict::{ResolvedConflict, resolve_volume_conflict};
use super::strategy::{CreatedPaths, FileWindow, MergeCtx, note_pending_for_local_dest, staging_for, stream_pipe_file};
use super::transfer_error::{AtPath, PathedVolumeError};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// What one leaf file's copy reports back to the walker.
type LeafResult = Result<u64, PathedVolumeError>;

/// What a leaf is called in the operation's in-flight table. Built even when no
/// probe is registered (the tests), which costs two `PathBuf` clones per file
/// against a network round trip.
struct LeafRow {
    source: PathBuf,
    dest: PathBuf,
}

/// The leaves this operation currently has in flight, plus the running totals
/// the walker reads once the tree is walked.
///
/// The `FuturesUnordered` is LOCAL to one top-level source's walk (it lives on
/// that walker's task, so nothing here needs `'static` or a spawn), while the
/// [`FileWindow`] it reserves from is shared by the whole operation. That split
/// is the point: a walker can hold as many leaves as it likes in its own set,
/// but it cannot start one without a slot from the single op-wide window.
struct LeafPool<'a> {
    window: FileWindow,
    op_probe: Option<Arc<OperationProbe>>,
    /// Row number for the next leaf's entry in the in-flight table. Per walker,
    /// which is enough to tell two rows of the same subtree apart in a dump.
    next_row: usize,
    in_flight: FuturesUnordered<Pin<Box<dyn Future<Output = LeafResult> + Send + 'a>>>,
    bytes: u64,
    /// The FIRST leaf failure. Later ones are dropped: the walker reports the
    /// file that actually broke, and a second error piled on top of it says
    /// nothing the user can act on (same rule `cleanup.rs::remove_tree` follows).
    first_error: Option<PathedVolumeError>,
}

impl<'a> LeafPool<'a> {
    fn new(window: FileWindow, op_probe: Option<Arc<OperationProbe>>) -> Self {
        Self {
            window,
            op_probe,
            next_row: 0,
            in_flight: FuturesUnordered::new(),
            bytes: 0,
            first_error: None,
        }
    }

    fn fold(&mut self, result: LeafResult) {
        match result {
            Ok(bytes) => self.bytes += bytes,
            Err(e) => {
                if self.first_error.is_none() {
                    self.first_error = Some(e);
                }
            }
        }
    }

    /// Reserves an op-wide slot, keeping THIS walker's own leaves moving while it
    /// waits. Without the drain arm a full window would park the walker on a
    /// permit its own in-flight leaves are the only ones able to release.
    async fn reserve(&mut self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        enum Step {
            Landed(LeafResult),
            Reserved(Option<tokio::sync::OwnedSemaphorePermit>),
        }
        let window = self.window.clone();
        loop {
            if self.in_flight.is_empty() {
                return window.reserve().await;
            }
            let step = {
                let in_flight = &mut self.in_flight;
                tokio::select! {
                    // Prefer landing finished work: it frees a slot, records the
                    // file, and keeps the progress bar moving.
                    biased;
                    Some(landed) = in_flight.next() => Step::Landed(landed),
                    permit = window.reserve() => Step::Reserved(permit),
                }
            };
            match step {
                Step::Landed(landed) => self.fold(landed),
                Step::Reserved(permit) => return permit,
            }
        }
    }

    /// Runs one leaf's byte copy inside the window.
    ///
    /// A SERIAL window awaits it inline, so an MTP transfer sees exactly the
    /// sequence it always did — one operation at a time on the bulk transport,
    /// directory creates included. A concurrent window parks it in this walker's
    /// set with a permit and an in-flight-table row of its own.
    ///
    /// `Err` means the walk must stop: the pool already holds every leaf that
    /// landed, and this is the first failure.
    async fn submit(
        &mut self,
        row: LeafRow,
        leaf: impl Future<Output = LeafResult> + Send + 'a,
    ) -> Result<(), PathedVolumeError> {
        if let Some(e) = self.first_error.take() {
            return Err(e);
        }
        if self.window.is_serial() {
            let landed = leaf.await;
            self.fold(landed);
            return match self.first_error.take() {
                Some(e) => Err(e),
                None => Ok(()),
            };
        }
        let permit = self.reserve().await;
        if let Some(e) = self.first_error.take() {
            return Err(e);
        }
        // One row per in-flight WRITE, which is the invariant every `TaskProbe`
        // field is built on: `arm_stall_abort` replaces the row's token per
        // attempt and `set_bytes` stores (never adds) the attempt's count, so two
        // leaves sharing a row would clobber each other's stall-abort signal and
        // keep resetting the watchdog's stillness clock. Dropping the handle with
        // the future takes the row away again.
        let table_row = self.op_probe.as_ref().map(|probe| {
            let handle = probe.begin_task(
                self.next_row,
                &row.source.display().to_string(),
                &row.dest.display().to_string(),
            );
            self.next_row += 1;
            handle
        });
        self.in_flight.push(Box::pin(async move {
            // Both ride the leaf's whole life: the permit returns the op-wide
            // slot and the handle clears the in-flight row, on success, failure,
            // or a drop mid-write.
            let _permit = permit;
            let table_row = table_row;
            match table_row.as_ref().map(TaskProbeHandle::probe) {
                Some(probe) => CURRENT_TASK_PROBE.scope(probe, leaf).await,
                None => leaf.await,
            }
        }));
        Ok(())
    }

    /// Waits out every leaf still in flight and folds it in.
    ///
    /// ❌ Never skipped, on any exit path: a leaf still running when the walk
    /// returns would keep writing to the destination after the driver has moved
    /// on to cleanup, and its `created` record would land too late for rollback
    /// to see it. Under cancel the leaves wind down on their own next chunk
    /// (`stream_pipe_file` checks the intent per chunk), and the driver's
    /// cancel-drain deadline is the backstop for one that doesn't.
    async fn drain(&mut self) {
        while let Some(landed) = self.in_flight.next().await {
            self.fold(landed);
        }
    }
}

/// Streams ONE file to the destination and records it, which is everything the
/// walker used to do inline per file child. Split out so it can be handed to the
/// window as a unit of work.
#[allow(
    clippy::too_many_arguments,
    reason = "One leaf's whole context: both volumes, both paths, the size hint, the safe-replace original, shared state, the ledger, and two progress callbacks."
)]
async fn copy_leaf<'a>(
    source_volume: &'a Arc<dyn Volume>,
    child_source: PathBuf,
    source_size_hint: Option<u64>,
    dest_volume: &'a Arc<dyn Volume>,
    write_dest: PathBuf,
    replace_after_write: Option<PathBuf>,
    state: &'a Arc<WriteOperationState>,
    created: &'a CreatedPaths,
    on_file_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &'a (dyn Fn(u64) + Sync),
) -> LeafResult {
    // ❗ `.at(&child_source)` is the whole point: this is the deepest frame that
    // knows WHICH file failed. Report it one level up and the user gets the name
    // of the folder they selected instead of the file that broke.
    let bytes = stream_pipe_file(
        source_volume,
        &child_source,
        source_size_hint,
        dest_volume,
        &write_dest,
        state,
        on_file_progress,
        staging_for(&replace_after_write),
    )
    .await
    .at(&child_source)?;
    // Safe-replace finalize for a file→file Overwrite: the temp now holds the
    // complete new bytes; swap it over the original. On finalize error the temp
    // is preserved as committed data (see `finalize_safe_replace`).
    let recorded = match replace_after_write {
        Some(orig) => {
            super::conflict::finalize_safe_replace(dest_volume, &write_dest, &orig)
                .await
                .at(&child_source)?;
            // A deep-merge child that replaced an existing dest file: record the
            // overwrite so the operation-log eligibility is honest (a copy / move
            // that overwrote isn't rollbackable — the original is gone).
            created.record_overwrite();
            orig
        }
        None => write_dest,
    };
    created.record_file(recorded);
    on_file_complete(bytes);
    Ok(bytes)
}

/// Copies (merges) a directory tree from source to destination, streaming each
/// file through `write_from_stream`. Cancellation is checked between entries.
///
/// The recursion itself lives in `merge_level`; this is the entry point that owns
/// the subtree's leaf pool and guarantees it is drained before returning.
///
/// ## Scan-as-you-merge
///
/// The merge discovers deep conflicts inline, level by level, with no upfront
/// recursive pre-scan. The trigger is the destination directory's existence:
///
/// - `create_directory` returns `Ok(())` ⇒ WE created this level fresh. Nothing
///   inside it can clash, so we skip the dest listing entirely and stream every
///   source child straight in.
/// - `create_directory` returns `AlreadyExists` ⇒ we're MERGING into the user's
///   pre-existing directory. We list the dest level ONCE and build a
///   `name → FileEntry` map, then for each source child that hits the map we
///   dispatch through the conflict resolver (file policy: Stop-wait, latch,
///   conditional reduce, type mismatches) — EXCEPT dir-vs-dir, which recurses
///   unconditionally (a folder landing on a folder always merges, never
///   prompts). A child with no map hit is copied straight in. One listing per
///   level, in-memory lookups after — no per-child `get_metadata` probes.
///
/// The `Ok` vs `AlreadyExists` split also drives rollback: `Ok` records the dir
/// in `created` (rollback may remove it once empty); `AlreadyExists` does NOT,
/// so rollback never touches the user's pre-existing directory — only the files
/// we wrote into it. This is what keeps a merge from destroying dest-only files.
///
/// When `merge` is `None`, there's no per-child conflict resolution: a clashing
/// dest file is overwritten blindly (the cross-volume move's copy phase, where
/// the dest is fresh staging, plus tests that never merge). `Some` is what the
/// volume copy / cross-volume move pipelines pass so deep clashes honor policy.
/// `None` also means no window, so those callers keep a strictly serial walk.
///
/// ## The window
///
/// Discovery stays serial and copying goes wide: this function walks the tree and
/// resolves every conflict on ONE walker, in listing order, then hands each
/// file's byte copy to the operation-wide [`FileWindow`] on `MergeCtx`. Prompt
/// order, the apply-to-all latch, and the order directories are created in are
/// therefore all exactly what they were when the whole walk was serial; only the
/// bytes overlap. PLAN MODE never streams a byte, so it never touches the window.
#[allow(
    clippy::too_many_arguments,
    reason = "Mirrors copy_single_path's argument list plus the rollback ledger, merge context, and the sequential-extract plan sink; bundling into a struct adds ceremony without cleaning anything up."
)]
pub(super) async fn copy_directory_streaming(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn(u64) + Sync),
    merge: Option<&MergeCtx<'_>>,
    // `Some` ⇒ PLAN MODE for the one-pass sequential extractor: create the
    // destination directory structure and resolve every file's conflict as usual,
    // but instead of streaming each file's bytes, record its resolved destination
    // in the plan and leave the byte write to the caller's single decode pass.
    // `None` ⇒ normal streaming copy.
    plan: Option<&super::sequential_extract::ExtractPlan>,
) -> Result<u64, PathedVolumeError> {
    // ONE pool for this whole subtree, so a file at depth 5 shares the window
    // with a file at depth 1 instead of opening one of its own per level.
    let mut pool = LeafPool::new(
        merge.map_or_else(FileWindow::serial, |ctx| ctx.window.clone()),
        merge.and_then(|ctx| ctx.op_probe.clone()),
    );
    let walked = merge_level(
        source_volume,
        source_path,
        dest_volume,
        dest_path,
        state,
        created,
        on_file_progress,
        on_file_complete,
        merge,
        plan,
        &mut pool,
    )
    .await;
    // Unconditional: nothing may still be writing to the destination when this
    // returns, whether the walk finished, failed, or hit a cancel.
    pool.drain().await;
    match walked {
        // The walk's own error is the FIRST failure by construction (it stops
        // the moment a leaf reports one), so it outranks anything the drain
        // then collected from leaves that were already in flight.
        Err(e) => Err(e),
        Ok(()) => match pool.first_error.take() {
            Some(e) => Err(e),
            None => Ok(pool.bytes),
        },
    }
}

/// One level of the merge walk. Recurses into directory children and submits
/// file children to `pool`; see [`copy_directory_streaming`] for the semantics.
#[allow(
    clippy::too_many_arguments,
    reason = "Mirrors copy_single_path's argument list plus the rollback ledger, merge context, the sequential-extract plan sink, and the leaf pool."
)]
async fn merge_level<'a>(
    source_volume: &'a Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &'a Arc<dyn Volume>,
    dest_path: &Path,
    state: &'a Arc<WriteOperationState>,
    created: &'a CreatedPaths,
    on_file_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &'a (dyn Fn(u64) + Sync),
    merge: Option<&MergeCtx<'_>>,
    plan: Option<&super::sequential_extract::ExtractPlan>,
    pool: &mut LeafPool<'a>,
) -> Result<(), PathedVolumeError> {
    note_pending_for_local_dest(dest_volume, dest_path);

    // Ensure the destination directory exists, and learn whether THIS level
    // pre-existed (a merge) or we created it fresh.
    //
    // Every backend EXCEPT MTP surfaces "already exists" as
    // `VolumeError::AlreadyExists` (SMB needs smb2 ≥ 0.8.0 to typed-classify
    // STATUS_OBJECT_NAME_COLLISION). MTP's `create_directory` does NOT error on
    // a same-name dir — the MTP protocol allows same-name sibling objects, so a
    // blind `create_folder` would make a duplicate `photos` and the merge would
    // target the WRONG dir. So on MTP (and any backend whose `create_directory`
    // can't be trusted to error on collision) we pre-check existence with the
    // one listing the merge level pays anyway, and skip the create when present.
    let level_pre_existed = if backend_create_directory_detects_collisions(dest_volume) {
        match dest_volume.create_directory(dest_path).await {
            Ok(()) => {
                created.record_dir(dest_path.to_path_buf());
                false
            }
            Err(VolumeError::AlreadyExists(_)) => true,
            Err(VolumeError::NotSupported) => {
                // Backend can't create directories at all; assume
                // `write_from_stream` materializes parents on demand (LocalPosix
                // does via `create_dir_all` semantics). Treat as fresh.
                false
            }
            Err(e) => return Err(e).at(source_path),
        }
    } else {
        // Untrusted-collision backend (MTP): pre-check existence.
        if dest_volume.exists(dest_path).await {
            true
        } else {
            match dest_volume.create_directory(dest_path).await {
                Ok(()) => {
                    created.record_dir(dest_path.to_path_buf());
                    false
                }
                // A race created it between the check and the create; merge.
                Err(VolumeError::AlreadyExists(_)) => true,
                Err(VolumeError::NotSupported) => false,
                Err(e) => return Err(e).at(source_path),
            }
        }
    };

    // Build the dest name→entry map ONCE, only for a pre-existing (merging)
    // level. A freshly-created level can't clash, so we never list it.
    let dest_by_name: HashMap<String, FileEntry> = if level_pre_existed {
        dest_volume
            .list_directory(dest_path, None)
            .await
            .at(source_path)?
            .into_iter()
            .map(|e| (e.name.clone(), e))
            .collect()
    } else {
        HashMap::new()
    };

    let entries = source_volume.list_directory(source_path, None).await.at(source_path)?;

    for entry in &entries {
        if super::super::super::state::is_cancelled(&state.intent) {
            return Err(VolumeError::Cancelled("Operation cancelled by user".to_string())).at(source_path);
        }

        let child_source = PathBuf::from(&entry.path);
        let child_dest = dest_path.join(&entry.name);
        let dest_hit = dest_by_name.get(&entry.name);

        if entry.is_directory {
            // Dir-vs-dir (and dir-into-nothing) always recurses to merge — no
            // resolver call for the folder itself. A dir landing on a same-named
            // FILE is a type mismatch, which the resolver (below) handles.
            let dir_clashes_with_file = dest_hit.is_some_and(|d| !d.is_directory);
            if !dir_clashes_with_file {
                Box::pin(merge_level(
                    source_volume,
                    &child_source,
                    dest_volume,
                    &child_dest,
                    state,
                    created,
                    on_file_progress,
                    on_file_complete,
                    merge,
                    plan,
                    pool,
                ))
                .await?;
                continue;
            }
        }

        // At this point the child is either a FILE, or a directory clashing with
        // a same-named dest FILE (type mismatch). If there's a dest hit and we
        // have merge context, route it through the file-policy resolver.
        let mut write_dest = child_dest.clone();
        let mut replace_after_write: Option<PathBuf> = None;
        if let Some(hit) = dest_hit
            && let Some(ctx) = merge
        {
            match resolve_merge_child(ctx, source_volume, &child_source, entry, dest_volume, &child_dest, hit)
                .await
                .at(&child_source)?
            {
                MergeChildDecision::Skip => {
                    // A DEEP skip: record it so the caller knows this subtree did
                    // not extract in full (the move-out op must keep the source in
                    // the archive; deleting it would drop this un-landed child).
                    created.record_skip(child_source.clone(), entry.size.unwrap_or(0));
                    continue;
                }
                MergeChildDecision::Proceed { write_path, replace } => {
                    write_dest = write_path;
                    replace_after_write = replace;
                }
            }
        }

        if entry.is_directory {
            // Type-mismatch Overwrite/Rename that resolved to Proceed: the
            // resolver already cleared/relocated the dest file, so recurse into
            // `write_dest` as a fresh (or renamed) directory root.
            Box::pin(merge_level(
                source_volume,
                &child_source,
                dest_volume,
                &write_dest,
                state,
                created,
                on_file_progress,
                on_file_complete,
                merge,
                plan,
                pool,
            ))
            .await?;
            continue;
        }

        // PLAN MODE (one-pass sequential extract): the destination + conflict are
        // resolved; record the write and let the caller's single decode pass
        // stream the bytes. Don't stream, count, record, or emit progress here —
        // the data pass owns all of that. The directory structure and conflict
        // prompts still happened above, exactly as a streaming copy would.
        if let Some(plan) = plan {
            plan.record(
                child_source,
                super::sequential_extract::PlannedWrite {
                    dest_path: write_dest,
                    replace_after_write,
                },
            );
            continue;
        }

        // Conflict resolution for this child is DONE, on the walker, in listing
        // order — the same rule the top-level concurrent driver follows. Only the
        // bytes go wide.
        let row = LeafRow {
            source: child_source.clone(),
            dest: write_dest.clone(),
        };
        pool.submit(
            row,
            copy_leaf(
                source_volume,
                child_source,
                entry.size,
                dest_volume,
                write_dest,
                replace_after_write,
                state,
                created,
                on_file_progress,
                on_file_complete,
            ),
        )
        .await?;
    }

    Ok(())
}

/// Whether this backend's `create_directory` reliably returns
/// `VolumeError::AlreadyExists` when a same-name directory already exists.
///
/// `true` for LocalPosix (`std::fs::create_dir` → `ErrorKind::AlreadyExists`),
/// SMB (smb2 typed STATUS_OBJECT_NAME_COLLISION), and InMemoryVolume's
/// merge-test variant. `false` for MTP: the protocol allows same-name sibling
/// objects and `create_folder` happily makes a duplicate, so the merge walker
/// must pre-check existence instead of trusting the create to error.
fn backend_create_directory_detects_collisions(volume: &Arc<dyn Volume>) -> bool {
    volume.create_directory_errors_on_existing_dir()
}

/// Outcome of resolving one clashing child inside a merge.
enum MergeChildDecision {
    /// Honor a Skip: do NOT touch the dest child at all.
    Skip,
    /// Proceed writing to `write_path`; `replace` is `Some(orig)` for a
    /// file→file safe-replace (write to a temp sibling, finalize after).
    Proceed {
        write_path: PathBuf,
        replace: Option<PathBuf>,
    },
}

/// Dispatches one clashing merge child through the volume conflict resolver,
/// reusing the op-wide apply-to-all latch so a "…all" choice from any level (top
/// or deep) applies here. Mirrors the serial top-level path's latch handling:
/// copy the latch out of the shared cell, run the async resolver on the stack
/// local, store it back. The `conflict_dispatch_lock` inside the resolver — not
/// this cell — is what serializes the human across concurrent merges.
async fn resolve_merge_child(
    ctx: &MergeCtx<'_>,
    source_volume: &Arc<dyn Volume>,
    child_source: &Path,
    entry: &FileEntry,
    dest_volume: &Arc<dyn Volume>,
    child_dest: &Path,
    dest_hit: &FileEntry,
) -> Result<MergeChildDecision, VolumeError> {
    // Deep children aren't top-level sources, so no preflight hint exists for
    // them; the resolver falls back to trait calls. We DO know both sides' type
    // and size from the listing entries already in hand — the source's from this
    // level's source listing, the destination's from the `dest_by_name` map the
    // caller built for the same level. That saves the resolver a redundant
    // `is_directory` probe and seeds the dialog's size annotations.
    //
    // ❗ The dest size matters beyond display: it's what `OverwriteSmaller`
    // compares against. Passing `None` here used to leave the resolver
    // fabricating a `0`, which made every destination look smaller.
    let source_is_directory_hint = Some(entry.is_directory);
    let source_size_hint = if entry.is_directory { None } else { entry.size };
    let dest_size_hint = if dest_hit.is_directory { None } else { dest_hit.size };
    let _ = ctx.source_hints; // hints are keyed by top-level source path; deep children never match

    let mut latched = *ctx.apply_to_all.lock_ignore_poison();
    let resolved = resolve_volume_conflict(
        source_volume,
        child_source,
        dest_volume,
        child_dest,
        ctx.config,
        ctx.events,
        ctx.operation_id,
        ctx.state,
        &mut latched,
        source_size_hint,
        dest_size_hint,
        source_is_directory_hint,
    )
    .await;
    *ctx.apply_to_all.lock_ignore_poison() = latched;

    match resolved {
        Ok(None) => Ok(MergeChildDecision::Skip),
        Ok(Some(ResolvedConflict {
            write_path,
            replace_after_write,
        })) => Ok(MergeChildDecision::Proceed {
            write_path,
            replace: replace_after_write,
        }),
        // The resolver returns a typed `WriteOperationError`; map cancellation
        // back to the `VolumeError::Cancelled` this function's callers expect so
        // the post-loop reclassifies it as a cancel, not a transport error.
        Err(WriteOperationError::Cancelled { .. }) => Err(VolumeError::Cancelled("Operation cancelled by user".into())),
        Err(other) => Err(VolumeError::IoError {
            message: format!("conflict resolution failed: {other:?}"),
            raw_os_error: None,
        }),
    }
}
