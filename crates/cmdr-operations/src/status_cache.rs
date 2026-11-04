//! The operation status cache, and the busy-volume set it drives.
//!
//! One `HashMap` keyed by operation id, holding what a query API needs to
//! answer "how is this operation doing?" without touching the live
//! [`WriteOperationState`](super::state::WriteOperationState). Registering and
//! unregistering an entry is also the ONE lifecycle choke point where an
//! operation's touched volumes become "busy", so the eject guard and the
//! indexing priority gauge can't drift apart from each other.
//!
//! The cache and the busy set are one struct because they are one mechanism:
//! the busy set is derived from the cache, and every membership change comes
//! from a register / unregister here. One per engine ([`StatusCache`]), owned
//! by its `Inner` and shared with each operation's scope.
//!
//! Callers reach all of this through `state::` — see the re-export there.

use cmdr_fs::ignore_poison::{IgnorePoison, RwLockIgnorePoison};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use super::Operations;
use super::host::EngineEvents;
use super::types::{LifecycleStatus, OperationStatus, WriteOperationPhase, WriteOperationType};
#[cfg(target_os = "macos")]
use cmdr_operation_log::types::OperationId;

#[cfg(test)]
#[path = "status_cache_tests.rs"]
mod tests;

/// Internal status tracking for operations.
#[derive(Debug, Clone)]
struct OperationStatusInternal {
    operation_type: WriteOperationType,
    phase: WriteOperationPhase,
    current_file: Option<String>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
    started_at: u64,
    /// Volume IDs this operation reads from or writes to (source and/or
    /// destination). Drives the "busy volumes" set the volume picker reads to
    /// disable Eject while a transfer touches that device. Empty for pure
    /// same-`root` local ops (root is never ejectable). Populated only for
    /// cross-volume copy/move and volume-aware delete, where an ejectable
    /// USB / DMG / SMB / MTP volume can be involved.
    volume_ids: Vec<String>,
}

/// One engine's status cache and the busy-volume set it drives.
pub(crate) struct StatusCache {
    /// Every registered operation's status, by operation id. A value store:
    /// each update replaces one entry's fields, so a poisoned lock is
    /// recovered, never skipped (skipping dropped a busy volume, and Eject then
    /// went ahead mid-transfer).
    entries: RwLock<HashMap<String, OperationStatusInternal>>,
    /// Last busy set reported, so a progress update that doesn't change
    /// membership reports nothing (the cache changes on every progress tick,
    /// but the busy set only changes when an op starts or finishes).
    last_reported_busy: std::sync::Mutex<HashSet<String>>,
    /// Where the transfer gauge pair and the busy-set changes go.
    events: Arc<dyn EngineEvents>,
    /// `volumes-busy-changed` reports that got past the dedupe, ever, counted
    /// before the host hears them, since unit tests have no frontend.
    #[cfg(test)]
    busy_emits: std::sync::atomic::AtomicU64,
}

impl StatusCache {
    /// An empty cache reporting to `events`.
    pub(super) fn new(events: Arc<dyn EngineEvents>) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            last_reported_busy: std::sync::Mutex::new(HashSet::new()),
            events,
            #[cfg(test)]
            busy_emits: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Updates the internal status for an operation.
    #[allow(clippy::too_many_arguments, reason = "the counters of one progress tick")]
    pub(super) fn update(
        &self,
        operation_id: &str,
        phase: WriteOperationPhase,
        current_file: Option<String>,
        files_done: usize,
        files_total: usize,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        if let Some(status) = self.entries.write_ignore_poison().get_mut(operation_id) {
            status.phase = phase;
            status.current_file = current_file;
            status.files_done = files_done;
            status.files_total = files_total;
            status.bytes_done = bytes_done;
            status.bytes_total = bytes_total;
        }
    }

    /// Registers a new operation in the status cache.
    ///
    /// `volume_ids` lists every volume the operation touches (source and/or
    /// destination). Pass an empty `Vec` for pure same-`root` local ops. Any
    /// ejectable volume in the list is marked "busy" until the op unregisters, so
    /// the volume picker can disable Eject for it (see [`Self::busy_volume_ids`]).
    pub(super) fn register(&self, operation_id: &str, operation_type: WriteOperationType, volume_ids: Vec<String>) {
        let now = crate::rt::wall_now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // Report the transfer alongside the eject busy set: register/unregister
        // are the ONE write-op lifecycle choke point, so indexing's "a transfer
        // trumps me" signal (the app feeds its priority gauge from it) and
        // eject's guard can't drift apart. The finish fires from the same
        // panic-safe cleanup paths that unregister.
        self.events.transfer_started(&volume_ids);
        self.entries.write_ignore_poison().insert(
            operation_id.to_string(),
            OperationStatusInternal {
                operation_type,
                phase: WriteOperationPhase::Scanning,
                current_file: None,
                files_done: 0,
                files_total: 0,
                bytes_done: 0,
                bytes_total: 0,
                started_at: now,
                volume_ids,
            },
        );
        self.recompute_and_report_busy_volumes();
    }

    /// Removes an operation from the status cache.
    pub(super) fn unregister(&self, operation_id: &str) {
        let removed_volume_ids = self
            .entries
            .write_ignore_poison()
            .remove(operation_id)
            .map(|status| status.volume_ids);
        // Finish the transfer with the SAME ids the register started it with (they
        // ride the cache entry, so the pair can't drift). A double unregister
        // removed nothing and finishes nothing.
        if let Some(volume_ids) = removed_volume_ids {
            self.events.transfer_finished(&volume_ids);
        }
        self.recompute_and_report_busy_volumes();
    }

    /// Computes the current set of busy volume IDs: the union of every active
    /// operation's touched volumes, minus the default `root` volume (never
    /// ejectable, so marking it busy is pointless noise).
    fn compute_busy_volume_ids(&self) -> HashSet<String> {
        self.entries
            .read_ignore_poison()
            .values()
            .flat_map(|status| status.volume_ids.iter())
            .filter(|id| id.as_str() != cmdr_fs::volume::DEFAULT_VOLUME_ID)
            .cloned()
            .collect()
    }

    /// Returns the busy volume IDs (sorted, for a stable payload / bootstrap).
    /// Used by the `get_busy_volume_ids` bootstrap command, the `eject_volume`
    /// guard, and the native breadcrumb-menu builder.
    pub(super) fn busy_volume_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.compute_busy_volume_ids().into_iter().collect();
        ids.sort();
        ids
    }

    /// Recomputes the busy set and reports it only when its membership changed.
    /// Called from register/unregister (the only two points where membership
    /// can change), so it's panic-safe: unregister runs from the manager's
    /// `ManagedTaskGuard` / external-seam cleanup that fires even on unwind.
    ///
    /// Compute, dedupe, and report run under the dedupe lock, one of the two
    /// engine locks a seam is called under (`host/CLAUDE.md`): the busy set is a state change and the
    /// report is its announcement, so they have to be one step. Outside it, two
    /// racing updates could report an older set last, and the volume picker
    /// would offer Eject on a drive a transfer just started on (the backend's
    /// guard reads [`Self::busy_volume_ids`] live, so it would still refuse).
    /// The host's answer returns at once ([`EngineEvents`]), so the hold is
    /// short.
    fn recompute_and_report_busy_volumes(&self) {
        let mut last = self.last_reported_busy.lock_ignore_poison();
        let current = self.compute_busy_volume_ids();
        if *last == current {
            return;
        }
        let mut volume_ids: Vec<String> = current.iter().cloned().collect();
        volume_ids.sort();
        *last = current;
        #[cfg(test)]
        self.busy_emits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.events.volumes_busy_changed(volume_ids);
    }

    /// `volumes-busy-changed` reports that got past the dedupe so far.
    #[cfg(test)]
    pub(super) fn busy_emit_count(&self) -> u64 {
        self.busy_emits.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// `operation_id`'s cached status, with `lifecycle` as the manager answered
    /// it and no activity (the caller classifies the wait outside this lock).
    pub(super) fn snapshot(&self, operation_id: &str, lifecycle: Option<LifecycleStatus>) -> Option<OperationStatus> {
        let cache = self.entries.read_ignore_poison();
        let status = cache.get(operation_id)?;
        Some(OperationStatus {
            operation_id: operation_id.to_string(),
            operation_type: status.operation_type,
            phase: status.phase,
            lifecycle,
            current_file: status.current_file.clone(),
            files_done: status.files_done,
            files_total: status.files_total,
            bytes_done: status.bytes_done,
            bytes_total: status.bytes_total,
            started_at: status.started_at,
            activity: None,
        })
    }
}

impl Operations {
    // ========================================================================
    // External busy-volume seam (drag-out file promises)
    // ========================================================================
    //
    // The drag-out fulfillment service (`crate::native_drag::fulfillment`) lives
    // outside this module but needs the same eject guard: while it streams bytes
    // off an MTP/SMB device into a Finder-chosen destination, the source volume
    // must register as busy so the user can't eject the phone mid-download (the
    // server-side `eject_volume` guard reads `busy_volume_ids()`). It is NOT a
    // real write operation — no registered state, no progress events, no settle
    // — so it can't go through `start_write_operation`. This thin `pub(crate)`
    // pair is the smallest honest seam: it touches only the status-cache half
    // (which is what the busy set is derived from), keeping the busy set and the
    // `volumes-busy-changed` event firing exactly as a real op would.

    /// Marks `volume_ids` busy for the duration of an external (non-write-op)
    /// operation, keyed by `op_id`. Used by the drag-out fulfillment service to
    /// guard the source volume against eject while a promise is streaming. Pair
    /// every call with [`Self::release_external_volume_op`] (the fulfillment
    /// service uses an RAII guard so release fires on every exit path).
    ///
    /// Registers under `WriteOperationType::Copy` because a drag-out download IS
    /// a copy from the device to local disk — the type only affects diagnostics
    /// (`get_operation_status`), and the busy set itself is type-agnostic.
    ///
    /// macOS-only: the sole caller is `native_drag::fulfillment`, which is
    /// `#[cfg(target_os = "macos")]`. On other targets this would be dead code
    /// under `#![deny(unused)]`.
    #[cfg(target_os = "macos")]
    pub fn register_external_volume_op(&self, op_id: &OperationId, volume_ids: Vec<String>) {
        self.status()
            .register(op_id.as_str(), WriteOperationType::Copy, volume_ids);
    }

    /// Clears the busy mark registered by [`Self::register_external_volume_op`].
    #[cfg(target_os = "macos")]
    pub fn release_external_volume_op(&self, op_id: &OperationId) {
        self.status().unregister(op_id.as_str());
    }

    /// Returns the busy volume IDs (sorted, for a stable payload / bootstrap).
    /// Used by the `get_busy_volume_ids` bootstrap command, the `eject_volume`
    /// guard, and the native breadcrumb-menu builder.
    pub fn busy_volume_ids(&self) -> Vec<String> {
        self.status().busy_volume_ids()
    }
}
