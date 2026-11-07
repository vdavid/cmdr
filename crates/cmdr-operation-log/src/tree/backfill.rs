//! Building the tree summary of an op journaled before the summary existed, the
//! first time someone opens its tree (`DETAILS.md` § "Tree summary").
//!
//! A [`TreeBackfill`] is a resumable chunk loop the HOST drives, one [`TreeBackfill::step`]
//! at a time: the desktop on a blocking task, the browser demo between turns of the
//! page's event loop (it has no thread). Each step reads one `seq` chunk on the job's
//! own read connection and folds it into an in-memory per-folder delta (folders are
//! few, rows many). The writer gets only two small messages: "intern these pre-v6
//! DIR rows' own folders" and, at the end, the aggregated rows.
//!
//! Races, each answered by the writer inside its transaction:
//! - a rollback lands outcomes while the job reads: `outcome_generation` moved, the
//!   writer answers `Stale`, and the job starts over (or stops, if the op is now
//!   `rolling_back`);
//! - retention prunes the op: `Gone`, nothing written;
//! - something else built it: `AlreadyBuilt`.
//!
//! The registry (`status.rs`) makes it one backfill per op: a second start joins,
//! a cancel stops the job at its next step, and the job forgets its claim when it
//! ends or is dropped.

use std::sync::Arc;

use rusqlite::Connection;

use super::status::{BackfillProgress, BackfillRegistry, Claim, UnbuiltReason};
use super::summary::{AGGREGATE_CHUNK, OpTreeContext, RowFacts, TreeDelta, read_facts_after, read_op_tree_context};
use crate::OperationLog;
use crate::store::{OperationLogStoreError, open_read_connection};
use crate::types::{EntryType, RollbackState};
use crate::writer::{OperationLogWriter, TreeSummaryWrite};

/// What asking for an op's backfill answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum TreeBackfillStart {
    /// A backfill started; its progress and end arrive as events.
    Started,
    /// One was already running; its events keep coming.
    Joined,
    /// The summary is already built: read the tree.
    Built,
    /// It can't be built yet, for this reason (a running operation or rollback).
    Unbuilt(UnbuiltReason),
    /// No operation with this id in the journal (or no journal).
    UnknownOperation,
}

/// How a backfill ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum TreeBackfillEnd {
    /// The summary is built: read the tree.
    Built,
    /// Cancelled before it finished; nothing was stamped, and the next start begins
    /// afresh.
    Cancelled,
    /// The operation is gone (retention pruned it meanwhile).
    Gone,
    /// It can't be built now, for this reason (a rollback started meanwhile).
    Unbuilt(UnbuiltReason),
    /// The journal couldn't be read or written; the details are in the log. The op
    /// stays unbuilt, so opening its tree again retries.
    Unreadable,
}

/// One step of a backfill, as the host hears it and passes it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum TreeBackfillStep {
    /// Still reading: `done` of `total` rows so far. A restart after a rollback landed
    /// starts `done` from zero again.
    Progress(BackfillProgress),
    /// Over; further steps answer the same.
    Finished(TreeBackfillEnd),
}

/// The event payload a host emits for every step: which op, and the step.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct TreeBackfillEvent {
    pub op_id: String,
    pub step: TreeBackfillStep,
}

/// One op's backfill, claimed in the registry until it ends or is dropped. Drive it
/// with [`Self::step`] until it answers [`TreeBackfillStep::Finished`].
pub struct TreeBackfill {
    op_id: String,
    conn: Connection,
    writer: OperationLogWriter,
    registry: Arc<BackfillRegistry>,
    /// The pass in flight; `None` before the first step and after a stale result.
    pass: Option<Pass>,
    end: Option<TreeBackfillEnd>,
}

/// One read through the op: the generation it started at, where it got to, and what
/// it gathered.
struct Pass {
    context: OpTreeContext,
    generation: i64,
    after: i64,
    progress: BackfillProgress,
    delta: TreeDelta,
}

impl TreeBackfill {
    /// Ask for `op_id`'s tree summary on `log`: a fresh backfill comes back as the job
    /// to drive, with [`TreeBackfillStart::Started`]; every other answer comes with none
    /// (joined the running one, already built, can't yet, unknown). Before the log
    /// opens, every op is unknown. Cancel through `log.tree_backfills()`.
    pub fn start(
        log: &OperationLog,
        op_id: &str,
    ) -> Result<(TreeBackfillStart, Option<TreeBackfill>), OperationLogStoreError> {
        match log.writer() {
            Some(writer) => Self::start_on(&writer, log.tree_backfills(), op_id),
            None => Ok((TreeBackfillStart::UnknownOperation, None)),
        }
    }

    /// [`Self::start`] on a writer and registry of the caller's own (the tests').
    pub(crate) fn start_on(
        writer: &OperationLogWriter,
        registry: &Arc<BackfillRegistry>,
        op_id: &str,
    ) -> Result<(TreeBackfillStart, Option<TreeBackfill>), OperationLogStoreError> {
        let conn = open_read_connection(writer.db_path())?;
        match read_readiness(&conn, op_id)? {
            Readiness::Gone => return Ok((TreeBackfillStart::UnknownOperation, None)),
            Readiness::Built => return Ok((TreeBackfillStart::Built, None)),
            Readiness::Blocked(reason) => return Ok((TreeBackfillStart::Unbuilt(reason), None)),
            Readiness::Ready { .. } => {}
        }
        if registry.claim(op_id) == Claim::Joined {
            return Ok((TreeBackfillStart::Joined, None));
        }
        let job = TreeBackfill {
            op_id: op_id.to_string(),
            conn,
            writer: writer.clone(),
            registry: Arc::clone(registry),
            pass: None,
            end: None,
        };
        Ok((TreeBackfillStart::Started, Some(job)))
    }

    /// The op this backfill builds.
    pub fn op_id(&self) -> &str {
        &self.op_id
    }

    /// Read one more chunk, or finish. Every step ends quickly (one chunk of
    /// [`AGGREGATE_CHUNK`] rows, or the final write), so a host can check for its own
    /// shutdown between steps.
    pub fn step(&mut self) -> TreeBackfillStep {
        if let Some(end) = self.end {
            return TreeBackfillStep::Finished(end);
        }
        let step = self.advance().unwrap_or_else(|e| {
            log::warn!(target: "operation_log", "tree backfill({}): {e}", self.op_id);
            TreeBackfillStep::Finished(TreeBackfillEnd::Unreadable)
        });
        match step {
            TreeBackfillStep::Progress(progress) => self.registry.report(&self.op_id, progress),
            TreeBackfillStep::Finished(end) => self.finish(end),
        }
        step
    }

    fn advance(&mut self) -> Result<TreeBackfillStep, OperationLogStoreError> {
        if self.registry.stop_if_cancelled(&self.op_id) {
            return Ok(TreeBackfillStep::Finished(TreeBackfillEnd::Cancelled));
        }
        let Some(pass) = self.pass.as_mut() else {
            return self.begin_pass();
        };
        let mut chunk = read_facts_after(&self.conn, &self.op_id, pass.after, AGGREGATE_CHUNK)?;
        let Some(last) = chunk.last().map(|facts| facts.seq) else {
            return self.write_result();
        };
        let unplaced: Vec<i64> = chunk
            .iter()
            .filter(|facts| lacks_own_folder(facts))
            .map(|f| f.seq)
            .collect();
        if !unplaced.is_empty() {
            // Rows journaled before the summary existed have no own folder: the writer
            // interns them (a read connection can't), then the chunk is read again.
            self.writer.fill_self_dirs(&self.op_id, unplaced)?;
            chunk = read_facts_after(&self.conn, &self.op_id, pass.after, AGGREGATE_CHUNK)?;
            chunk.retain(|facts| facts.seq <= last);
        }
        for facts in &chunk {
            pass.delta.add_row(pass.context.kind, pass.context.axis, facts, 1);
        }
        pass.after = last;
        pass.progress.done += chunk.len() as u64;
        Ok(TreeBackfillStep::Progress(pass.progress))
    }

    /// Start (or restart) a read through the op: why it can't, or its generation and
    /// row count. The generation is read BEFORE the first chunk, so an outcome batch
    /// that lands any time after makes the result stale.
    fn begin_pass(&mut self) -> Result<TreeBackfillStep, OperationLogStoreError> {
        let (context, generation) = match read_readiness(&self.conn, &self.op_id)? {
            Readiness::Gone => return Ok(TreeBackfillStep::Finished(TreeBackfillEnd::Gone)),
            Readiness::Built => return Ok(TreeBackfillStep::Finished(TreeBackfillEnd::Built)),
            Readiness::Blocked(reason) => return Ok(TreeBackfillStep::Finished(TreeBackfillEnd::Unbuilt(reason))),
            Readiness::Ready { context, generation } => (context, generation),
        };
        let total: i64 = self
            .conn
            .prepare_cached("SELECT COUNT(*) FROM operation_items WHERE op_id = ?1")?
            .query_row(rusqlite::params![self.op_id], |row| row.get(0))?;
        let progress = BackfillProgress {
            done: 0,
            total: total as u64,
        };
        self.pass = Some(Pass {
            context,
            generation,
            after: i64::MIN,
            progress,
            delta: TreeDelta::default(),
        });
        Ok(TreeBackfillStep::Progress(progress))
    }

    fn write_result(&mut self) -> Result<TreeBackfillStep, OperationLogStoreError> {
        let Some(pass) = self.pass.take() else {
            return self.begin_pass();
        };
        let rows = match pass.delta.into_rows(&self.conn) {
            Ok(rows) => rows,
            // Retention pruned the op mid-read and took the folders it alone kept, so
            // the chain walk finds a dir gone. That's the op gone, not a broken journal.
            Err(_) if read_op_tree_context(&self.conn, &self.op_id)?.is_none() => {
                return Ok(TreeBackfillStep::Finished(TreeBackfillEnd::Gone));
            }
            Err(e) => return Err(e),
        };
        Ok(
            match self.writer.write_tree_summary(&self.op_id, pass.generation, rows)? {
                Some(TreeSummaryWrite::Built | TreeSummaryWrite::AlreadyBuilt) => {
                    TreeBackfillStep::Finished(TreeBackfillEnd::Built)
                }
                Some(TreeSummaryWrite::Gone) => TreeBackfillStep::Finished(TreeBackfillEnd::Gone),
                // A rollback landed outcomes while this pass read: read again (the restart
                // stops instead if the op is still rolling back).
                Some(TreeSummaryWrite::Stale) => return self.begin_pass(),
                None => TreeBackfillStep::Finished(TreeBackfillEnd::Unreadable),
            },
        )
    }

    fn finish(&mut self, end: TreeBackfillEnd) {
        self.end = Some(end);
        self.pass = None;
        // A cancel already forgot the claim, in the same lock that saw it
        // (`stop_if_cancelled`); removing again could drop a newer backfill's claim.
        if end != TreeBackfillEnd::Cancelled {
            self.registry.remove(&self.op_id);
        }
    }
}

impl Drop for TreeBackfill {
    /// A job the host dropped mid-way (its task stopped) frees its op, so the next
    /// start begins afresh.
    fn drop(&mut self) {
        if self.end.is_none() {
            self.registry.remove(&self.op_id);
        }
    }
}

/// Whether a backfill can read `op_id` now, from its own row alone.
enum Readiness {
    /// No such op.
    Gone,
    /// Its summary is already built.
    Built,
    /// It can't be read exactly yet.
    Blocked(UnbuiltReason),
    /// Readable: its kind and axis, and the generation a result must still match.
    Ready { context: OpTreeContext, generation: i64 },
}

/// The op's own readiness, deliberately NOT `read_tree_summary_status`: that one puts
/// the journal's folder indexes first, and a backfill doesn't need them (it reads by
/// `(op_id, seq)`, and its one write queues behind their build on the writer).
fn read_readiness(conn: &Connection, op_id: &str) -> Result<Readiness, OperationLogStoreError> {
    let Some(context) = read_op_tree_context(conn, op_id)? else {
        return Ok(Readiness::Gone);
    };
    if context.built {
        return Ok(Readiness::Built);
    }
    let (running, rolling_back, generation) = conn
        .prepare_cached(
            "SELECT ended_at IS NULL, rollback_state = ?2, outcome_generation FROM operations WHERE op_id = ?1",
        )?
        .query_row(rusqlite::params![op_id, RollbackState::RollingBack.as_token()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    Ok(if running {
        Readiness::Blocked(UnbuiltReason::OperationRunning)
    } else if rolling_back {
        Readiness::Blocked(UnbuiltReason::RollingBack)
    } else {
        Readiness::Ready { context, generation }
    })
}

/// A DIR row journaled before the summary existed: it has no own folder yet.
fn lacks_own_folder(facts: &RowFacts) -> bool {
    facts.entry_type == EntryType::Dir && facts.self_dir_id.is_none()
}
