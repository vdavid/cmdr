//! What one operation reaches its engine through: [`OpScope`].
//!
//! Built by the engine when the operation is (a managed op's rides on its
//! `WriteOperationState`, an instant op's is built beside `run_instant`), so
//! code deep inside an operation asks its scope for the host's seams, the status
//! cache, and its own journal, never a process-wide slot. A scope holds parts of
//! the engine, never the engine itself: the engine's registry holds the states
//! that hold the scopes, so a scope holding the engine would be a cycle.
//!
//! ❌ A missing scope never falls back to a detached log or another engine's:
//! that silently drops undo rows.

use std::sync::{Arc, OnceLock};

use cmdr_operation_log::writer::OpenOperation;
use cmdr_operation_log::{OpJournal, OperationLog};

use super::EngineStores;
use super::host::EngineHost;
use super::status_cache::StatusCache;

/// One operation's handle on its engine: the host's seams, the status cache,
/// the engine's stores, and the operation's journal once it opened.
pub(crate) struct OpScope {
    host: Arc<EngineHost>,
    status: Arc<StatusCache>,
    stores: Arc<EngineStores>,
    operation_log: OperationLog,
    /// Set once, when the operation opens its journal row. Unset for an
    /// operation that never opened one (a queued op cancelled before it ran, a
    /// direct-call test), whose record points then journal nothing.
    journal: OnceLock<OpJournal>,
    /// Whether this operation's rows record which node each item landed as,
    /// decided once from the destination's filesystem
    /// ([`Self::records_landed_nodes`]).
    landed_nodes: OnceLock<bool>,
    /// Holds this operation's next safe-overwrite between its two renames,
    /// once its test arms it.
    #[cfg(test)]
    aside_park: super::overwrite::aside_park::AsideParkSlot,
}

impl OpScope {
    pub(super) fn new(
        host: Arc<EngineHost>,
        status: Arc<StatusCache>,
        stores: Arc<EngineStores>,
        operation_log: OperationLog,
    ) -> Self {
        Self {
            host,
            status,
            stores,
            operation_log,
            journal: OnceLock::new(),
            landed_nodes: OnceLock::new(),
            #[cfg(test)]
            aside_park: Default::default(),
        }
    }

    /// Everything the engine reaches its host through.
    pub(crate) fn host(&self) -> &EngineHost {
        &self.host
    }

    /// The same host, shared, for work that outlives this call (a listener,
    /// a background loop), which holds it weakly.
    pub(super) fn host_arc(&self) -> &Arc<EngineHost> {
        &self.host
    }

    /// The engine's status cache.
    pub(super) fn status(&self) -> &StatusCache {
        &self.status
    }

    /// The engine's stores: previews, the in-flight ledger, outages, probes.
    pub(super) fn stores(&self) -> &EngineStores {
        &self.stores
    }

    /// Opens this operation's journal row, deciding once whether it's journaled.
    /// A second open keeps the first: an operation has one row.
    pub(super) fn open_journal(&self, open: OpenOperation) {
        let mut opened_now = false;
        self.journal.get_or_init(|| {
            opened_now = true;
            self.operation_log.journal_open(open)
        });
        if !opened_now {
            log::warn!(target: "operation_log", "an operation opened its journal twice; keeping the first");
        }
    }

    /// This operation's journal, once it opened one.
    pub(super) fn journal(&self) -> Option<&OpJournal> {
        self.journal.get()
    }

    /// Whether this operation's rows record which node each item landed as.
    /// `destination_keeps_nodes` answers for the destination's filesystem and is
    /// asked at most once per operation, and never for one that journals nothing:
    /// an operation has one destination, and the answer can cost a `statfs`.
    pub(super) fn records_landed_nodes(&self, destination_keeps_nodes: impl FnOnce() -> bool) -> bool {
        self.journal().is_some() && *self.landed_nodes.get_or_init(destination_keeps_nodes)
    }

    /// The test seam that parks this operation's next safe-overwrite with the
    /// original set aside.
    #[cfg(test)]
    pub(crate) fn aside_park(&self) -> &super::overwrite::aside_park::AsideParkSlot {
        &self.aside_park
    }
}
