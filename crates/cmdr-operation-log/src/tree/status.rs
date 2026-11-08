//! Whether an op's tree summary can be read yet, and the registry of backfills
//! building one. The seam the tree view and the backfill meet at: the view asks
//! [`read_tree_summary_status`] and answers `Building` without knowing how a backfill
//! runs; the backfill (`backfill.rs`) claims its op here, reports its progress here,
//! and hears a cancel here. The registry also says when the journal's folder indexes
//! exist, so a tree waiting on them hears it rather than asking again.

use std::collections::HashMap;
use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;
use rusqlite::{Connection, OptionalExtension};

use crate::store::{OperationLogStoreError, tree_item_indexes_exist};
use crate::types::{RollbackState, TreeSummaryState};

/// Where an op's tree summary stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeSummaryStatus {
    /// The writer is still building the folder indexes every tree reads (once, after
    /// the upgrade that added them; `store/tree_indexes.rs`). Ahead of any op's state.
    IndexesBuilding,
    /// Exact: the tree reads it.
    Built,
    /// A backfill is building it: `done` of `total` rows read.
    Building(BackfillProgress),
    /// Not built, and nothing is building it, for this reason.
    Unbuilt(UnbuiltReason),
}

/// Why an op's summary isn't built and isn't building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UnbuiltReason {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackfillProgress {
    /// Rows read so far.
    pub done: u64,
    /// Rows the op holds.
    pub total: u64,
}

/// The backfills running now, by op id: one per op, so a second ask joins the first.
/// The host reaches its log's one registry through `OperationLog::tree_backfills` and
/// hands it to every tree read, which answers `building` from it. It also holds who's
/// waiting for the folder indexes ([`Self::when_indexes_ready`]).
#[derive(Debug, Default)]
pub struct BackfillRegistry {
    running: Mutex<HashMap<String, Running>>,
    indexes: Mutex<IndexesWatch>,
}

/// Whether the writer has built the folder indexes, and who waits for it until then.
#[derive(Default)]
struct IndexesWatch {
    ready: bool,
    waiting: Vec<Box<dyn FnOnce() + Send>>,
}

impl std::fmt::Debug for IndexesWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexesWatch")
            .field("ready", &self.ready)
            .field("waiting", &self.waiting.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy)]
struct Running {
    progress: BackfillProgress,
    /// Asked to stop; the backfill stops at its next chunk unless someone joins first.
    cancelled: bool,
}

/// What [`BackfillRegistry::claim`] answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Claim {
    /// Nobody was building this op's summary; the caller is now.
    Claimed,
    /// A backfill was already running; the caller joined it (and took back any cancel
    /// asked of it, since someone wants it again).
    Joined,
}

impl BackfillRegistry {
    /// The progress of `op_id`'s running backfill, if one is running.
    pub(crate) fn status(&self, op_id: &str) -> Option<BackfillProgress> {
        self.running.lock_ignore_poison().get(op_id).map(|r| r.progress)
    }

    /// Record `op_id`'s backfill progress, registering it on the first report.
    pub(crate) fn report(&self, op_id: &str, progress: BackfillProgress) {
        self.running
            .lock_ignore_poison()
            .entry(op_id.to_string())
            .and_modify(|r| r.progress = progress)
            .or_insert(Running {
                progress,
                cancelled: false,
            });
    }

    /// Forget `op_id`'s backfill: it finished, stopped, or was cancelled.
    pub(crate) fn remove(&self, op_id: &str) {
        self.running.lock_ignore_poison().remove(op_id);
    }

    /// Claim `op_id` for a new backfill, or join the one running.
    pub(crate) fn claim(&self, op_id: &str) -> Claim {
        let mut running = self.running.lock_ignore_poison();
        if let Some(existing) = running.get_mut(op_id) {
            existing.cancelled = false;
            return Claim::Joined;
        }
        running.insert(
            op_id.to_string(),
            Running {
                progress: BackfillProgress { done: 0, total: 0 },
                cancelled: false,
            },
        );
        Claim::Claimed
    }

    /// Ask `op_id`'s running backfill to stop at its next step (the dialog closed, or
    /// moved to another op). `false` when none is running. A start that comes first
    /// takes the cancel back.
    pub fn cancel(&self, op_id: &str) -> bool {
        match self.running.lock_ignore_poison().get_mut(op_id) {
            Some(running) => {
                running.cancelled = true;
                true
            }
            None => false,
        }
    }

    /// Run `notify` once the journal's folder indexes exist: at once when they already
    /// do, else on the writer thread right after it builds them (`store/tree_indexes.rs`).
    /// The host turns it into an event, so a tree answering `indexesBuilding` reads
    /// again when it can, ❌ never on a timer. `notify` must not block on the writer
    /// (it runs there). A build that fails never calls it: the tree keeps saying the
    /// indexes are building, which is what a read would answer too.
    pub fn when_indexes_ready(&self, notify: Box<dyn FnOnce() + Send>) {
        let mut watch = self.indexes.lock_ignore_poison();
        if watch.ready {
            drop(watch);
            notify();
            return;
        }
        watch.waiting.push(notify);
    }

    /// The writer built the folder indexes (or found them built): wake everyone waiting,
    /// outside the lock, and answer every later [`Self::when_indexes_ready`] at once.
    pub(crate) fn mark_indexes_ready(&self) {
        let waiting = {
            let mut watch = self.indexes.lock_ignore_poison();
            watch.ready = true;
            std::mem::take(&mut watch.waiting)
        };
        for notify in waiting {
            notify();
        }
    }

    /// Whether `op_id`'s backfill was asked to stop; if so it's forgotten in the same
    /// lock, so a [`Self::claim`] that comes after starts afresh instead of joining a
    /// backfill that's stopping.
    pub(crate) fn stop_if_cancelled(&self, op_id: &str) -> bool {
        let mut running = self.running.lock_ignore_poison();
        if running.get(op_id).is_some_and(|r| r.cancelled) {
            running.remove(op_id);
            return true;
        }
        false
    }
}

/// Where `op_id`'s summary stands, `None` for an op with no header. In order: the
/// journal's folder indexes still building (no tree can read a folder yet); a built
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
    if !tree_item_indexes_exist(conn)? {
        return Ok(Some(TreeSummaryStatus::IndexesBuilding));
    }
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
