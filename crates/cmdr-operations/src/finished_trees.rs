//! Finished operations' transfer trees, kept answerable after the operation
//! settles so the dialog showing one can pull its FINAL rows.
//!
//! An operation's state (and the tree on it) goes when it settles. A view pulls
//! at most every 200 ms, so a transfer that ends faster than that (a server-side
//! copy of 36 files, #702) settles before the pull that would have shown its end
//! comes back, and the done dialog sat on rows that said every file was still
//! waiting. So the manager hands each tree here as it settles, and the tree
//! stays until the dialog showing it closes (`release`) or, should that never
//! come, the safety TTL runs out. The memory it holds then lasts about as long
//! as the dialog shows it, the same as during the run.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_operation_log::types::OperationId;
use tokio::runtime::Handle;

use super::transfer_tree::{TransferTree, TransferTreeView, TransferTreeViewRequest};
use super::types::ProgressAxis;

/// How long a finished tree waits for its release before it goes on its own:
/// longer than anyone reads a done dialog, short enough that a lost release
/// (a window that went away mid-close) can't hold a big tree for the session.
const FINISHED_TREE_TTL: Duration = Duration::from_secs(10 * 60);

/// One settled operation's tree, as its last view saw it.
struct Finished {
    tree: Arc<TransferTree>,
    /// The axis its folder bars filled along when it settled: there are no more
    /// progress ticks to read it from.
    axis: ProgressAxis,
}

/// The engine's finished trees, by operation id. One per engine, on its manager.
pub(crate) struct FinishedTrees {
    entries: Mutex<HashMap<OperationId, Finished>>,
    ttl: Mutex<Duration>,
}

impl FinishedTrees {
    pub(crate) fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl: Mutex::new(FINISHED_TREE_TTL),
        }
    }

    /// Keeps `tree` for `operation_id`, which just settled, and arms its TTL on
    /// `runtime`. The timer holds the store weakly, so a dropped engine ends it.
    pub(crate) fn keep(
        self: &Arc<Self>,
        runtime: &Handle,
        operation_id: OperationId,
        tree: Arc<TransferTree>,
        axis: ProgressAxis,
    ) {
        self.entries
            .lock_ignore_poison()
            .insert(operation_id.clone(), Finished { tree, axis });
        let ttl = *self.ttl.lock_ignore_poison();
        let store: Weak<Self> = Arc::downgrade(self);
        runtime.spawn(async move {
            crate::rt::time::sleep(ttl).await;
            if let Some(store) = store.upgrade()
                && store.entries.lock_ignore_poison().remove(&operation_id).is_some()
            {
                log::debug!(target: "cmdr_operations", "transfer tree of op={operation_id} expired unreleased");
            }
        });
    }

    /// The window `request` asks for of a settled operation's tree, or `None`
    /// when none is kept (never kept, released, or expired). A tree a bug
    /// turned off answers `ready: false`, as it did while it ran.
    pub(crate) fn view(
        &self,
        operation_id: &OperationId,
        request: &TransferTreeViewRequest,
    ) -> Option<TransferTreeView> {
        let (tree, axis) = {
            let entries = self.entries.lock_ignore_poison();
            let finished = entries.get(operation_id)?;
            (Arc::clone(&finished.tree), finished.axis)
        };
        // Outside the store's lock: a big tree's window takes the tree's own.
        Some(
            tree.view(request, axis)
                .unwrap_or_else(|| TransferTreeView::not_ready(axis)),
        )
    }

    /// Lets `operation_id`'s tree go. A no-op for one not kept, including one
    /// still running: its tree is the operation's until it settles.
    pub(crate) fn release(&self, operation_id: &OperationId) {
        self.entries.lock_ignore_poison().remove(operation_id);
    }

    /// Shortens the TTL for trees kept from now on, so a test waits
    /// milliseconds for an expiry.
    #[cfg(test)]
    pub(crate) fn set_ttl_for_test(&self, ttl: Duration) {
        *self.ttl.lock_ignore_poison() = ttl;
    }
}
