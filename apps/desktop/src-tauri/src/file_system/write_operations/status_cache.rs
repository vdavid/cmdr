//! The operation status cache, and the busy-volume set it drives.
//!
//! One `HashMap` keyed by operation id, holding what a query API needs to
//! answer "how is this operation doing?" without touching the live
//! [`WriteOperationState`](super::state::WriteOperationState). Registering and
//! unregistering an entry is also the ONE lifecycle choke point where an
//! operation's touched volumes become "busy", so the eject guard and the
//! indexing priority gauge can't drift apart from each other.
//!
//! The cache and the busy set are one module because they are one mechanism:
//! `recompute_and_emit_busy_volumes` derives the busy set by reading this
//! module's own `OPERATION_STATUS_CACHE`, and every membership change comes
//! from a register / unregister here. Splitting them would put a private
//! static on the wrong side of a module boundary.
//!
//! Callers reach all of this through `state::` — see the re-export there.

use crate::ignore_poison::IgnorePoison;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, OnceLock, RwLock};

use super::state::WRITE_OPERATION_STATE;
use super::types::{OperationStatus, OperationSummary, WriteOperationPhase, WriteOperationType};

#[cfg(test)]
#[path = "status_cache_tests.rs"]
mod tests;

/// Global cache for operation status (for query APIs).
static OPERATION_STATUS_CACHE: LazyLock<RwLock<HashMap<String, OperationStatusInternal>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

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

// ============================================================================
// Status cache management
// ============================================================================

/// Updates the internal status for an operation.
pub(super) fn update_operation_status(
    operation_id: &str,
    phase: WriteOperationPhase,
    current_file: Option<String>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
) {
    if let Ok(mut cache) = OPERATION_STATUS_CACHE.write()
        && let Some(status) = cache.get_mut(operation_id)
    {
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
/// the volume picker can disable Eject for it (see `busy_volume_ids`).
pub(super) fn register_operation_status(
    operation_id: &str,
    operation_type: WriteOperationType,
    volume_ids: Vec<String>,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Raise the priority transfer gauge alongside the eject busy set: these two
    // register/unregister functions are the ONE write-op lifecycle choke point, so
    // indexing's "a transfer trumps me" signal (`crate::priority::transfers`) and
    // eject's guard can't drift apart. The finish fires from the same panic-safe
    // cleanup paths that unregister.
    crate::priority::transfers::note_transfer_started(&volume_ids);
    if let Ok(mut cache) = OPERATION_STATUS_CACHE.write() {
        cache.insert(
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
    }
    recompute_and_emit_busy_volumes();
}

/// Removes an operation from the status cache.
pub(super) fn unregister_operation_status(operation_id: &str) {
    let removed_volume_ids = if let Ok(mut cache) = OPERATION_STATUS_CACHE.write() {
        cache.remove(operation_id).map(|status| status.volume_ids)
    } else {
        None
    };
    // Lower the priority transfer gauge with the SAME ids the register raised it
    // with (they ride the cache entry, so the pair can't drift). A double
    // unregister removed nothing and lowers nothing.
    if let Some(volume_ids) = removed_volume_ids {
        crate::priority::transfers::note_transfer_finished(&volume_ids);
    }
    recompute_and_emit_busy_volumes();
}

// ============================================================================
// External busy-volume seam (drag-out file promises)
// ============================================================================
//
// `register_operation_status` / `unregister_operation_status` are `pub(super)`,
// reachable only inside `write_operations`. The drag-out fulfillment service
// (`crate::native_drag::fulfillment`) lives outside this module but needs the
// same eject guard: while it streams bytes off an MTP/SMB device into a
// Finder-chosen destination, the source volume must register as busy so the
// user can't eject the phone mid-download (the server-side `eject_volume` guard
// reads `busy_volume_ids()`). It is NOT a real write operation — no
// `WRITE_OPERATION_STATE` entry, no progress events, no settle — so it can't go
// through `start_write_operation`. This thin `pub(crate)` pair is the smallest
// honest seam: it touches only the `OPERATION_STATUS_CACHE` half (which is what
// `recompute_and_emit_busy_volumes` reads), keeping the busy set and the
// `volumes-busy-changed` event firing exactly as a real op would.

/// Marks `volume_ids` busy for the duration of an external (non-write-op)
/// operation, keyed by `op_id`. Used by the drag-out fulfillment service to
/// guard the source volume against eject while a promise is streaming. Pair
/// every call with [`release_external_volume_op`] (the fulfillment service uses
/// an RAII guard so release fires on every exit path).
///
/// Registers under `WriteOperationType::Copy` because a drag-out download IS a
/// copy from the device to local disk — the type only affects diagnostics
/// (`list_active_operations`), and the busy set itself is type-agnostic.
///
/// macOS-only: the sole caller is `native_drag::fulfillment`, which is
/// `#[cfg(target_os = "macos")]`. On other targets this would be dead code under
/// `#![deny(unused)]`.
#[cfg(target_os = "macos")]
pub(crate) fn register_external_volume_op(op_id: &str, volume_ids: Vec<String>) {
    register_operation_status(op_id, WriteOperationType::Copy, volume_ids);
}

/// Clears the busy mark registered by [`register_external_volume_op`].
#[cfg(target_os = "macos")]
pub(crate) fn release_external_volume_op(op_id: &str) {
    unregister_operation_status(op_id);
}

// ============================================================================
// Busy-volumes set (drives "disable Eject while an op touches this device")
// ============================================================================

/// Typed `volumes-busy-changed` Tauri event. Wraps the busy volume-ID list in a
/// struct because `tauri_specta::Event` payloads must be named types (a bare
/// `Vec<String>` can't derive `Event`). The struct name kebab-cases to
/// `volumes-busy-changed`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumesBusyChanged {
    /// IDs of volumes with an in-flight copy / move / delete operation (sorted).
    pub volume_ids: Vec<String>,
}

/// App handle for emitting `volumes-busy-changed`. Set once at startup via
/// `init_busy_volume_emitter`. Absent in unit tests, where the recompute is a
/// no-op emit (the set is still queryable via `busy_volume_ids`).
static BUSY_APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Last busy set we emitted, so a progress update that doesn't change
/// membership produces no event (the cache changes on every progress tick, but
/// the busy set only changes when an op starts or finishes).
static LAST_EMITTED_BUSY: LazyLock<std::sync::Mutex<HashSet<String>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashSet::new()));

/// Stores the app handle used to broadcast `volumes-busy-changed`. Call once at
/// app setup, before any write op can run.
pub fn init_busy_volume_emitter(app: &tauri::AppHandle) {
    let _ = BUSY_APP.set(app.clone());
}

/// Computes the current set of busy volume IDs: the union of every active
/// operation's touched volumes, minus the default `root` volume (never
/// ejectable, so marking it busy is pointless noise).
fn compute_busy_volume_ids() -> HashSet<String> {
    let Ok(cache) = OPERATION_STATUS_CACHE.read() else {
        return HashSet::new();
    };
    cache
        .values()
        .flat_map(|status| status.volume_ids.iter())
        .filter(|id| id.as_str() != crate::file_system::volume::DEFAULT_VOLUME_ID)
        .cloned()
        .collect()
}

/// Returns the busy volume IDs (sorted, for a stable payload / bootstrap).
/// Used by the `get_busy_volume_ids` bootstrap command, the `eject_volume`
/// guard, and the native breadcrumb-menu builder.
pub fn busy_volume_ids() -> Vec<String> {
    let mut ids: Vec<String> = compute_busy_volume_ids().into_iter().collect();
    ids.sort();
    ids
}

/// Recomputes the busy set and emits `volumes-busy-changed` only when its
/// membership changed. Called from register/unregister (the only two points
/// where membership can change), so it's panic-safe: unregister runs from the
/// manager's `ManagedTaskGuard` / external-seam cleanup that fires even on unwind.
fn recompute_and_emit_busy_volumes() {
    let current = compute_busy_volume_ids();

    {
        let mut last = LAST_EMITTED_BUSY.lock_ignore_poison();
        if *last == current {
            return;
        }
        *last = current.clone();
    }

    let Some(app) = BUSY_APP.get() else {
        return;
    };
    let mut volume_ids: Vec<String> = current.into_iter().collect();
    volume_ids.sort();
    use tauri_specta::Event as _;
    let payload = VolumesBusyChanged { volume_ids };
    if let Err(e) = payload.emit(app) {
        crate::log_error!(target: "eject", "Failed to emit volumes-busy-changed: {}", e);
    }
}

// Panic-safe cache cleanup for managed write ops now lives in
// `manager::ManagedTaskGuard` (it also frees the op's lane slots). The old
// standalone `OperationStateGuard` was retired when every spawn path moved
// behind the operation manager.

// ============================================================================
// Public query functions
// ============================================================================

/// Lists all active write operations.
///
/// Returns a list of operation summaries for all currently running operations.
/// This is useful for showing a global progress view or managing multiple concurrent operations.
pub fn list_active_operations() -> Vec<OperationSummary> {
    let cache = match OPERATION_STATUS_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cache
        .iter()
        .map(|(id, status)| {
            let percent_complete = if status.bytes_total > 0 {
                ((status.bytes_done as f64 / status.bytes_total as f64) * 100.0).min(100.0) as u8
            } else if status.files_total > 0 {
                ((status.files_done as f64 / status.files_total as f64) * 100.0).min(100.0) as u8
            } else {
                0
            };

            OperationSummary {
                operation_id: id.clone(),
                operation_type: status.operation_type,
                phase: status.phase,
                percent_complete,
                started_at: status.started_at,
            }
        })
        .collect()
}

/// Gets the detailed status of a specific operation.
///
/// Returns `None` if the operation is not found (either never existed or already completed).
pub fn get_operation_status(operation_id: &str) -> Option<OperationStatus> {
    let cache = OPERATION_STATUS_CACHE.read().ok()?;
    let status = cache.get(operation_id)?;

    // Check if the operation is still running
    let is_running = WRITE_OPERATION_STATE.contains(operation_id);

    Some(OperationStatus {
        operation_id: operation_id.to_string(),
        operation_type: status.operation_type,
        phase: status.phase,
        is_running,
        current_file: status.current_file.clone(),
        files_done: status.files_done,
        files_total: status.files_total,
        bytes_done: status.bytes_done,
        bytes_total: status.bytes_total,
        started_at: status.started_at,
    })
}
