//! Whether an op's tree summary can be read yet, and the registry of backfills
//! building one. The seam the tree view and the backfill meet at: the view asks
//! [`read_tree_summary_status`] and answers `Building` without knowing how a backfill
//! runs; the backfill reports into [`BackfillRegistry`].

use std::collections::HashMap;
use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;
use rusqlite::{Connection, OptionalExtension};

use crate::store::OperationLogStoreError;
use crate::types::{RollbackState, TreeSummaryState};

/// Where an op's tree summary stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeSummaryStatus {
    /// Exact: the tree reads it.
    Built,
    /// A backfill is building it: `done` of `total` rows read.
    Building(BackfillProgress),
    /// Not built, and nothing is building it, for this reason.
    Unbuilt(UnbuiltReason),
}

/// Why an op's summary isn't built and isn't building.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnbuiltReason {
    /// Nothing has built it yet (an op journaled before the summary existed, or one
    /// whose summary write failed). Opening its tree starts a backfill.
    NotBuilt,
    /// The operation is still running. A backfill can't start: inserts don't bump
    /// `outcome_generation`, so it couldn't tell rows that landed while it read.
    OperationRunning,
    /// A rollback of it is running. A backfill would restart on every outcome batch.
    RollingBack,
}

/// How far a running backfill got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackfillProgress {
    /// Rows read so far.
    pub(crate) done: u64,
    /// Rows the op holds.
    pub(crate) total: u64,
}

/// The backfills running now, by op id: one per op, so a second ask joins the first.
#[derive(Debug, Default)]
pub(crate) struct BackfillRegistry {
    running: Mutex<HashMap<String, BackfillProgress>>,
}

impl BackfillRegistry {
    /// The progress of `op_id`'s running backfill, if one is running.
    pub(crate) fn status(&self, op_id: &str) -> Option<BackfillProgress> {
        self.running.lock_ignore_poison().get(op_id).copied()
    }

    /// Record `op_id`'s backfill progress, registering it on the first report.
    pub(crate) fn report(&self, op_id: &str, progress: BackfillProgress) {
        self.running.lock_ignore_poison().insert(op_id.to_string(), progress);
    }

    /// Forget `op_id`'s backfill: it finished, stopped, or was cancelled.
    pub(crate) fn remove(&self, op_id: &str) {
        self.running.lock_ignore_poison().remove(op_id);
    }
}

/// Where `op_id`'s summary stands, `None` for an op with no header. In order: a built
/// summary reads as built whatever the op is doing (every op opened since the summary
/// existed is built from its first row); a running op or rollback says why nothing can
/// build it yet; then a running backfill's progress; else not built.
pub(crate) fn read_tree_summary_status(
    conn: &Connection,
    op_id: &str,
    registry: &BackfillRegistry,
) -> Result<Option<TreeSummaryStatus>, OperationLogStoreError> {
    let row = conn
        .prepare_cached("SELECT tree_summary, ended_at IS NULL, rollback_state FROM operations WHERE op_id = ?1")?
        .query_row(rusqlite::params![op_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .optional()?;
    let Some((summary, running, rollback_state)) = row else {
        return Ok(None);
    };
    if let Some(value) = summary {
        return match TreeSummaryState::from_token(&value) {
            Some(TreeSummaryState::Built) => Ok(Some(TreeSummaryStatus::Built)),
            None => Err(OperationLogStoreError::Decode {
                column: "tree_summary",
                value,
            }),
        };
    }
    if running {
        return Ok(Some(TreeSummaryStatus::Unbuilt(UnbuiltReason::OperationRunning)));
    }
    let rolling_back = RollbackState::from_token(&rollback_state) == Some(RollbackState::RollingBack);
    if rolling_back {
        return Ok(Some(TreeSummaryStatus::Unbuilt(UnbuiltReason::RollingBack)));
    }
    Ok(Some(match registry.status(op_id) {
        Some(progress) => TreeSummaryStatus::Building(progress),
        None => TreeSummaryStatus::Unbuilt(UnbuiltReason::NotBuilt),
    }))
}
