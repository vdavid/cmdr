//! Merging one directory tree into another, level by level.
//!
//! The tree walk half of the cross-volume engine: [`copy_directory_streaming`]
//! recurses a source directory into a destination, resolving deep conflicts
//! inline as it discovers them. Its sibling `strategy.rs` owns the other half —
//! how ONE file's bytes get from A to B (staging, the write itself, and the two
//! cancel tiers) — and the two call into each other: a directory child comes
//! back here, a file child goes there.
//!
//! Shared vocabulary (`MergeCtx`, `CreatedPaths`, `FileWindow`) lives in
//! `merge_ctx.rs` because both halves, both drivers, and `sequential_extract.rs`
//! speak it.
//!
//! The walk DISCOVERS serially and COPIES concurrently: one walker descends the
//! tree in listing order and resolves every conflict on itself, exactly as it
//! always did, while each file's byte copy joins the operation-wide
//! `merge_ctx.rs::FileWindow` and overlaps its siblings. `DETAILS.md`
//! § "One window for the whole operation".
//!
//! The merge invariant this file has to keep: a merge never deletes or
//! overwrites a destination file the source doesn't shadow, under every policy,
//! backend, and cancel/rollback/retry mid-merge. Assert it through
//! `safety_oracle.rs`, never fresh inline asserts. See `CLAUDE.md` § Merge and
//! conflicts, and `DETAILS.md` § "Scan-as-you-merge".

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use super::super::super::state::WriteOperationState;
use super::super::super::types::WriteOperationError;
use super::super::dest_name_index::DestNameIndex;
use super::super::transfer_driver::SourceProgress;
use super::super::transfer_probe::{CURRENT_TASK_PROBE, TaskPhase, TaskProbeHandle, TaskRole, set_task_phase};
use super::conflict::{ResolvedConflict, resolve_volume_conflict};
use super::folder_dates::FolderDates;
use super::landing::{DestFolder, NewName, where_it_lands};
use super::merge_ctx::{CreatedPaths, FileWindow, MergeCtx, MergeProbe};
use super::naming::take_back_reservation;
use super::preflight::SourceFileFacts;
use super::rename_merge::merges_as_a_directory;
use super::strategy::Replaces;
use super::strategy::{LandingName, WriteStaging, note_pending_for_local_dest, staging_for, stream_pipe_file};
use super::transfer_error::{AtPath, PathedVolumeError};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{ChildName, DirectoryCreation, Volume, VolumeError};
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
/// and the created folders the walker reads once the tree is walked.
///
/// The `FuturesUnordered` is LOCAL to one top-level source's walk (it lives on
/// that walker's task, so nothing here needs `'static` or a spawn), while the
/// [`FileWindow`] it reserves from is shared by the whole operation. That split
/// is the point: a walker can hold as many leaves as it likes in its own set,
/// but it cannot start one without a slot from the single op-wide window.
struct LeafPool<'a> {
    window: FileWindow,
    /// The in-flight table and the row of the source this walk descends from,
    /// which is what every leaf row is numbered under.
    probe: Option<MergeProbe>,
    /// Which leaf of this source the next row is. Only ever climbs, so a number
    /// is never reused inside one dump.
    next_leaf: usize,
    in_flight: FuturesUnordered<Pin<Box<dyn Future<Output = LeafResult> + Send + 'a>>>,
    bytes: u64,
    /// The FIRST leaf failure. Later ones are dropped: the walker reports the
    /// file that actually broke, and a second error piled on top of it says
    /// nothing the user can act on (same rule `cleanup.rs::remove_tree` follows).
    first_error: Option<PathedVolumeError>,
    /// Every folder below the root this walk created, noted as the walk leaves
    /// it, so the subtree can date them once its leaves have drained.
    folders: FolderDates,
}

impl<'a> LeafPool<'a> {
    fn new(window: FileWindow, probe: Option<MergeProbe>) -> Self {
        Self {
            window,
            probe,
            next_leaf: 0,
            in_flight: FuturesUnordered::new(),
            bytes: 0,
            first_error: None,
            folders: FolderDates::default(),
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
        let table_row = self.probe.as_ref().map(|probe| {
            let handle = probe.operation.begin_task(
                // Under this source's own row, so the dump says which top-level
                // source is producing the leaf and no two rows collide.
                probe.source_row.leaf(self.next_leaf),
                // Every row this walker opens is one leaf FILE's byte copy,
                // holding the permit reserved just above.
                TaskRole::File,
                &row.source.display().to_string(),
                &row.dest.display().to_string(),
            );
            self.next_leaf += 1;
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
    reason = "One leaf's whole context: both volumes, both paths, what the listing knows about the source, the safe-replace original, how the write is staged, shared state, the ledger, and the source's progress accounting."
)]
async fn copy_leaf<'a>(
    source_volume: &'a Arc<dyn Volume>,
    child_source: PathBuf,
    source_facts: SourceFileFacts,
    dest_volume: &'a Arc<dyn Volume>,
    write_dest: PathBuf,
    replaces: Replaces,
    reserved_placeholder: bool,
    staging: WriteStaging,
    state: &'a Arc<WriteOperationState>,
    created: &'a CreatedPaths,
    progress: &'a Arc<SourceProgress>,
) -> LeafResult {
    // This leaf's own share of the operation's in-flight byte total, held for
    // its whole life. Siblings streaming beside it hold their own, so neither
    // this one finishing nor a sibling's can take the other's bytes off the bar.
    // A leaf that never lands withdraws its share when this handle drops.
    let leaf = progress.begin_leaf();
    let on_chunk = |file_bytes_done: u64, _file_bytes_total: u64| leaf.on_chunk(file_bytes_done);
    // ❗ `.at(&child_source)` is the whole point: this is the deepest frame that
    // knows WHICH file failed. Report it one level up and the user gets the name
    // of the folder they selected instead of the file that broke.
    let streamed = stream_pipe_file(
        source_volume,
        &child_source,
        source_facts,
        dest_volume,
        &write_dest,
        state,
        &on_chunk,
        staging,
    )
    .await;
    // This child gave up (a read failure, a cancel between chunks), so the name
    // it reserved goes back now, rather than waiting for the post-loop sweep of
    // unfilled reservations (`naming.rs::take_back_unfilled_reservations`), which
    // is there for the leaf whose future is dropped before it gets here.
    if streamed.is_err() && reserved_placeholder {
        take_back_reservation(dest_volume, &write_dest, &state.claimed_names).await;
    }
    let bytes = streamed.map_err(|f| PathedVolumeError::at_source_or_rescued_dest(f, &child_source, &write_dest))?;
    // Safe-replace finalize for a file→file Overwrite: the temp now holds the
    // complete new bytes; swap it over the original. On finalize error the temp
    // is preserved as committed data (see `finalize_safe_replace`).
    // An overwrite (safe-replace or in place) makes the op not rollbackable.
    if replaces.overwrites() {
        created.record_overwrite();
    }
    let recorded = match replaces {
        Replaces::ViaTemp(orig) => {
            super::finalize::finalize_safe_replace(dest_volume, &write_dest, &orig)
                .await
                .map_err(|e| PathedVolumeError::at_destination(e, &orig))?;
            orig
        }
        Replaces::InPlace | Replaces::Nothing => write_dest,
    };
    created.record_file(recorded, bytes);
    leaf.complete(bytes);
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
///   pre-existing directory. We list the dest level ONCE and index it into the
///   `DestNameIndex` the top-level pre-check uses, then for each source child
///   the index reports as taken we dispatch through the conflict resolver (file
///   policy: Stop-wait, latch, conditional reduce, type mismatches) — EXCEPT
///   dir-vs-dir, which recurses unconditionally (a folder landing on a folder
///   always merges, never prompts). A child the index reports free is copied
///   straight in. One listing per level, in-memory lookups after; the only
///   `get_metadata` is for a name the listing can't settle, which on an
///   ordinary tree is none of them (`landing.rs::where_it_lands`). A child the
///   destination holds under another Unicode spelling is a clash with THAT
///   entry, and the walk addresses it by its stored name from then on.
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
    progress: &Arc<SourceProgress>,
    merge: Option<&MergeCtx<'_>>,
    // `Some` ⇒ PLAN MODE for the one-pass sequential extractor: create the
    // destination directory structure and resolve every file's conflict as usual,
    // but instead of streaming each file's bytes, record its resolved destination
    // in the plan and leave the byte write to the caller's single decode pass.
    // `None` ⇒ normal streaming copy.
    plan: Option<&super::sequential_extract::ExtractPlan>,
    // The source folder's own date, from the scan's stat of it
    // (`SourceHint::modified_at`): the one date no listing in the walk carries.
    // Dates `dest_path` once the subtree landed, if the walk created it.
    source_modified_at: Option<u64>,
) -> Result<u64, PathedVolumeError> {
    // ONE pool for this whole subtree, so a file at depth 5 shares the window
    // with a file at depth 1 instead of opening one of its own per level.
    let mut pool = LeafPool::new(
        merge.map_or_else(FileWindow::serial, |ctx| ctx.window.clone()),
        merge.and_then(|ctx| ctx.probe.clone()),
    );
    let walked = merge_level(
        source_volume,
        source_path,
        dest_volume,
        dest_path,
        state,
        created,
        progress,
        merge,
        plan,
        &mut pool,
    )
    .await;
    // Unconditional: nothing may still be writing to the destination when this
    // returns, whether the walk finished, failed, or hit a cancel.
    pool.drain().await;
    let root = match walked {
        // The walk's own error is the FIRST failure by construction (it stops
        // the moment a leaf reports one), so it outranks anything the drain
        // then collected from leaves that were already in flight.
        Err(e) => return Err(e),
        Ok(root) => root,
    };
    if let Some(e) = pool.first_error.take() {
        return Err(e);
    }

    // Every leaf landed, so no write can bump a folder's date after this. A
    // failed or cancelled subtree returned above and dates nothing.
    let mut folders = std::mem::take(&mut pool.folders);
    if root == DirectoryCreation::Created {
        folders.note_filled(dest_path.to_path_buf(), source_modified_at);
    }
    match plan {
        // PLAN MODE wrote no file yet: the data pass dates the folders once its
        // decode lands every member.
        Some(plan) => plan.hold_folder_dates(folders),
        None => folders.stamp(dest_volume, state).await,
    }
    Ok(pool.bytes)
}

/// One level of the merge walk. Recurses into directory children and submits
/// file children to `pool`; see [`copy_directory_streaming`] for the semantics.
///
/// Answers whether THIS walk created the level (`Created`), which is what
/// earns a folder its source's date; `AlreadyExisted` when in doubt.
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
    progress: &'a Arc<SourceProgress>,
    merge: Option<&MergeCtx<'_>>,
    plan: Option<&super::sequential_extract::ExtractPlan>,
    pool: &mut LeafPool<'a>,
) -> Result<DirectoryCreation, PathedVolumeError> {
    note_pending_for_local_dest(dest_volume, dest_path);
    // Say what this task is doing before the first `.await` of the level. A
    // walk parks on listings, so a stack sample sees nothing and the dump would
    // otherwise still read `spawned` however long the tree takes. Each leaf
    // sets its own phase once it starts streaming, and the next level sets this
    // one back.
    set_task_phase(TaskPhase::Walking);

    // The source listing is needed at EVERY level, whatever the destination
    // turns out to hold, and it is independent of the whole destination chain
    // below. So the two run concurrently rather than in sequence: on a
    // cross-share merge each leg is a full network round trip, and paying them
    // one after the other doubled the walk's cost per directory (measured
    // ~1.2 s/dir SMB→SMB in a user's bundle, `ERR-AYVM4`, where the walk WAS the
    // transfer).
    //
    // ❗ Unless either side is a single-transport backend. MTP reports
    // `max_concurrent_ops() == 1` because it is one USB bulk transport, and the
    // device lock it takes is released before the PTP transaction runs, so two
    // overlapping calls would interleave transactions on one phone. The window
    // keeps every LEAF serial there for the same reason; these two listings sit
    // outside the window, so they have to honor it themselves.
    //
    // Nothing about ordering changes either way: every decision — prompt order,
    // the apply-to-all latch, the order directories are created in — is made in
    // the `for entry in &entries` loop below, after both legs have landed.
    let legs_may_overlap = source_volume.max_concurrent_ops() > 1 && dest_volume.max_concurrent_ops() > 1;

    // Ensure the destination directory exists, learn whether THIS level
    // pre-existed (a merge) or we created it fresh, and for a merge build the
    // name→entry map ONCE. A freshly-created level can't clash, so we never
    // list it.
    //
    // Every backend EXCEPT MTP surfaces "already exists" as
    // `VolumeError::AlreadyExists` (SMB needs smb2 ≥ 0.8.0 to typed-classify
    // STATUS_OBJECT_NAME_COLLISION). MTP's `create_directory` does NOT error on
    // a same-name dir — the MTP protocol allows same-name sibling objects, so a
    // blind `create_folder` would make a duplicate `photos` and the merge would
    // target the WRONG dir. So on MTP (and any backend whose `create_directory`
    // can't be trusted to error on collision) we pre-check existence with the
    // one listing the merge level pays anyway, and skip the create when present.
    let dest_prepare = async {
        // THIS walk's `create_directory` made the level (proof it was empty),
        // ❌ never the `NotSupported` "treat as fresh" case below.
        let mut made_here = false;
        let level_pre_existed = if backend_create_directory_detects_collisions(dest_volume) {
            match dest_volume.create_directory(dest_path).await {
                Ok(()) => {
                    created.record_dir(dest_path.to_path_buf());
                    made_here = true;
                    false
                }
                Err(VolumeError::AlreadyExists(_)) => true,
                Err(VolumeError::NotSupported) => {
                    // Backend can't create directories at all; assume
                    // `write_from_stream` materializes parents on demand (LocalPosix
                    // does via `create_dir_all` semantics). Treat as fresh.
                    false
                }
                Err(e) => return Err(e),
            }
        } else {
            // Untrusted-collision backend (MTP): pre-check existence.
            if dest_volume.exists(dest_path).await {
                true
            } else {
                match dest_volume.create_directory(dest_path).await {
                    Ok(()) => {
                        created.record_dir(dest_path.to_path_buf());
                        made_here = true;
                        false
                    }
                    // A race created it between the check and the create; merge.
                    Err(VolumeError::AlreadyExists(_)) => true,
                    Err(VolumeError::NotSupported) => false,
                    Err(e) => return Err(e),
                }
            }
        };

        // A level we created ourselves holds nothing, so there is no index to
        // build and no name to look up: `None` is the answer for every child.
        let dest_index = if level_pre_existed {
            Some(DestNameIndex::build(dest_volume.list_directory(dest_path, None).await?))
        } else {
            None
        };
        Ok((dest_index, made_here))
    };

    let (dest_index, entries) = if legs_may_overlap {
        tokio::join!(dest_prepare, source_volume.list_directory(source_path, None))
    } else {
        (
            dest_prepare.await,
            source_volume.list_directory(source_path, None).await,
        )
    };
    let (dest_index, level_made_here) = dest_index.at(source_path)?;
    let entries = entries.at(source_path)?;
    // A move sweeps exactly the folders this walk listed; anything else it
    // finds in the source afterwards arrived later and stays.
    created.record_walked_source_dir(source_path);

    for entry in &entries {
        // The cooperative boundary, per entry. The walk's own work (listings,
        // destination creation, a conflict decision per child) never passes
        // through the between-chunks checkpoint the byte path parks at, so this
        // is the only thing that stops a paused merge from walking on.
        if state.stop_or_park_async().await {
            return Err(VolumeError::Cancelled("Operation cancelled by user".to_string())).at(source_path);
        }

        let child_source = PathBuf::from(&entry.path);
        // The level listing settles almost every child: a byte-exact hit, a
        // look-alike (taken, in ITS spelling), or a name nothing folds onto. Only
        // a case-only match costs a probe, and on an ordinary tree none does.
        let folder = match dest_index.as_ref() {
            Some(index) => DestFolder::Listed(index),
            None => DestFolder::CreatedByUs,
        };
        // ❗ The source's listing named this child, and a hostile server or
        // device can name it `../x` or `/x`: ❌ never join it raw.
        let name = ChildName::new(&entry.name)
            .map_err(VolumeError::from)
            .at(&child_source)?;
        let (child_dest, dest_hit) = where_it_lands(dest_volume, dest_path, name, folder, NewName::Respell)
            .await
            .at(&child_source)?
            .into_parts();
        let dest_hit = dest_hit.as_ref();

        if entry.is_directory {
            // Dir-vs-dir (and dir-into-nothing) always recurses to merge — no
            // resolver call for the folder itself. A dir landing on a same-named
            // LEAF is a type mismatch, which the resolver (below) handles.
            //
            // ❗ A LINK is a leaf, whatever it points at. The listing reports a
            // link to a folder as `is_directory`, and recursing on that alone
            // walks THROUGH it: the incoming files land in its target, a folder
            // the user never picked, and an Overwrite replaces files there. The
            // entry in hand answers it, so a real folder costs no probe.
            let dir_clashes_with_leaf = dest_hit.is_some_and(|d| !merges_as_a_directory(d));
            if !dir_clashes_with_leaf {
                let level = Box::pin(merge_level(
                    source_volume,
                    &child_source,
                    dest_volume,
                    &child_dest,
                    state,
                    created,
                    progress,
                    merge,
                    plan,
                    pool,
                ))
                .await?;
                if level == DirectoryCreation::Created {
                    pool.folders.note_filled(child_dest, entry.modified_at);
                }
                continue;
            }
        }

        // At this point the child is either a FILE, or a directory clashing with
        // a same-named dest LEAF (a file or a link: a type mismatch). If there's
        // a dest hit and we have merge context, route it through the file-policy
        // resolver.
        let mut write_dest = child_dest.clone();
        let mut replaces = Replaces::Nothing;
        // Nothing has resolved a conflict for this child yet, so the name it is
        // about to take is one we believe FREE, and ❗ in a level this walk made,
        // free by proof (`LandingName::free`). A resolver decision below is what
        // turns that into a claim (`staged_write.rs::LandingName`).
        let mut landing = LandingName::free(level_made_here);
        // Nothing has reserved anything for this child either, until a `Rename`
        // resolution below says otherwise.
        let mut reserved_placeholder = false;
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
                    let skipped_bytes = entry.size.unwrap_or(0);
                    created.record_skip(child_source.clone(), skipped_bytes);
                    // ...and credit it to the bars. Without this a merge whose
                    // children all clash reports nothing at all until it ends.
                    progress.skip_leaf(skipped_bytes);
                    continue;
                }
                MergeChildDecision::Proceed {
                    write_path,
                    replace,
                    reserved_placeholder: reserved,
                } => {
                    write_dest = write_path;
                    replaces = replace;
                    reserved_placeholder = reserved;
                    // The resolver picked this name: a `Rename` reserved it with
                    // a placeholder, an Overwrite across types already cleared
                    // it. Either way what the landing may find there is ours.
                    landing = LandingName::ClaimedByTheCaller;
                }
            }
        }

        if entry.is_directory {
            // Only a resolver decision sends a folder on from here. With no
            // merge context nobody freed the name, and recursing would merge
            // into the leaf that holds it (THROUGH it, for a link): refuse.
            if dest_hit.is_some() && merge.is_none() {
                return Err(VolumeError::AlreadyExists(child_dest.display().to_string())).at(&child_source);
            }
            // Type-mismatch Overwrite/Rename that resolved to Proceed: the
            // resolver already set aside/relocated the dest leaf, so recurse
            // into `write_dest` as a fresh (or renamed) directory root.
            let level = Box::pin(merge_level(
                source_volume,
                &child_source,
                dest_volume,
                &write_dest,
                state,
                created,
                progress,
                merge,
                plan,
                pool,
            ))
            .await?;
            if level == DirectoryCreation::Created {
                pool.folders.note_filled(write_dest, entry.modified_at);
            }
            continue;
        }

        // Past every Skip, so this child is one the move carries. Recording it
        // here rather than when its bytes land is safe: a leaf that fails fails
        // the whole source, and a failed source is never swept.
        created.record_carried_source(&child_source, entry);

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
                    replaces,
                    landing,
                    // The plan pass is the only one that lists the source, so
                    // the mode has to be recorded here or the data pass has
                    // nothing to land the file with.
                    source_mode: entry.permissions,
                },
            );
            continue;
        }

        // Conflict resolution for this child is DONE, on the walker, in listing
        // order — the same rule the top-level concurrent driver follows. Only the
        // bytes go wide.
        let staging = staging_for(&replaces, landing);
        let row = LeafRow {
            source: child_source.clone(),
            dest: write_dest.clone(),
        };
        pool.submit(
            row,
            copy_leaf(
                source_volume,
                child_source,
                // The walker listed this level, so the child's size AND mode are
                // already in hand: a deep file costs the copy engine no extra
                // round trip to land with the bits its source reported.
                SourceFileFacts::from_entry(entry),
                dest_volume,
                write_dest,
                replaces,
                reserved_placeholder,
                staging,
                state,
                created,
                progress,
            ),
        )
        .await?;
    }

    Ok(if level_made_here {
        DirectoryCreation::Created
    } else {
        DirectoryCreation::AlreadyExisted
    })
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
    /// Proceed writing to `write_path`; `replace` says what that does to a file
    /// at the name (`ViaTemp(orig)`: write to a temp sibling, finalize after).
    /// `reserved_placeholder` says the resolver put a zero-byte `O_EXCL` file at
    /// `write_path` to hold the name, which the leaf owes taking back if its
    /// write never happens (`naming.rs::ClaimedName`).
    Proceed {
        write_path: PathBuf,
        replace: Replaces,
        reserved_placeholder: bool,
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
            replaces,
            reserved_placeholder,
            displaced,
        })) => {
            // A cross-type Overwrite's aside is the operation's to settle, once
            // it knows how it ended.
            if let Some(displaced) = displaced {
                ctx.displaced.hold(displaced);
            }
            Ok(MergeChildDecision::Proceed {
                write_path,
                replace: replaces,
                reserved_placeholder,
            })
        }
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
