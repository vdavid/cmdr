//! Read-only questions about the registry: which volumes are in it, what kind
//! each one is, and whether one is active, failed, or still waiting for its first
//! walk.
//!
//! Nothing here transitions anything. Each call takes the registry lock, answers
//! from what it finds, and lets it go.

use std::sync::atomic::Ordering;

use super::{INDEX_REGISTRY, IndexPhase};
use crate::indexing::lifecycle::freshness::Freshness;
use crate::indexing::store::{IndexFailure, IndexStore};
use crate::indexing::volume::{IndexVolumeKind, VolumeId};

/// Snapshot the ready-to-score volume ids WITH their typed kind. The importance and
/// media-index schedulers' startup sweeps use this to branch typed on the kind (score
/// Local + SMB, exclude MTP) without re-deriving the kind from the volume-id
/// string. Readiness filter: a registered instance whose
/// freshness is `Fresh` (an authoritative completed scan). `Scanning`/`Stale` volumes
/// are excluded (a `Scanning` one fires `ScanCompleted` on the bus when it finishes; a
/// `Stale` one has nothing to score yet).
///
/// A volume that loaded `Fresh` at launch from its persisted `scan_completed_at` never
/// re-fires a `ScanCompleted`, so a scheduler that only waited on the bus would never
/// act on it (its retained bus value stays `Pending`) — the common restart case. This
/// snapshot is how the sweeps find those volumes; wiring their subscriptions is NOT
/// enough on its own, so each scheduler pairs this with an explicit startup enqueue
/// (media's `kick_all_ready_passes`, importance's `enqueue_initial_full_pass_if_unscored`).
/// ⚠️ A volume being covered in PHASES is admitted on its home-coverage marker
/// too, and is deliberately NOT `Fresh` while it is: the drive genuinely isn't
/// covered yet. Without this the early media kick works on the first run and never
/// again — a relaunch mid-coverage would wire nothing for a home-covered volume,
/// and the signal it publishes would have no subscriber.
pub(crate) fn ready_volumes_with_kind() -> Vec<(VolumeId, IndexVolumeKind)> {
    // Snapshot under the lock, decide off it: reading a marker is SQLite work, and
    // nothing that touches a database belongs under the lifecycle lock.
    let candidates: Vec<(VolumeId, IndexVolumeKind, bool)> = {
        let reg = INDEX_REGISTRY.lock().expect("INDEX_REGISTRY lock poisoned");
        reg.iter()
            .map(|(vid, instance)| {
                let fresh = instance
                    .signals
                    .freshness
                    .lock()
                    .ok()
                    .and_then(|f| *f)
                    .is_some_and(|f| f == Freshness::Fresh);
                (vid.clone(), instance.kind, fresh)
            })
            .collect()
    };
    candidates
        .into_iter()
        .filter(|(vid, _, fresh)| *fresh || has_covered_home(vid))
        .map(|(vid, kind, _)| (vid, kind))
        .collect()
}

/// Whether a volume has covered the user's home folder, whatever the rest of the
/// drive is doing.
fn has_covered_home(volume_id: &str) -> bool {
    let Ok(db_path) = super::resolved_index_db_path(volume_id) else {
        return false;
    };
    IndexStore::open_read_connection(&db_path)
        .and_then(|conn| IndexStore::get_meta(&conn, crate::indexing::lifecycle::phases::HOME_COVERED_AT_KEY))
        .unwrap_or(None)
        .is_some()
}

/// Snapshot every registered volume id. Used by the global memory watchdog to
/// stop EVERY volume's index (not just `root`) when the global budget is hit.
pub(crate) fn all_registered_volume_ids() -> Vec<VolumeId> {
    INDEX_REGISTRY
        .lock()
        .map(|reg| reg.keys().cloned().collect())
        .unwrap_or_default()
}

/// The typed kind of a registered volume, or `None` if it has no index instance.
///
/// Lets a consumer (the `record_visit` command) branch on the kind — record a
/// visit for a Local/SMB volume, skip an MTP one — without inspecting the
/// volume-id string.
pub(crate) fn volume_kind(volume_id: &str) -> Option<IndexVolumeKind> {
    INDEX_REGISTRY.lock().ok()?.get(volume_id).map(|i| i.kind)
}

/// All registered MTP volume ids belonging to `device_id` (one device hosts N
/// storages, each a separate index). Used by the disconnect hook to flip every
/// one of the device's indexes to Stale.
///
/// Matches by the volume id's device-id half (robust `rsplit` via
/// `cmdr_fs::volume::mtp_ids`, so a `:` in a serial device id doesn't mis-key), NOT a raw
/// prefix — `mtp-AA` must not match `mtp-AAB:1`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn registered_mtp_volume_ids_for_device(device_id: &str) -> Vec<String> {
    let reg = INDEX_REGISTRY.lock().expect("INDEX_REGISTRY lock poisoned");
    reg.keys()
        .filter(|vid| cmdr_fs::volume::mtp_ids::device_id_of_volume(vid) == Some(device_id))
        .cloned()
        .collect()
}

/// Whether a volume has an index but has never actually been walked: nothing
/// scanning now, and no completed scan on disk.
///
/// Two things leave that shape. A search-driven coverage walk stands a writer up
/// and nothing else ([`Activation::WriterOnly`](super::Activation::WriterOnly)), and a first scan someone
/// stopped leaves its manager behind. In both cases the volume reads as active
/// while no walk has ever covered it, so an enable that short-circuits on
/// activity alone would swallow the very request that asks for one.
pub(crate) fn awaits_its_first_scan(volume_id: &str) -> bool {
    let db_path = {
        let Ok(reg) = INDEX_REGISTRY.lock() else { return false };
        match reg.get(volume_id).map(|i| &i.phase) {
            // `Initializing` is a start already in flight, and a scanning manager
            // is a walk already running: neither needs another. A volume the phase
            // machine still has work for is the third shape of the same thing — it
            // is being walked whole, in pieces.
            Some(IndexPhase::Running(mgr)) if !mgr.scanning.load(Ordering::Relaxed) && !mgr.phases_have_work() => {
                mgr.db_path().to_path_buf()
            }
            _ => return false,
        }
    };
    // Off the lock: reading meta is SQLite work, and nothing that touches a
    // database belongs under the lifecycle lock. The key is the one a completed
    // scan writes and a scan start clears (`manager/start.rs`).
    let completed = IndexStore::open_read_connection(&db_path)
        .and_then(|conn| IndexStore::get_meta(&conn, "scan_completed_at"))
        .unwrap_or(None);
    completed.is_none()
}

/// Whether anything is watching this volume's filesystem right now.
///
/// The one observable difference between "covered and kept current" and "covered
/// but unwatched", which the branch set deliberately no longer says.
#[cfg(test)]
pub(crate) fn is_watching_for_test(volume_id: &str) -> bool {
    use cmdr_fs::ignore_poison::IgnorePoison;
    match INDEX_REGISTRY.lock_ignore_poison().get(volume_id).map(|i| &i.phase) {
        Some(IndexPhase::Running(mgr)) => mgr.is_watching(),
        _ => false,
    }
}

/// Check whether a volume's index is active (initializing, running, or momentarily
/// detached for a scan start).
///
/// ⚠️ `Detached` counts, and it has to: nine callers ask this, including the drive
/// badge, and a volume that reads inactive for the length of a rescan renders
/// `enabled: false` next to a live freshness color — a shape the badge's own doc
/// comment says can't occur.
pub fn is_active(volume_id: &str) -> bool {
    INDEX_REGISTRY
        .lock()
        .map(|reg| {
            matches!(
                reg.get(volume_id).map(|i| &i.phase),
                Some(IndexPhase::Initializing { .. } | IndexPhase::Running(_) | IndexPhase::Detached { .. })
            )
        })
        .unwrap_or(false)
}

/// Whether a volume's index is in the `Failed` phase (its DB died with a fatal
/// storage error). Distinct from disabled/absent: a failed volume is still
/// registered so the badge is honest. Used by the recovery commands to rebuild
/// from scratch instead of a no-op resume.
pub fn is_failed(volume_id: &str) -> bool {
    INDEX_REGISTRY
        .lock()
        .map(|reg| matches!(reg.get(volume_id).map(|i| &i.phase), Some(IndexPhase::Failed { .. })))
        .unwrap_or(false)
}

/// The typed fatal-storage reason if the volume is in the `Failed` phase, else
/// `None`. Surfaced on `VolumeIndexStatus` so logs and any detailed tooltip can be
/// specific.
pub(crate) fn index_failure(volume_id: &str) -> Option<IndexFailure> {
    match INDEX_REGISTRY.lock().ok()?.get(volume_id).map(|i| &i.phase) {
        Some(IndexPhase::Failed { reason, .. }) => Some(*reason),
        _ => None,
    }
}
