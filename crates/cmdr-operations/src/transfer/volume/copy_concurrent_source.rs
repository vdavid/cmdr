//! Getting one top-level source ready to spawn: is it a directory, where does
//! it land, and is something already sitting at that name?
//!
//! All of it runs on the driver, before the task exists. That is the contract
//! the concurrent path is built on: a Stop prompt holds back its source and
//! every one after it instead of several tasks racing prompts into one oneshot
//! slot, and nothing streams into a destination the user is still being asked
//! about. The tasks already spawned run on meanwhile, a deep merge's prompt
//! included. The driver itself is `copy_concurrent.rs`; what it spawns is
//! `copy_concurrent_task.rs`.
//!
//! These are methods on `ConcurrentCopy` rather than free functions because the
//! borrowed context already holds every input they need — the volumes, the
//! policy, the destination index, the preflight hints, and the shared counters.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::super::super::types::SourceItemOutcome;
use super::super::transfer_driver::emit_source_outcome;
use super::super::volume_outage::through_outages;
use super::conflict::{ResolvedConflict, resolve_volume_conflict};
use super::copy_concurrent::ConcurrentCopy;
use super::copy_concurrent_task::CopyTask;
use super::landing::{DestFolder, Landing, NewName, where_it_lands};
use super::merge_ctx::MergeProbe;
use super::preflight::SourceFileFacts;
use super::strategy::{Replaces, resolve_source_is_directory};
use super::transfer_error::{AtPath, PathRole, PathedVolumeError, WriteFailure, map_volume_error};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{ChildName, VolumeError};

impl ConcurrentCopy<'_> {
    /// Works out what (if anything) the driver should spawn for one top-level
    /// source: is it a directory, where does it land, and does something already
    /// sit at that name?
    ///
    /// `Ok(None)` means the source is done with — the bulk skip already
    /// accounted it, or conflict resolution said Skip. `Err` means resolution
    /// itself failed, which ends the operation.
    ///
    /// Everything here runs on the driver, before the task exists. That is the
    /// contract: a Stop prompt holds back this source and every one after it
    /// instead of several tasks racing prompts into one oneshot slot, and
    /// nothing streams into a destination the user is still being asked about.
    /// ❌ Don't move any of it into `run_copy_task`.
    pub(super) async fn prepare_source(
        &self,
        source_index: usize,
        source_path: &Path,
    ) -> Result<Option<CopyTask>, WriteFailure> {
        // Pre-known conflict already accounted upfront in the bulk skip; its
        // verdict goes out as the driver passes it, in order.
        if self.pre_skip_paths.contains(source_path) {
            self.skipped_whole(source_path);
            return Ok(None);
        }

        // Is this source a directory? Resolved ONCE per source, here, from
        // the preflight hint — or by probing when there is none (a LOCAL
        // scan preview completes with an empty `per_path`, so a real
        // directory can arrive hintless). Both things downstream that read this
        // answer break on a wrong one: the conflict resolver, and
        // `copy_single_path`'s streaming branch.
        let source_hint = self.source_hints.get(source_path).copied();
        let source_is_dir = resolve_source_is_directory(
            &self.source_volume,
            source_path,
            source_hint.map(|hint| hint.is_directory),
        )
        .await
        .map_err(|e| {
            WriteFailure::synthetic(map_volume_error(
                &source_path.display().to_string(),
                PathRole::Source,
                e,
            ))
        })?;
        // Sizes stay hint-only: a missing size just means no SMB compound
        // fast path, never a wrong branch, so it isn't worth a probe.
        let source_size_hint = source_hint.and_then(|hint| (!hint.is_directory).then_some(hint.size));

        // Resolve destination path + conflict synchronously.
        let mut dest_item_path = if let Some(name) = source_path.file_name() {
            self.dest_path.join(name)
        } else {
            self.dest_path.to_path_buf()
        };
        // For a file→file Overwrite, conflict resolution says how the
        // original is replaced (`Replaces`): a swap the write's landing makes
        // once its staged bytes are whole, or an in-place write on a
        // destination that publishes whole. Either way the write goes to
        // `dest_item_path`, the original's own name.
        let mut replaces = Replaces::Nothing;
        // Nothing has resolved anything yet, so the name this task writes to is
        // one we believe free (`staged_write.rs::LandingName`).
        let mut dest_name_claimed = false;
        let (landing_path, existing_dest) = self
            .existing_dest_entry(source_index, source_path, &dest_item_path)
            .await
            .map_err(WriteFailure::from)?
            .into_parts();
        // The entry that's THERE, in its own spelling, or the new name spelled
        // for the destination.
        dest_item_path = landing_path;
        if let Some(dest_meta) = existing_dest {
            // The type and size come from the scan (or the one probe above),
            // never a re-stat: an MTP `scan_for_copy` lists the parent dir,
            // ~18 s for 1046 photos on a cold cache.
            log::debug!(
                "copy_volumes_with_progress: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                LogPrivate::path(&dest_item_path),
                source_is_dir,
                dest_meta.is_directory,
            );
            let resolved = self
                .resolve_conflict_on_the_driver(
                    source_path,
                    &dest_item_path,
                    source_size_hint,
                    dest_meta.size,
                    source_is_dir,
                )
                .await?;
            match resolved {
                None => {
                    log::debug!(
                        "copy_volumes_with_progress: skipping {} due to conflict resolution",
                        LogPrivate::path(source_path)
                    );
                    self.account_skipped_file(source_path);
                    return Ok(None);
                }
                Some(rc) => {
                    dest_item_path = rc.write_path;
                    replaces = rc.replaces;
                    // ❗ Claimed only when the pick put a placeholder of ours
                    // there. A pick that reserved nothing (every remote
                    // backend), or a cross-type Overwrite that moved the
                    // destination aside, saw a FREE name: what's there at
                    // landing is someone else's, ❌ never cleared.
                    dest_name_claimed = rc.reserved_placeholder;
                }
            }
        }

        log::debug!(
            "copy_volumes_with_progress: spawning copy {} -> {}",
            LogPrivate::path(source_path),
            LogPrivate::path(&dest_item_path)
        );

        // Register before the task is pushed, so a task that never gets
        // polled still shows up in a dump as `spawned`.
        let source_row = super::super::transfer_probe::TaskRow::source(source_index);
        let task_probe = self.op_probe.as_ref().map(|probe| {
            probe.begin_task(
                source_row,
                // A DIRECTORY source's task walks and feeds the window; only a
                // FILE source takes a slot and copies bytes itself.
                if source_is_dir {
                    super::super::transfer_probe::TaskRole::Walker
                } else {
                    super::super::transfer_probe::TaskRole::File
                },
                &source_path,
                &dest_item_path,
            )
        });
        Ok(Some(CopyTask {
            events: Arc::clone(&self.events),
            operation_id: self.operation_id.to_string(),
            state: Arc::clone(self.state),
            source_volume: Arc::clone(&self.source_volume),
            dest_volume: Arc::clone(&self.dest_volume),
            config: self.config.clone(),
            apply_to_all: Arc::clone(&self.cells.apply_to_all_cell),
            source_path: source_path.to_path_buf(),
            source_is_dir,
            source_facts: SourceFileFacts::from_hint(source_hint),
            dest_path: dest_item_path,
            replaces,
            dest_name_claimed,
            window: self.file_window.clone(),
            displaced: Arc::clone(&self.cells.displaced),
            created: Arc::new(super::merge_ctx::CreatedPaths::default()),
            // Every leaf of a directory source's subtree numbers itself under
            // this source's own row.
            merge_probe: self.op_probe.as_ref().map(|probe| MergeProbe {
                operation: Arc::clone(probe),
                source_row,
            }),
            task_probe,
            leaf_ledger: Arc::clone(&self.leaf_ledger),
            last_progress: Arc::clone(&self.last_progress_mutex),
        }))
    }

    /// Is something already sitting at this destination name?
    ///
    /// Asked as a `get_metadata` per source it is ONE ROUND TRIP PER FILE,
    /// serialized here on the driver, and on a NAS at 3.7 ms RTT it measured
    /// 2.378 s of a 3.224 s best run for 500 files — 74%, and no window width
    /// can overlap it
    /// (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`).
    ///
    /// Two things answer it more cheaply, in order:
    ///
    /// 1. **A destination directory THIS OPERATION created** (Phase 0.5):
    ///    nothing the user already had can be inside a folder that didn't exist
    ///    a moment ago, so every probe is a guaranteed miss and there's nothing
    ///    even to index. ❌ Never widen this to "the destination is empty". A
    ///    pre-existing empty directory can gain an entry from another process
    ///    between any two instants; one we just created cannot have held
    ///    anything BEFORE we made it. Only the second claim is safe, and the
    ///    difference is silent when you get it wrong.
    /// 2. **The destination listing Phase 0.6 already paid for**, for a merge
    ///    into a pre-existing folder — the ordinary F5 copy. `DestNameIndex`
    ///    answers `Absent` only when no name in that listing can resolve to this
    ///    one on any backend; anything it can't settle comes back `Unknown` and
    ///    falls through to the probe, which stays authoritative.
    ///
    /// The listing is taken once, at the start: by the 400th file of a large
    /// batch it can be minutes old, so a file that ARRIVES at the destination
    /// mid-batch is missed and an Overwrite replaces it with no prompt. That
    /// trade is deliberate and David chose it (2026-08-02); ❌ don't answer it
    /// with re-listing, polling, or a freshness window. `DETAILS.md` §
    /// "Answering the pre-check from one listing".
    ///
    /// ❗ A probe that can't ANSWER fails the item (`Err`), and is never read as
    /// free: a `ConnectionTimeout` on a flaky share would otherwise hand the
    /// source to the fresh-write path with no resolver and no policy. Only
    /// `NotFound` is a free name. `landing.rs::where_it_lands` holds the rule,
    /// the look-alike guard, and the new-name spelling for every engine.
    async fn existing_dest_entry(
        &self,
        source_index: usize,
        source_path: &Path,
        dest_item_path: &Path,
    ) -> Result<Landing, PathedVolumeError> {
        let folder = if self.dest_dir_is_ours {
            DestFolder::CreatedByUs
        } else {
            match self.dest_index.as_ref() {
                Some(index) => DestFolder::Listed(index),
                // A local destination, or a listing that failed.
                None => DestFolder::Unlisted,
            }
        };
        let (Some(dest_dir), Some(name)) = (dest_item_path.parent(), source_path.file_name()) else {
            // A source with no name of its own lands on the destination folder
            // itself, which no listing describes.
            if matches!(folder, DestFolder::CreatedByUs) {
                return Ok(Landing::Free(dest_item_path.to_path_buf()));
            }
            return match self.dest_volume.get_metadata(dest_item_path).await {
                Ok(entry) => Ok(Landing::Taken {
                    path: dest_item_path.to_path_buf(),
                    entry: Box::new(entry),
                }),
                Err(VolumeError::NotFound(_)) => Ok(Landing::Free(dest_item_path.to_path_buf())),
                Err(e) => Err(e).at_destination(dest_item_path),
            };
        };
        // Record the pre-check BEFORE awaiting it. In the 2026-07-31 incident
        // this destination `get_metadata` was the driver's last log line and
        // nothing said whether it returned, so a dump has to be able to name it
        // as the step in progress. A listed folder only probes a name its
        // listing can't settle, but naming the step costs nothing either way.
        if !matches!(folder, DestFolder::CreatedByUs)
            && let Some(probe) = self.op_probe.as_ref()
        {
            probe.set_driver_phase_at(
                super::super::transfer_probe::DriverPhase::PreparingNext,
                // The same label the source's own row renders under, so a
                // reader can match the driver's step against the table.
                Some(super::super::transfer_probe::TaskRow::source(source_index)),
                dest_item_path,
            );
        }
        let name = ChildName::new(name)
            .map_err(VolumeError::from)
            .at_destination(dest_item_path)?;
        // A share that drops under the probe is waited for, and the probe asks
        // again: it only reads (`../volume_outage.rs`). Pathed, so a share that
        // stays gone is worded as the lost connection it is.
        through_outages(self.state, &[&self.dest_volume], || async move {
            where_it_lands(&self.dest_volume, dest_dir, name, folder, NewName::Respell)
                .await
                .at_destination(dest_item_path)
        })
        .await
    }

    /// Runs the conflict resolver for one top-level clash, on the driver.
    ///
    /// The resolver's `conflict_dispatch_lock` (acquired inside) queues this
    /// prompt behind any deep merge's, so the two can't race the one oneshot
    /// slot, and its turn re-reads the op-wide latch so an "…all" answered
    /// meanwhile collapses it. The merge holding that lock is a spawned task,
    /// so it reads its answer and lets go while the driver waits here
    /// (ERR-8MEX6). The known acceptable residual: an already-emitted prompt isn't retroactively
    /// resolved by another task's "…all" latch — a rare extra prompt, never
    /// data loss.
    async fn resolve_conflict_on_the_driver(
        &self,
        source_path: &Path,
        dest_item_path: &Path,
        source_size_hint: Option<u64>,
        dest_size_hint: Option<u64>,
        source_is_dir: bool,
    ) -> Result<Option<ResolvedConflict>, WriteFailure> {
        // Parked on a PERSON, this prompt's or the one it queues behind: the
        // window drains but fills no further until they answer, so a dump
        // taken now has to say so rather than leave the pre-check's phase
        // standing.
        if let Some(probe) = self.op_probe.as_ref() {
            probe.set_driver_phase_at(
                super::super::transfer_probe::DriverPhase::ResolvingConflict,
                None,
                dest_item_path,
            );
        }
        let resolved = resolve_volume_conflict(
            &self.source_volume,
            source_path,
            &self.dest_volume,
            dest_item_path,
            self.config,
            &*self.events,
            self.operation_id,
            self.state,
            &self.cells.apply_to_all_cell,
            source_size_hint,
            dest_size_hint,
            Some(source_is_dir),
        )
        .await
        .map_err(WriteFailure::from);
        // An aside a cross-type Overwrite made is the OPERATION's to settle, not
        // the task's: the driver can abandon a task at its cancel deadline.
        resolved.map(|resolved| {
            resolved.map(|mut rc| {
                if let Some(displaced) = rc.displaced.take() {
                    self.cells.displaced.hold(displaced);
                }
                rc
            })
        })
    }

    /// A source skipped whole, before any task: its tree rows, and its
    /// `write-source-item-done`. A skipped copy touched no source.
    fn skipped_whole(&self, source_path: &Path) {
        let tree = self.state.transfer_tree.reporter();
        tree.skip_whole(tree.source(source_path));
        emit_source_outcome(
            &*self.events,
            self.operation_id,
            source_path,
            SourceItemOutcome::Skipped,
            false,
        );
    }

    /// Bumps `files_done` and `bytes_done` for a skipped source and (throttled)
    /// emits a `write-progress` event. Without this, a "Skip all" choice silently
    /// runs through dozens of conflicts with the progress bar pinned at 0% — the
    /// user expects the bar to reflect skipped files since the operation is in
    /// fact processing them.
    fn account_skipped_file(&self, source_path: &Path) {
        self.skipped_whole(source_path);
        let hint_size = self
            .source_hints
            .get(source_path)
            .map(|h| if h.is_directory { 0 } else { h.size })
            .unwrap_or(0);
        // Through the ledger, which bumps `files_done` too: the tasks already in
        // the window report into it from other threads, and only its ordered
        // emit keeps this tick from landing behind theirs.
        let snapshot = self.leaf_ledger.credit_skipped_source(hint_size);
        self.files_skipped_atomic.fetch_add(1, Ordering::Relaxed);
        self.bytes_skipped_atomic.fetch_add(hint_size, Ordering::Relaxed);

        let mut last = self.last_progress_mutex.lock_ignore_poison();
        if last.elapsed() >= self.progress_interval {
            *last = Instant::now();
            drop(last);
            self.leaf_ledger.emit(
                source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                snapshot,
            );
        }
    }
}
