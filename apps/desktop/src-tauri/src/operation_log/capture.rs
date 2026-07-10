//! The capture layer: the `OperationJournal` trait and its production
//! (`WriterJournal`), disabled (`NoopJournal`), and test (`CapturingJournal`)
//! implementations, plus the pure eligibility (D3) and completeness (D4) logic.
//!
//! Capture rides the managed operation pipeline through a per-item observer at
//! the sink's altitude, NOT by decorating `OperationEventSink` (D4). The write
//! pipeline bundles this trait with the sink into an `OperationObservers` context
//! (defined in `write_operations`, which holds the sink) and threads it down the
//! same seam. This module owns the journal half: it feeds the [`writer`] and,
//! crucially, computes the two data-safety decisions the writer deliberately does
//! NOT (`writer.rs` stores terminal state, never judges it):
//!
//! - **Eligibility** ([`compute_eligibility`]): the per-kind rollback rule (D3),
//!   from the op `kind`, whether any item overwrote, and — for `archive_edit` —
//!   the driver-supplied subkind + net-new flag (Finding 3).
//! - **Completeness** ([`apply_completeness`]): the per-`row_role` issued-vs-
//!   written check (D4). A dropped/errored `rollback_unit` row degrades the op to
//!   `not_rollbackable(journal_incomplete)`; a dropped `search_only` row degrades
//!   `search_coverage` to `top_level_only(search_row_incomplete)`. This compares
//!   the `record_item` calls the op actually ISSUED (items reached) against the
//!   rows durably written — NOT the planned `item_count`, which a canceled op
//!   never reaches (Finding 1). So a canceled/failed op stays rollbackable for
//!   what it reached.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::ignore_poison::IgnorePoison;

use super::types::{
    ArchiveSubkind, ExecutionStatus, NotRollbackableReason, OpKind, RollbackState, RowRole, SearchCoverage,
    SearchCoverageReason,
};
use super::writer::{FinalizeOperation, FinalizeOutcome, JournalItem, OpenOperation, OperationLogWriter};

/// The per-item observer that journals a managed operation. Sibling to
/// `OperationEventSink` (UI events). The write pipeline reaches the installed
/// journal by `op_id` through the free functions in [`super`] (`journal_open` /
/// `journal_record_items` / `journal_finalize`), mirroring the op-keyed
/// `update_operation_status` status cache written at the same record points — a
/// recorded deviation from D4's threaded `OperationObservers` (whose hard
/// constraint, never extending `OperationEventSink`, is kept). Production installs
/// a [`WriterJournal`]; a test installs a [`CapturingJournal`]; when no journal is
/// installed (the DB failed to open), the free functions no-op.
///
/// The journal NEVER fails the operation: every method swallows its own errors
/// (logged), because the file operation's data safety outranks the journal's
/// completeness (D4).
pub trait OperationJournal: Send + Sync {
    /// Open the operation row (typically `Running`). Called once at op start.
    fn open(&self, open: OpenOperation);

    /// Record a batch of item rows for an open operation. Accumulates the
    /// issued-count and overwrote signals the finalize decisions need.
    fn record_items(&self, op_id: &str, items: Vec<JournalItem>);

    /// Downgrade this op's search coverage (worst-wins, idempotent). The trash /
    /// same-FS-move drivers call this when the `search_only` leaf subtree is
    /// capped / index-absent / stale / not-live (D-granularity). Default `Full`
    /// needs no call.
    fn note_search_coverage(&self, op_id: &str, coverage: SearchCoverage, reason: Option<SearchCoverageReason>);

    /// Finalize the operation: compute eligibility (D3) from the accumulated
    /// overwrote flags plus `inputs`, apply the per-`row_role` completeness
    /// downgrade (D4), store the terminal state, and return the durable per-role
    /// counts. Acts as a barrier for this op's prior records.
    fn finalize(&self, op_id: &str, inputs: FinalizeInputs) -> FinalizeOutcome;
}

/// The terminal inputs the CALLER supplies at finalize — the parts the journal
/// can't observe from item rows. Eligibility, coverage, and completeness are
/// computed by the journal from these plus its accumulated per-op state.
#[derive(Debug, Clone)]
pub struct FinalizeInputs {
    /// The op's lifecycle result (`done` / `failed` / `canceled`). A failed or
    /// canceled op still journals and stays rollbackable for what it reached
    /// (D4), so this does NOT force `not_rollbackable`.
    pub execution_status: ExecutionStatus,
    pub kind: OpKind,
    /// The `archive_edit` subkind, supplied by the capturing driver (Finding 3);
    /// `None` for non-archive ops.
    pub archive_subkind: Option<ArchiveSubkind>,
    /// For `archive_edit`/`compress`: was the archive net-new (vs overwriting a
    /// prior archive)? Ignored for other kinds.
    pub net_new: bool,
    pub ended_at: i64,
    /// The scanned planned total, refining the provisional count stored at `open`
    /// (the header-aggregate rider). `None` leaves the open-time value in place — the caller can't
    /// observe a real total.
    pub item_count: Option<u64>,
    pub items_done: u64,
    pub bytes_total: u64,
    pub dev_summary: Option<String>,
}

/// Per-`row_role` count of `record_item` calls the op ISSUED (items reached).
/// The D4 completeness yardstick — compared against the durably-written rows,
/// NOT the planned `item_count`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IssuedCounts {
    pub rollback_unit: u64,
    pub search_only: u64,
}

impl IssuedCounts {
    fn add(&mut self, role: RowRole, n: u64) {
        match role {
            RowRole::RollbackUnit => self.rollback_unit += n,
            RowRole::SearchOnly => self.search_only += n,
        }
    }
}

/// Compute the stored rollback eligibility from what actually happened (D3). Pure
/// so the per-kind table is tested in isolation. `any_overwrote` is true iff some
/// recorded item overwrote an existing destination; `archive_subkind` + `net_new`
/// are the driver-supplied archive facts (Finding 3, `None`/false otherwise).
///
/// Note: `execution_status` is deliberately NOT an input — a failed or canceled
/// op stays rollbackable for the items it reached (D4). Only the per-kind rule
/// and the overwrite/net-new facts decide eligibility.
pub fn compute_eligibility(
    kind: OpKind,
    any_overwrote: bool,
    archive_subkind: Option<ArchiveSubkind>,
    net_new: bool,
) -> (RollbackState, Option<NotRollbackableReason>) {
    match kind {
        // Deleting the copies / moving back is safe only if nothing was
        // overwritten (the originals would be gone).
        OpKind::Copy | OpKind::Move => {
            if any_overwrote {
                (RollbackState::NotRollbackable, Some(NotRollbackableReason::Overwrote))
            } else {
                (RollbackState::Rollbackable, None)
            }
        }
        // A permanent delete can't be restored.
        OpKind::Delete => (
            RollbackState::NotRollbackable,
            Some(NotRollbackableReason::PermanentDelete),
        ),
        // Restore-from-trash / rename-back / remove-if-net-new: the precondition
        // is rechecked at rollback time, so these open rollbackable.
        OpKind::Trash | OpKind::Rename | OpKind::CreateFolder | OpKind::CreateFile => {
            (RollbackState::Rollbackable, None)
        }
        OpKind::ArchiveEdit => match archive_subkind {
            // Compress: deleting the archive is safe only if it was net-new (an
            // overwrite discarded the prior bytes). The rollback rechecks the
            // archive is unchanged before deleting (the rollback engine, Finding 5).
            Some(ArchiveSubkind::Compress) => {
                if net_new {
                    (RollbackState::Rollbackable, None)
                } else {
                    (
                        RollbackState::NotRollbackable,
                        Some(NotRollbackableReason::ArchiveOverwrite),
                    )
                }
            }
            // Zip-inner edit + extract: no v1 rollback (result need not be
            // byte-identical; designed to become rollbackable later).
            Some(ArchiveSubkind::Edit) | Some(ArchiveSubkind::Extract) | None => (
                RollbackState::NotRollbackable,
                Some(NotRollbackableReason::ZipEditUnsupported),
            ),
        },
    }
}

/// Apply the per-`row_role` completeness downgrade (D4). A `rollback_unit`
/// shortfall (a dropped/errored reversal row) forces
/// `not_rollbackable(journal_incomplete)` — a missing row is invisible to
/// rollback, so a lossy journal must never claim rollbackability. A `search_only`
/// shortfall (a dropped search leaf) downgrades coverage to
/// `top_level_only(search_row_incomplete)`, never touching rollbackability. The
/// two populations are scoped independently so a dropped search leaf can't kill a
/// perfectly-journaled trash op's rollback, and a dropped reversal row can't be
/// masked as a mere coverage gap.
pub fn apply_completeness(
    state: RollbackState,
    reason: Option<NotRollbackableReason>,
    coverage: SearchCoverage,
    coverage_reason: Option<SearchCoverageReason>,
    issued: IssuedCounts,
    written: FinalizeOutcome,
) -> (
    RollbackState,
    Option<NotRollbackableReason>,
    SearchCoverage,
    Option<SearchCoverageReason>,
) {
    let (mut rb, mut rr) = (state, reason);
    if written.rollback_unit_rows < issued.rollback_unit {
        rb = RollbackState::NotRollbackable;
        rr = Some(NotRollbackableReason::JournalIncomplete);
    }
    let (mut cov, mut cvr) = (coverage, coverage_reason);
    if written.search_only_rows < issued.search_only && cov == SearchCoverage::Full {
        cov = SearchCoverage::TopLevelOnly;
        cvr = Some(SearchCoverageReason::SearchRowIncomplete);
    }
    (rb, rr, cov, cvr)
}

// ── Per-op accumulator ───────────────────────────────────────────────────────

/// Flush the per-op record buffer to the writer once it reaches this many rows,
/// so a huge op coalesces its inserts into batched transactions (D4) instead of
/// one writer message per leaf. The remainder flushes at finalize.
const RECORD_BATCH: usize = 512;

/// The per-op state the journal accumulates between `open` and `finalize`: the
/// issued counts (D4 yardstick), whether any item overwrote (D3), the worst
/// search-coverage the driver noted (D-granularity), and a small buffer that
/// batches item rows before they cross to the writer thread.
#[derive(Debug)]
struct OpAccum {
    issued: IssuedCounts,
    any_overwrote: bool,
    coverage: SearchCoverage,
    coverage_reason: Option<SearchCoverageReason>,
    buffer: Vec<JournalItem>,
    /// Monotonic per-op sequence, assigned in recording order so callers never
    /// track it. Files recorded during the op precede dirs recorded from the
    /// transaction at the end, so the dir rows land AFTER their contents (D2,
    /// Finding 2) and a `seq DESC` rollback removes files before their dirs.
    next_seq: i64,
}

impl Default for OpAccum {
    fn default() -> Self {
        Self {
            issued: IssuedCounts::default(),
            any_overwrote: false,
            coverage: SearchCoverage::Full,
            coverage_reason: None,
            buffer: Vec::new(),
            next_seq: 0,
        }
    }
}

// ── Production journal: feeds the writer ─────────────────────────────────────

/// The production journal: forwards to the single [`OperationLogWriter`] thread
/// and owns the eligibility + completeness decisions at finalize.
pub struct WriterJournal {
    writer: OperationLogWriter,
    /// Per-open-op accumulators, keyed by `op_id`. Removed at finalize.
    ops: Mutex<HashMap<String, OpAccum>>,
    /// Test-only: when set, the next `rollback_unit` item is counted as issued
    /// but NOT forwarded to the writer, simulating a dropped/errored row so the
    /// completeness path can be exercised end-to-end.
    #[cfg(test)]
    drop_next_rollback_row: std::sync::atomic::AtomicBool,
}

impl WriterJournal {
    pub fn new(writer: OperationLogWriter) -> Self {
        Self {
            writer,
            ops: Mutex::new(HashMap::new()),
            #[cfg(test)]
            drop_next_rollback_row: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Test seam: drop the next `rollback_unit` row on the floor (still counted
    /// as issued), to exercise the D4 `journal_incomplete` downgrade.
    #[cfg(test)]
    pub fn arm_drop_next_rollback_row(&self) {
        self.drop_next_rollback_row
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl OperationJournal for WriterJournal {
    fn open(&self, open: OpenOperation) {
        self.ops
            .lock_ignore_poison()
            .insert(open.op_id.clone(), OpAccum::default());
        if let Err(e) = self.writer.open_operation(open) {
            log::warn!(target: "operation_log", "journal open failed: {e}");
        }
    }

    fn record_items(&self, op_id: &str, items: Vec<JournalItem>) {
        if items.is_empty() {
            return;
        }
        // Accumulate the issued counts + overwrote signal for the finalize
        // decisions, buffer the rows, and drain a full batch out under the lock
        // (a batched writer message instead of one per leaf — D4).
        let batch = {
            let mut ops = self.ops.lock_ignore_poison();
            let accum = ops.entry(op_id.to_string()).or_default();
            for mut item in items {
                // Assign the op-monotonic seq here so record points never track
                // it; recording order IS the seq order.
                item.seq = accum.next_seq;
                accum.next_seq += 1;
                accum.issued.add(item.row_role, 1);
                accum.any_overwrote |= item.overwrote;
                #[cfg(test)]
                if item.row_role == RowRole::RollbackUnit
                    && self
                        .drop_next_rollback_row
                        .swap(false, std::sync::atomic::Ordering::SeqCst)
                {
                    // Counted as issued above, but not buffered: a simulated drop.
                    continue;
                }
                accum.buffer.push(item);
            }
            if accum.buffer.len() >= RECORD_BATCH {
                Some(std::mem::take(&mut accum.buffer))
            } else {
                None
            }
        };
        // Forward OUTSIDE the ops lock: `record_items` can block briefly under
        // writer backpressure, and holding the per-op map lock across that would
        // serialize unrelated ops.
        if let Some(batch) = batch
            && let Err(e) = self.writer.record_items(op_id, batch)
        {
            log::warn!(target: "operation_log", "journal record_items failed: {e}");
        }
    }

    fn note_search_coverage(&self, op_id: &str, coverage: SearchCoverage, reason: Option<SearchCoverageReason>) {
        // Worst-wins: only a downgrade FROM `Full` sticks (there is no coverage
        // state below `TopLevelOnly`, so the first downgrade's reason is kept).
        if coverage == SearchCoverage::Full {
            return;
        }
        let mut ops = self.ops.lock_ignore_poison();
        let accum = ops.entry(op_id.to_string()).or_default();
        if accum.coverage == SearchCoverage::Full {
            accum.coverage = coverage;
            accum.coverage_reason = reason;
        }
    }

    fn finalize(&self, op_id: &str, inputs: FinalizeInputs) -> FinalizeOutcome {
        let accum = self.ops.lock_ignore_poison().remove(op_id).unwrap_or_default();

        // Flush the buffered tail BEFORE finalize so the writer commits every
        // record before it counts rows (messages are processed in FIFO order).
        if !accum.buffer.is_empty()
            && let Err(e) = self.writer.record_items(op_id, accum.buffer)
        {
            log::warn!(target: "operation_log", "journal finalize flush failed: {e}");
        }

        let (state, reason) =
            compute_eligibility(inputs.kind, accum.any_overwrote, inputs.archive_subkind, inputs.net_new);

        let finalize = FinalizeOperation {
            op_id: op_id.to_string(),
            execution_status: inputs.execution_status,
            rollback_state: state,
            not_rollbackable_reason: reason,
            archive_subkind: inputs.archive_subkind,
            search_coverage: accum.coverage,
            search_coverage_reason: accum.coverage_reason,
            ended_at: inputs.ended_at,
            item_count: inputs.item_count,
            items_done: inputs.items_done,
            bytes_total: inputs.bytes_total,
            dev_summary: inputs.dev_summary,
        };

        let outcome = match self.writer.finalize_operation(finalize.clone()) {
            Ok(o) => o,
            Err(e) => {
                log::warn!(target: "operation_log", "journal finalize failed: {e}");
                return FinalizeOutcome {
                    rollback_unit_rows: 0,
                    search_only_rows: 0,
                };
            }
        };

        // Completeness (D4): downgrade if the durable rows fall short of issued.
        // Rare (only a real drop/DB error), so the correcting re-finalize is rare.
        let (rb, rr, cov, cvr) = apply_completeness(
            state,
            reason,
            accum.coverage,
            accum.coverage_reason,
            accum.issued,
            outcome,
        );
        if (rb, rr, cov, cvr) != (state, reason, accum.coverage, accum.coverage_reason) {
            let corrected = FinalizeOperation {
                rollback_state: rb,
                not_rollbackable_reason: rr,
                search_coverage: cov,
                search_coverage_reason: cvr,
                ..finalize
            };
            if let Err(e) = self.writer.finalize_operation(corrected) {
                log::warn!(target: "operation_log", "journal completeness re-finalize failed: {e}");
            }
        }
        outcome
    }
}

// ── Test journal: captures calls for assertions ──────────────────────────────

/// A test journal that records every call for assertions, with no DB. Lets a
/// pipeline test install a journal without a sink or a store.
#[cfg(test)]
#[derive(Default)]
pub struct CapturingJournal {
    pub opens: Mutex<Vec<OpenOperation>>,
    pub items: Mutex<Vec<JournalItem>>,
    pub finalizes: Mutex<Vec<(String, FinalizeInputs)>>,
    pub coverage_notes: Mutex<Vec<(String, SearchCoverage, Option<SearchCoverageReason>)>>,
}

#[cfg(test)]
impl CapturingJournal {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
impl OperationJournal for CapturingJournal {
    fn open(&self, open: OpenOperation) {
        self.opens.lock_ignore_poison().push(open);
    }
    fn record_items(&self, _op_id: &str, items: Vec<JournalItem>) {
        self.items.lock_ignore_poison().extend(items);
    }
    fn note_search_coverage(&self, op_id: &str, coverage: SearchCoverage, reason: Option<SearchCoverageReason>) {
        self.coverage_notes
            .lock_ignore_poison()
            .push((op_id.to_string(), coverage, reason));
    }
    fn finalize(&self, op_id: &str, inputs: FinalizeInputs) -> FinalizeOutcome {
        let counts = {
            let items = self.items.lock_ignore_poison();
            let mut c = IssuedCounts::default();
            for it in items.iter().filter(|i| i.seq >= 0) {
                c.add(it.row_role, 1);
            }
            c
        };
        self.finalizes.lock_ignore_poison().push((op_id.to_string(), inputs));
        FinalizeOutcome {
            rollback_unit_rows: counts.rollback_unit,
            search_only_rows: counts.search_only,
        }
    }
}

#[cfg(test)]
mod tests;
