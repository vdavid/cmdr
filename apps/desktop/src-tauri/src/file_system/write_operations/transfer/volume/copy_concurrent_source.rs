//! Getting one top-level source ready to spawn: is it a directory, where does
//! it land, and is something already sitting at that name?
//!
//! All of it runs SYNCHRONOUSLY on the driver, before the task exists. That is
//! the contract the concurrent path is built on: one Stop prompt blocks the
//! whole batch instead of several tasks racing prompts into one oneshot slot,
//! and nothing streams into a folder the user is still being asked about. The
//! driver itself is `copy_concurrent.rs`; what it spawns is
//! `copy_concurrent_task.rs`.
//!
//! These are methods on `ConcurrentCopy` rather than free functions because the
//! borrowed context already holds every input they need — the volumes, the
//! policy, the destination index, the preflight hints, and the shared counters.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::super::super::state::update_operation_status;
use super::super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use super::super::dest_name_index::DestLookup;
use super::conflict::{ResolvedConflict, resolve_volume_conflict};
use super::copy_concurrent::ConcurrentCopy;
use super::copy_concurrent_task::CopyTask;
use super::strategy::resolve_source_is_directory;
use super::transfer_error::{PathRole, WriteFailure, map_volume_error};
use crate::file_system::listing::FileEntry;
use crate::ignore_poison::IgnorePoison;

impl ConcurrentCopy<'_> {
    /// Works out what (if anything) the driver should spawn for one top-level
    /// source: is it a directory, where does it land, and does something already
    /// sit at that name?
    ///
    /// `Ok(None)` means the source is done with — the bulk skip already
    /// accounted it, or conflict resolution said Skip. `Err` means resolution
    /// itself failed, which ends the operation.
    ///
    /// Everything here runs SYNCHRONOUSLY on the driver, before the task exists.
    /// That is the contract: one Stop prompt blocks the whole batch instead of
    /// several tasks racing prompts into one oneshot slot, and nothing streams
    /// into a folder the user is still being asked about. ❌ Don't move any of
    /// it into `run_copy_task`.
    pub(super) async fn prepare_source(
        &self,
        source_index: usize,
        source_path: &Path,
    ) -> Result<Option<CopyTask>, WriteFailure> {
        // Pre-known conflict already accounted upfront in the bulk skip.
        if self.pre_skip_paths.contains(source_path) {
            return Ok(None);
        }

        // Is this source a directory? Resolved ONCE per source, here, from
        // the preflight hint — or by probing when there is none (a LOCAL
        // scan preview completes with an empty `per_path`, so a real
        // directory can arrive hintless). Three things downstream read this
        // answer and all three break on a wrong one: the conflict resolver,
        // `copy_single_path`'s streaming branch, and — the data-safety one —
        // the `in_flight_partials` gate below, which keeps a merged
        // destination directory out of the post-loop's recursive sweep.
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
        // For a file→file Overwrite, conflict resolution hands back a
        // temp sibling to stream into plus the original path to swap in
        // after the write fully lands (safe-replace). `None` ⇒ write
        // `dest_item_path` directly.
        let mut replace_after_write: Option<PathBuf> = None;
        if let Some(dest_meta) = self
            .existing_dest_entry(source_index, source_path, &dest_item_path)
            .await
        {
            // The type and size come from the scan (or the one probe above),
            // never a re-stat: an MTP `scan_for_copy` lists the parent dir,
            // ~18 s for 1046 photos on a cold cache.
            log::debug!(
                "copy_volumes_with_progress: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                dest_item_path.display(),
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
                        source_path.display()
                    );
                    self.account_skipped_file(source_path);
                    return Ok(None);
                }
                Some(rc) => {
                    dest_item_path = rc.write_path;
                    replace_after_write = rc.replace_after_write;
                }
            }
        }

        let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
        log::debug!(
            "copy_volumes_with_progress: spawning copy {} -> {}",
            source_path.display(),
            dest_item_path.display()
        );

        // Mark this destination as in-flight so cancel/error can clean it
        // up — but ONLY for a FILE source. A DIRECTORY source's dest is a
        // (possibly pre-existing, merged) dir whose cleanup path is
        // `remove_tree`; recording the dir ROOT here and
        // then recursively deleting it on keep-partials/rollback would
        // destroy pre-existing dest-only files (the merge invariant). A
        // directory source's cleanup is owned entirely by the per-file
        // `created`/`copied_paths` ledger threaded out of the task's
        // result arms. (A dir task dropped mid-flight on abort leaves its
        // in-flight `.cmdr-tmp-<uuid>` for the backend writer's abort to
        // clean; never the merged root.) Pinned by
        // `cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file`.
        if !source_is_dir {
            self.in_flight_partials
                .lock_ignore_poison()
                .push(dest_item_path.clone());
        }

        // Register before the task is pushed, so a task that never gets
        // polled still shows up in a dump as `spawned`.
        let task_probe = self.op_probe.as_ref().map(|probe| {
            probe.begin_task(
                source_index,
                // A DIRECTORY source's task walks and feeds the window; only a
                // FILE source takes a slot and copies bytes itself.
                if source_is_dir {
                    super::super::transfer_probe::TaskRole::Walker
                } else {
                    super::super::transfer_probe::TaskRole::File
                },
                &source_path.display().to_string(),
                &dest_item_path.display().to_string(),
            )
        });
        Ok(Some(CopyTask {
            events: Arc::clone(&self.events),
            operation_id: self.operation_id.to_string(),
            state: Arc::clone(self.state),
            source_volume: Arc::clone(&self.source_volume),
            dest_volume: Arc::clone(&self.dest_volume),
            config: self.config.clone(),
            apply_to_all: Arc::clone(&self.apply_to_all_cell),
            source_path: source_path.to_path_buf(),
            source_is_dir,
            source_size_hint,
            dest_path: dest_item_path,
            replace_after_write,
            file_name,
            window: self.file_window.clone(),
            op_probe: self.op_probe.clone(),
            task_probe,
            files_done: Arc::clone(&self.files_done_atomic),
            bytes_done: Arc::clone(&self.atomic_bytes_done),
            last_progress: Arc::clone(&self.last_progress_mutex),
            progress_interval: self.progress_interval,
            total_files: self.total_files,
            total_bytes: self.total_bytes,
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
    async fn existing_dest_entry(
        &self,
        source_index: usize,
        source_path: &Path,
        dest_item_path: &Path,
    ) -> Option<FileEntry> {
        if self.dest_dir_is_ours {
            return None;
        }
        match self
            .dest_index
            .as_ref()
            .map(|index| index.lookup(source_path.file_name()))
        {
            Some(DestLookup::Absent) => None,
            Some(DestLookup::Present(entry)) => Some(*entry),
            // No index (a local destination, or a listing that failed), or a
            // name only the backend can settle.
            Some(DestLookup::Unknown) | None => {
                // Record the pre-check BEFORE awaiting it. In the 2026-07-31
                // incident this destination `get_metadata` was the driver's last
                // log line and nothing said whether it returned, so a dump has to
                // be able to name it as the step in progress.
                if let Some(probe) = self.op_probe.as_ref() {
                    probe.set_driver_phase(
                        super::super::transfer_probe::DriverPhase::PreparingNext,
                        &format!("#{source_index} {}", dest_item_path.display()),
                    );
                }
                self.dest_volume.get_metadata(dest_item_path).await.ok()
            }
        }
    }

    /// Runs the conflict resolver for one top-level clash, on the driver.
    ///
    /// Copies the op-wide latch out, runs the resolver on the stack local,
    /// stores it back — mirroring the serial path. The resolver's
    /// `conflict_dispatch_lock` (acquired inside) is what serializes the human
    /// against in-flight deep merges spawned by earlier iterations; that same
    /// lock is why a top-level prompt and a deep prompt can't race the one
    /// oneshot slot. The known acceptable residual: an already-emitted prompt
    /// isn't retroactively resolved by another task's "…all" latch — a rare
    /// extra prompt, never data loss.
    async fn resolve_conflict_on_the_driver(
        &self,
        source_path: &Path,
        dest_item_path: &Path,
        source_size_hint: Option<u64>,
        dest_size_hint: Option<u64>,
        source_is_dir: bool,
    ) -> Result<Option<ResolvedConflict>, WriteFailure> {
        // Parked on a PERSON, with the whole batch behind it: the driver
        // neither fills nor drains the window while a prompt is up, so a dump
        // taken now has to say so rather than leave the pre-check's phase
        // standing.
        if let Some(probe) = self.op_probe.as_ref() {
            probe.set_driver_phase(
                super::super::transfer_probe::DriverPhase::ResolvingConflict,
                &dest_item_path.display().to_string(),
            );
        }
        let mut latched = *self.apply_to_all_cell.lock_ignore_poison();
        let resolved = resolve_volume_conflict(
            &self.source_volume,
            source_path,
            &self.dest_volume,
            dest_item_path,
            self.config,
            &*self.events,
            self.operation_id,
            self.state,
            &mut latched,
            source_size_hint,
            dest_size_hint,
            Some(source_is_dir),
        )
        .await
        .map_err(WriteFailure::synthetic);
        *self.apply_to_all_cell.lock_ignore_poison() = latched;
        resolved
    }

    /// Drops one path from the in-flight partial list, wherever it sits.
    pub(super) fn forget_in_flight_partial(&self, path: &Path) {
        let mut partials = self.in_flight_partials.lock_ignore_poison();
        if let Some(pos) = partials.iter().position(|p| p == path) {
            partials.swap_remove(pos);
        }
    }

    /// Bumps `files_done` and `bytes_done` for a skipped source and (throttled)
    /// emits a `write-progress` event. Without this, a "Skip all" choice silently
    /// runs through dozens of conflicts with the progress bar pinned at 0% — the
    /// user expects the bar to reflect skipped files since the operation is in
    /// fact processing them.
    fn account_skipped_file(&self, source_path: &Path) {
        let hint_size = self
            .source_hints
            .get(source_path)
            .map(|h| if h.is_directory { 0 } else { h.size })
            .unwrap_or(0);
        let new_files = self.files_done_atomic.fetch_add(1, Ordering::Relaxed) + 1;
        let new_bytes = self.atomic_bytes_done.fetch_add(hint_size, Ordering::Relaxed) + hint_size;
        self.files_skipped_atomic.fetch_add(1, Ordering::Relaxed);
        self.bytes_skipped_atomic.fetch_add(hint_size, Ordering::Relaxed);

        let mut last = self.last_progress_mutex.lock_ignore_poison();
        if last.elapsed() >= self.progress_interval {
            *last = Instant::now();
            drop(last);
            self.state.emit_progress_via_sink(
                &*self.events,
                WriteProgressEvent::new(
                    self.operation_id.to_string(),
                    WriteOperationType::Copy,
                    WriteOperationPhase::Copying,
                    source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                    new_files,
                    self.total_files,
                    new_bytes,
                    self.total_bytes,
                ),
            );
            update_operation_status(
                self.operation_id,
                WriteOperationPhase::Copying,
                source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                new_files,
                self.total_files,
                new_bytes,
                self.total_bytes,
            );
        }
    }
}
