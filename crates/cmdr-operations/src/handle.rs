//! Building an [`Operations`] engine, and reaching its parts.
//!
//! The host passes the two seams nothing works without to
//! [`Operations::builder`] (an engine with no volumes writes nothing, one with
//! no events leaves every dialog on "Copying…") and may replace any other with
//! a builder method; the rest default to answers that do nothing.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_operation_log::OperationLog;

use super::host::{
    EngineEvents, EngineHost, NameOrder, OperationIndex, OperationSinkFactory, OperationVolumes, OwnWrites, Pacing,
};
use super::manager::OperationManager;
use super::scope::OpScope;
use super::state::WriteOperationRegistry;
use super::state::WriteOperationState;
use super::status_cache::StatusCache;
use super::types::OperationStatus;
use super::{EngineStores, Inner, Operations};
use cmdr_operation_log::types::OperationId;

/// The seams an [`Operations`] is built with. From [`Operations::builder`].
pub struct OperationsBuilder {
    volumes: Arc<dyn OperationVolumes>,
    events: Arc<dyn EngineEvents>,
    volume_host: VolumeHost,
    operation_log: OperationLog,
    operation_sinks: Option<OperationSinkFactory>,
    own_writes: Option<Arc<dyn OwnWrites>>,
    index: Option<Arc<dyn OperationIndex>>,
    pacing: Option<Arc<dyn Pacing>>,
    name_order: Option<Arc<dyn NameOrder>>,
}

impl OperationsBuilder {
    /// The volume host whose runtime the engine spawns on and whose listings
    /// it asks. Defaults to [`VolumeHost::detached`], whose runtime is
    /// `cmdr-fs`'s shared fallback: ❌ never a `Handle::current()` captured at
    /// build, which would orphan every operation admitted onto a runtime torn
    /// down later.
    pub fn volume_host(mut self, volume_host: VolumeHost) -> Self {
        self.volume_host = volume_host;
        self
    }

    /// The operation log every operation journals into. Defaults to a detached
    /// one, which journals nothing.
    pub fn operation_log(mut self, operation_log: OperationLog) -> Self {
        self.operation_log = operation_log;
        self
    }

    /// How to build a sink for an operation the engine starts on its own
    /// behalf. Without one, such an operation refuses plainly.
    pub fn operation_sinks(mut self, factory: OperationSinkFactory) -> Self {
        self.operation_sinks = Some(factory);
        self
    }

    /// Who hears about every path the engine is about to write.
    pub fn own_writes(mut self, own_writes: Arc<dyn OwnWrites>) -> Self {
        self.own_writes = Some(own_writes);
        self
    }

    /// What answers the engine's drive-index questions.
    pub fn index(mut self, index: Arc<dyn OperationIndex>) -> Self {
        self.index = Some(index);
        self
    }

    /// How long the engine waits where it deliberately waits.
    pub fn pacing(mut self, pacing: Arc<dyn Pacing>) -> Self {
        self.pacing = Some(pacing);
        self
    }

    /// The collator the panes sort names with.
    pub fn name_order(mut self, name_order: Arc<dyn NameOrder>) -> Self {
        self.name_order = Some(name_order);
        self
    }

    /// The engine. It starts nothing and reads no file until asked.
    pub fn build(self) -> Operations {
        let mut host = EngineHost::new(self.volume_host, self.volumes, self.events);
        if let Some(factory) = self.operation_sinks {
            host = host.with_operation_sinks(factory);
        }
        if let Some(own_writes) = self.own_writes {
            host = host.with_own_writes(own_writes);
        }
        if let Some(index) = self.index {
            host = host.with_index(index);
        }
        if let Some(pacing) = self.pacing {
            host = host.with_pacing(pacing);
        }
        if let Some(name_order) = self.name_order {
            host = host.with_name_order(name_order);
        }
        Operations::from_host(host, self.operation_log)
    }
}

impl Operations {
    /// An engine over `volumes`, reporting engine-wide facts to `events`.
    pub fn builder(volumes: Arc<dyn OperationVolumes>, events: Arc<dyn EngineEvents>) -> OperationsBuilder {
        OperationsBuilder {
            volumes,
            events,
            volume_host: VolumeHost::detached(),
            operation_log: OperationLog::detached(),
            operation_sinks: None,
            own_writes: None,
            index: None,
            pacing: None,
            name_order: None,
        }
    }

    /// An engine with every seam answering nothing: no volumes, no events, no
    /// journal, and `cmdr-fs`'s shared fallback runtime. For tools and benches.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn detached() -> Self {
        Self::detached_journaling_into(OperationLog::detached())
    }

    /// [`Self::detached`], journaling into `operation_log`: a bench that puts the
    /// journal on its measured path.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn detached_journaling_into(operation_log: OperationLog) -> Self {
        Self::from_host(EngineHost::detached(), operation_log)
    }

    fn from_host(host: EngineHost, operation_log: OperationLog) -> Self {
        let host = Arc::new(host);
        let registry = Arc::new(WriteOperationRegistry::new());
        let status = Arc::new(StatusCache::new(Arc::clone(host.events_arc())));
        let stores = Arc::new(EngineStores::new());
        Self {
            inner: Arc::new_cyclic(|engine| Inner {
                manager: Arc::new(OperationManager::new(
                    Arc::clone(&host),
                    Arc::clone(&registry),
                    Arc::clone(&status),
                    Arc::clone(stores.previews_arc()),
                    engine.clone(),
                )),
                host,
                operation_log,
                registry,
                status,
                stores,
            }),
        }
    }

    /// Everything the engine reaches its host through.
    pub fn host(&self) -> &EngineHost {
        &self.inner.host
    }

    /// The same host, shared, for work that outlives the call (a listener, a
    /// sweep thread), which holds it weakly where it runs in the background.
    pub(super) fn host_arc(&self) -> &Arc<EngineHost> {
        &self.inner.host
    }

    /// The operation log this engine journals into.
    pub fn operation_log(&self) -> &OperationLog {
        &self.inner.operation_log
    }

    /// Admission, lanes, and the queue's records.
    pub(crate) fn manager(&self) -> &Arc<OperationManager> {
        &self.inner.manager
    }

    /// Every live operation's state, by id.
    pub(super) fn registry(&self) -> &WriteOperationRegistry {
        &self.inner.registry
    }

    /// Every registered operation's status, and the busy-volume set.
    pub(super) fn status(&self) -> &StatusCache {
        &self.inner.status
    }

    /// The stores each operation's scope shares: previews, the in-flight
    /// ledger, outages, and probes.
    pub(super) fn stores(&self) -> &EngineStores {
        &self.inner.stores
    }

    /// A scope for one new operation on this engine: its seams, its status
    /// cache, the engine's stores, and a journal it opens later.
    pub(super) fn op_scope(&self) -> OpScope {
        OpScope::new(
            Arc::clone(&self.inner.host),
            Arc::clone(&self.inner.status),
            Arc::clone(&self.inner.stores),
            self.inner.operation_log.clone(),
        )
    }

    /// A fresh state for one new managed operation on this engine.
    pub(super) fn new_operation_state(&self, progress_interval: Duration) -> WriteOperationState {
        WriteOperationState::scoped(progress_interval, Arc::new(self.op_scope()))
    }

    /// Keeps a failed operation's row readable after its record settles: what
    /// the app's terminal side effects call when an operation's `write-error`
    /// goes out. Not every error is a failure; see
    /// [`OperationManager::retain_failure`].
    pub fn retain_failure(
        &self,
        operation_id: &OperationId,
        operation_type: super::types::WriteOperationType,
        error: &super::types::WriteOperationError,
    ) {
        self.manager().retain_failure(operation_id, operation_type, error);
    }

    /// Gets the detailed status of a specific operation, or `None` once it's
    /// gone (or if it never existed).
    pub fn get_operation_status(&self, operation_id: &OperationId) -> Option<OperationStatus> {
        // Asked BEFORE the cache lock, never inside it: this is the only place the
        // two locks meet, and taking them in one order here means there is no order
        // to get wrong. The busy-volume recompute already walks the other way (cache
        // lock, then out), so nesting would close a cycle through data-safety code.
        let lifecycle = self.manager().lifecycle_status(operation_id);
        let mut snapshot = self.status().snapshot(operation_id.as_str(), lifecycle)?;

        // The counters above are a cached echo of the last progress tick; the WAIT is
        // classified live, off the same `WriteOperationState::activity` the event
        // stream reads. A reader that never caught an event (an agent polling
        // `cmdr://state`, a window opened mid-transfer) still has to tell a slow copy
        // from a wedged mount from a parked conflict prompt, and the cache can't hold
        // that: it would be stale the moment the operation moved on.
        //
        // Outside the cache guard, like `lifecycle` above: the probe registry and the
        // state map are other locks, and this function keeps them all un-nested.
        snapshot.activity = self
            .registry()
            .get(operation_id)
            .and_then(|state| state.activity(operation_id.as_str(), None));
        Some(snapshot)
    }
}
