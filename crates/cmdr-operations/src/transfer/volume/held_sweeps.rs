//! A cross-volume move's held source sweeps: the top-level sources whose copies
//! landed and whose originals wait for the destination's device barrier before
//! they go, so a move of N loose files pays about one barrier per interval
//! instead of N (#1148).
//!
//! The must-holds, in short: a source goes only after a barrier that STARTED
//! after its copy landed confirmed; a refused barrier keeps its whole batch; a
//! cancel or a failure sweeps nothing more (❌ only a completed loop sweeps the
//! tail). The rule, its why, and its tests: `../DETAILS.md` § Durability,
//! "Cross-volume move".

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::super::super::event_sinks::OperationEventSink;
use super::super::super::journal;
use super::super::super::ledger::WrittenFile;
use super::super::super::locked_sources::LockedPlan;
use super::super::super::state::WriteOperationState;
use super::super::super::types::{SourceItemOutcome, WriteOperationError};
use super::super::left_in_source::LeftInSource;
use super::super::transfer_driver::emit_source_outcome;
use super::displaced_destination::DisplacedLedger;
use super::source_sweep::{CarriedSource, sweep_carried_source};
use super::transfer_error::{PathRole, map_volume_error, source_not_removed};
use crate::rt::Instant;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::Volume;

/// One top-level source whose copy landed, with everything its sweep and the
/// steps after it need.
pub(super) struct HeldSource {
    pub(super) source: PathBuf,
    pub(super) is_dir: bool,
    /// What the copy carried, which is all the sweep may remove.
    pub(super) carried: CarriedSource,
    /// The merge's Skips under it, which stay.
    pub(super) skipped: HashSet<PathBuf>,
    /// Where it landed: the original's own name after an overwrite, a folder's
    /// destination root otherwise.
    pub(super) landed: PathBuf,
    /// The verdict its first `write-source-item-done` already gave.
    pub(super) verdict: SourceItemOutcome,
    /// The files and folders the copy created, for the journal.
    pub(super) created_files: Vec<WrittenFile>,
    pub(super) created_dirs: Vec<PathBuf>,
    /// The bytes a FILE source's copy wrote, for its journal row.
    pub(super) bytes: u64,
    /// Whether it replaced anything at the destination (move eligibility).
    pub(super) overwrote: bool,
}

/// The move's held sweeps, and the context every sweep runs in.
pub(super) struct HeldSweeps {
    pub(super) state: Arc<WriteOperationState>,
    pub(super) events: Arc<dyn OperationEventSink>,
    pub(super) operation_id: String,
    pub(super) source_volume: Arc<dyn Volume>,
    pub(super) dest_volume: Arc<dyn Volume>,
    pub(super) locked: Arc<LockedPlan>,
    pub(super) left_in_source: Arc<Mutex<LeftInSource>>,
    pub(super) displaced: Arc<DisplacedLedger>,
    pub(super) journal_volumes: Option<(String, String)>,
    /// In copy order. A source leaves only once it's swept, so whatever is here
    /// when the move stops is still in both places.
    held: Mutex<Vec<HeldSource>>,
    last_barrier: Mutex<Instant>,
    interval: Duration,
}

impl HeldSweeps {
    #[allow(
        clippy::too_many_arguments,
        reason = "the context a source sweep and the steps after it run in"
    )]
    pub(super) fn new(
        state: Arc<WriteOperationState>,
        events: Arc<dyn OperationEventSink>,
        operation_id: String,
        source_volume: Arc<dyn Volume>,
        dest_volume: Arc<dyn Volume>,
        locked: Arc<LockedPlan>,
        left_in_source: Arc<Mutex<LeftInSource>>,
        displaced: Arc<DisplacedLedger>,
        journal_volumes: Option<(String, String)>,
    ) -> Self {
        let interval = state.barriers.interval();
        Self {
            state,
            events,
            operation_id,
            source_volume,
            dest_volume,
            locked,
            left_in_source,
            displaced,
            journal_volumes,
            held: Mutex::new(Vec::new()),
            last_barrier: Mutex::new(crate::rt::now()),
            interval,
        }
    }

    /// Holds one landed source until a barrier covers it.
    pub(super) fn hold(&self, source: HeldSource) {
        self.held.lock_ignore_poison().push(source);
    }

    /// How many landed sources are still unswept, so in both places.
    pub(super) fn unswept(&self) -> usize {
        self.held.lock_ignore_poison().len()
    }

    /// Sweeps the batch when the barrier interval has gone by since the last
    /// barrier started.
    pub(super) async fn sweep_if_due(&self) -> Result<(), WriteOperationError> {
        let due = crate::rt::since(*self.last_barrier.lock_ignore_poison()) >= self.interval;
        if due { self.sweep_held().await } else { Ok(()) }
    }

    /// One barrier, then every held source swept in copy order. Whatever this
    /// doesn't sweep stays held: the whole batch when the barrier refuses or the
    /// move was cancelled, and the failed source plus the rest after a sweep
    /// refuses one.
    pub(super) async fn sweep_held(&self) -> Result<(), WriteOperationError> {
        let batch = std::mem::take(&mut *self.held.lock_ignore_poison());
        let Some(first) = batch.first() else {
            return Ok(());
        };
        if self.state.intent().is_cancelled() {
            self.put_back(batch);
            return Err(cancelled());
        }
        // Stamped at the START: the barrier covers what was handed over before
        // it began, which is every copy in `batch`.
        *self.last_barrier.lock_ignore_poison() = crate::rt::now();
        if let Err(e) = self.dest_volume.flush_writes().await {
            log::warn!(
                target: "write_durability",
                "move_between_volumes: {} sources stay, their copies aren't proven durable (first {}): {}",
                batch.len(),
                LogPrivate::path(&first.source),
                LogError(&e)
            );
            // The refusal is the DESTINATION's, so it's mapped as one.
            let landed_at = first.landed.display().to_string();
            let error = WriteOperationError::SourceNotRemoved {
                path: first.source.display().to_string(),
                cause: Box::new(map_volume_error(&landed_at, PathRole::Destination, e)),
                landed_at,
            };
            self.put_back(batch);
            return Err(error);
        }
        let mut rest = batch.into_iter();
        while let Some(held) = rest.next() {
            // A cancel during a long batch keeps what it hasn't reached.
            if self.state.intent().is_cancelled() {
                self.put_back(std::iter::once(held).chain(rest).collect());
                return Err(cancelled());
            }
            if let Err(e) = self.sweep_one(&held).await {
                self.put_back(std::iter::once(held).chain(rest).collect());
                return Err(e);
            }
        }
        Ok(())
    }

    /// Removes one held source from what its copy carried, then runs the steps
    /// that follow a swept source.
    async fn sweep_one(&self, held: &HeldSource) -> Result<(), WriteOperationError> {
        let kept = sweep_carried_source(
            &self.state,
            &self.source_volume,
            &held.source,
            &held.carried,
            &held.skipped,
            &self.locked,
        )
        .await
        .map_err(|e| {
            // Name the file that actually refused to go, which for a folder
            // source is a leaf inside it: ❌ never `held.source`, whose own
            // `ENOTEMPTY` is only the symptom.
            log::warn!(
                target: "move",
                "move_between_volumes: source delete failed for {} (under {}) after successful copy: {}",
                LogPrivate::path(&e.path),
                LogPrivate::path(&held.source),
                LogError(&e.error)
            );
            // ❗ The copy LANDED, so the move didn't fail: say so.
            source_not_removed(&e.path, &held.landed, e.error)
        })?;
        self.left_in_source
            .lock_ignore_poison()
            .note(&held.source, held.is_dir, kept.appeared, kept.changed);
        // What the sweep kept is what keeps the source standing: a Skip, a
        // newcomer, or a change. ❌ Never inferred from "it was a move", and ❌
        // never a fresh probe: on an object store that is a billed request per
        // source the estimate doesn't count.
        let source_removed = kept.appeared == 0 && kept.changed == 0 && held.skipped.is_empty();
        // Moved in full, so whatever it set aside is replaced.
        self.displaced.landed_under(&held.landed);
        emit_source_outcome(
            &*self.events,
            &self.operation_id,
            &held.source,
            held.verdict,
            source_removed,
        );
        self.journal(held).await;
        Ok(())
    }

    /// Journals one swept source's leaves under the REAL volume ids: a file
    /// source as one leaf, a folder source as one leaf per copied file plus the
    /// destination folders it created after them, so their `seq` follows the
    /// contents within the subtree. No-ops without journal volumes (tests, the
    /// both-local shortcut).
    async fn journal(&self, held: &HeldSource) {
        let Some((src_vol, dst_vol)) = self.journal_volumes.as_ref() else {
            return;
        };
        let landed =
            journal::LandedTimes::read_back(&*self.dest_volume, &held.landed, held.is_dir, &held.created_files).await;
        journal::record_volume_transfer_source(
            self.state.scope(),
            src_vol,
            &held.source,
            dst_vol,
            &held.landed,
            held.is_dir,
            &held.created_files,
            (!held.is_dir).then_some(held.bytes as i64),
            &landed,
            held.overwrote,
        );
        journal::record_created_dirs_on(self.state.scope(), dst_vol, &held.created_dirs);
    }

    /// Returns unswept sources to the front of the queue, in copy order.
    fn put_back(&self, mut unswept: Vec<HeldSource>) {
        let mut held = self.held.lock_ignore_poison();
        unswept.append(&mut held);
        *held = unswept;
    }
}

fn cancelled() -> WriteOperationError {
    WriteOperationError::Cancelled {
        message: "Operation cancelled by user".into(),
    }
}
