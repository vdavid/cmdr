//! The master drive-indexing switch and the per-drive intent it overrides.
//!
//! Two switches decide whether a volume indexes, and they compose ONE way:
//!
//! - The **master switch** (`indexing.enabled` in settings) is a hard gate. Off ⇒
//!   nothing indexes, anywhere: no launch auto-start, no per-drive enable, and no
//!   autonomous resume (SMB reconnect, MTP, local external).
//! - The **per-drive intent** (persisted on each volume's own index DB) selects
//!   WHICH drives index while the master is on.
//!
//! Flipping the master off stops every running index through `stop_indexing`,
//! which by design never writes the sticky `user_disabled` marker
//! (`transports/CLAUDE.md`). So per-drive intent survives the master toggle, and
//! flipping the master back on restores exactly the drives the user had chosen
//! rather than turning on everything.
//!
//! The master value lives in a process-wide atomic rather than being re-read from
//! `settings.json` per call: the settings loader is a launch-time reader, and the
//! live-apply rule (`settings/CLAUDE.md`) says every change is pushed via IPC. The
//! atomic is seeded at launch and updated by `set_indexing_enabled`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::indexing::store::IndexStore;

/// The master drive-indexing switch, mirrored from `indexing.enabled`.
///
/// Defaults to `true` so a launch that never seeds it (unit tests, an early start
/// before `setup()` reads settings) behaves like today's default-on setting.
static MASTER_ENABLED: AtomicBool = AtomicBool::new(true);

/// Mirror the user's `indexing.enabled` setting into the process.
///
/// Called at launch (seeded from `settings.json`) and on every live change from
/// the `set_indexing_enabled` command. Starting or stopping the affected volumes
/// is the caller's job; this only moves the gate.
pub fn set_master_enabled(enabled: bool) {
    MASTER_ENABLED.store(enabled, Ordering::Relaxed);
    log::info!(target: "indexing::master", "Drive indexing master switch is now {}", if enabled { "on" } else { "off" });
}

/// Whether the master drive-indexing switch is on. Every start/resume path
/// consults this; `false` means no volume may index.
pub fn master_enabled() -> bool {
    MASTER_ENABLED.load(Ordering::Relaxed)
}

/// Whether a drive should be indexing right now, from the master switch plus the
/// PERSISTED per-drive intent on its own index DB.
///
/// Pure in the master value (passed in, not read) so the composition is testable
/// without touching the process-wide atomic, which would race parallel tests.
///
/// Per-drive intent is two facts on the DB:
/// - `user_disabled` (sticky, written ONLY by the explicit disable command) vetoes
///   unconditionally: a reconnect must never turn back on what the user turned off.
/// - `persisted_scan_completed` is the "the user enabled this drive and it finished
///   at least once" signal. External drives are opt-IN, so they need it; the boot
///   disk (`is_root`) is opt-OUT and indexes by default, so it doesn't.
pub(crate) fn drive_index_should_run(master_on: bool, db_path: &Path, is_root: bool) -> bool {
    if !master_on {
        return false;
    }
    if IndexStore::user_disabled(db_path) {
        return false;
    }
    is_root || IndexStore::persisted_scan_completed(db_path)
}

/// Every drive that should be indexing now that the master switch is back on,
/// and isn't already: the boot disk plus every registered volume whose persisted
/// per-drive intent says yes.
///
/// This is what makes the master switch RESTORE per-drive choices instead of
/// re-enabling everything: a drive the user never turned on (no completed scan on
/// record) or explicitly turned off (`user_disabled`) isn't in the list. The
/// caller routes each id through the normal per-drive enable, so each transport's
/// own gate (the direct-smb2 upgrade, MTP device presence) still applies.
pub(crate) fn drives_to_resume(app: &tauri::AppHandle) -> Vec<String> {
    let mut ids = vec![crate::indexing::ROOT_VOLUME_ID.to_string()];
    ids.extend(
        crate::file_system::get_volume_manager()
            .list_volumes()
            .into_iter()
            .map(|(id, _name)| id)
            .filter(|id| id != crate::indexing::ROOT_VOLUME_ID),
    );
    ids.retain(|id| {
        if super::state::is_active(id) {
            return false;
        }
        match super::state::resolved_index_db_path(app, id) {
            Ok(db_path) => drive_index_should_run(true, &db_path, id == crate::indexing::ROOT_VOLUME_ID),
            Err(e) => {
                log::debug!(target: "indexing::master", "drives_to_resume: can't resolve db path for '{id}': {e}");
                false
            }
        }
    });
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DB that looks exactly like an external drive the user enabled and whose
    /// first scan completed: the state the SMB auto-resume gate acts on.
    fn enabled_external_db(dir: &Path) -> std::path::PathBuf {
        let db_path = dir.join("index-smb-192-168-1-111-445-naspi.db");
        drop(IndexStore::open(&db_path).expect("open store"));
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp scan_completed_at");
        drop(conn);
        db_path
    }

    #[test]
    fn master_off_refuses_a_drive_whose_per_drive_intent_says_yes() {
        // The reported bug: with drive indexing off in settings, a NAS share whose
        // persisted state said "enabled, scan completed" still auto-resumed at every
        // launch, paying the full index cost the user opted out of. The master switch
        // must veto the per-drive intent, not sit beside it.
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = enabled_external_db(dir.path());

        assert!(
            drive_index_should_run(true, &db_path, false),
            "master on + per-drive intent on ⇒ the drive indexes",
        );
        assert!(
            !drive_index_should_run(false, &db_path, false),
            "master off must veto a drive whose per-drive intent says yes",
        );
    }

    #[test]
    fn master_off_refuses_the_boot_disk_too() {
        // The gate is global, not external-only: the boot disk is opt-out (it needs
        // no completed scan on record), but the master switch still vetoes it.
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("index-root.db");

        assert!(drive_index_should_run(true, &db_path, true), "master on ⇒ root indexes");
        assert!(
            !drive_index_should_run(false, &db_path, true),
            "master off ⇒ root doesn't"
        );
    }

    #[test]
    fn master_on_still_honors_the_sticky_user_disabled_marker() {
        // Turning the master back on restores per-drive choices; it must not
        // re-enable a drive the user explicitly turned off.
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = enabled_external_db(dir.path());
        IndexStore::set_user_disabled(&db_path, true).expect("mark user_disabled");

        assert!(
            !drive_index_should_run(true, &db_path, false),
            "a user-disabled drive stays off when the master comes back on",
        );
    }

    #[test]
    fn an_external_drive_that_never_finished_a_scan_is_not_resumed() {
        // Opt-in: a share the user never enabled (no DB) or whose first scan never
        // finished must never be indexed uninvited.
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("index-smb-never-enabled.db");
        assert!(!drive_index_should_run(true, &missing, false), "no DB ⇒ never resumed");

        let started = dir.path().join("index-smb-half-enabled.db");
        drop(IndexStore::open(&started).expect("open store"));
        assert!(
            !drive_index_should_run(true, &started, false),
            "no completed scan ⇒ not resumed",
        );
    }

    #[test]
    fn the_master_atomic_defaults_on() {
        // Default-on matters: a unit test or an early start that never seeds the
        // atomic must behave like today's default-on setting. Deliberately read-only
        // — flipping the process-wide atomic here would race every parallel test
        // that starts an index.
        assert!(master_enabled(), "the master switch defaults on");
    }
}
