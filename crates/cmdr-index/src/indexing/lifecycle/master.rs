//! The master drive-indexing switch and the per-drive intent it overrides.
//!
//! Two switches decide whether a volume indexes, and they compose ONE way:
//!
//! - The **master switch** (`indexing.enabled` in settings) is a hard gate. Off ⇒
//!   nothing indexes on its own, anywhere: no launch auto-start, no per-drive
//!   enable, and no autonomous resume (SMB reconnect, MTP, local external).
//! - The **per-drive intent** (persisted on each volume's own index DB) selects
//!   WHICH drives index while the master is on.
//!
//! **Both govern BACKGROUND work only, never a user-initiated read.** A
//! search-driven coverage walk (`Activation::WriterOnly`, `lifecycle/cover/`)
//! runs with either switch off, because searching a folder Cmdr hasn't indexed IS
//! reading it, and the person asking is right there. Refusing wouldn't save them
//! work; it would hand them a search that silently omits files. What the switches
//! keep is everything that runs uninvited: no scan is scheduled, and no watcher
//! is started uninvited. A vetoed drive gets none at all, which is the veto's
//! real teeth: what a search walked there stays covered and served, but stops
//! being kept current the moment the app does. See [`branch_watch_allowed`],
//! which is where a walked branch's watcher asks these two switches (and only
//! these two).
//!
//! Flipping the master off stops every running index through `stop_indexing`,
//! which by design never writes per-drive intent (`transports/CLAUDE.md`). So the
//! choice survives the master toggle, and flipping the master back on restores
//! exactly the drives the user had chosen rather than turning on everything —
//! including a drive whose first index the toggle interrupted, because intent is
//! recorded when the user asks rather than when a scan finishes.
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

/// Whether the master drive-indexing switch is on. Every BACKGROUND start/resume
/// path consults this; `false` means no volume may index on its own.
///
/// ❌ Not a gate on a user-initiated read. A search-driven coverage walk
/// (`Activation::WriterOnly`) runs whatever this says: the switch governs work
/// the app does uninvited, and someone searching a folder asked for exactly the
/// reading the walk does. See the module doc.
pub fn master_enabled() -> bool {
    MASTER_ENABLED.load(Ordering::Relaxed)
}

/// Set the switch for one test and put it back on drop, so a test that needs it
/// off doesn't leak that into whichever test runs next in the same binary.
///
/// Process-wide, like every other seam a test handle installs: hold
/// `handle::test_lock()` first.
#[cfg(any(test, feature = "testing"))]
#[must_use = "the switch is restored when the guard drops"]
pub(crate) fn install_for_test(enabled: bool) -> MasterSwitchGuard {
    let previous = MASTER_ENABLED.swap(enabled, Ordering::Relaxed);
    MasterSwitchGuard { previous }
}

/// Restores the master switch on drop.
#[cfg(any(test, feature = "testing"))]
pub(crate) struct MasterSwitchGuard {
    previous: bool,
}

#[cfg(any(test, feature = "testing"))]
impl Drop for MasterSwitchGuard {
    fn drop(&mut self) {
        MASTER_ENABLED.store(self.previous, Ordering::Relaxed);
    }
}

/// Whether a drive should be indexing right now, from the master switch plus the
/// PERSISTED per-drive intent on its own index DB.
///
/// Pure in the master value (passed in, not read) so the composition is testable
/// without touching the process-wide atomic, which would race parallel tests.
///
/// Per-drive intent is a pair of sticky markers on the DB, written together
/// (`IndexStore::set_drive_index_intent`) so at most one ever holds:
/// - `user_disabled` (written ONLY by the explicit disable command) vetoes
///   unconditionally: a reconnect must never turn back on what the user turned off.
/// - `user_enabled` (written by `Index::start_volume`, before it starts anything)
///   is the drive's opt-IN. External drives need it; the boot disk (`is_root`) is
///   opt-OUT and indexes by default, so it doesn't.
///
/// ⚠️ `persisted_scan_completed` is the THIRD arm and it is not interchangeable
/// with the second: it means "a scan finished here once", which is absent all
/// through a first index and again all through every rescan (`start_scan` deletes
/// the marker before it walks). Reading intent off it alone forgot a drive in
/// exactly those windows. It stays because every index enabled before the marker
/// existed carries only this fact; ❌ don't drop it, and ❌ don't reach for it as
/// the enable.
pub(crate) fn drive_index_should_run(master_on: bool, db_path: &Path, is_root: bool) -> bool {
    if !master_on {
        return false;
    }
    if IndexStore::user_disabled(db_path) {
        return false;
    }
    is_root || IndexStore::user_enabled(db_path) || IndexStore::persisted_scan_completed(db_path)
}

/// Whether a drive may be WALKED in the background right now: asked per phase and
/// per frontier root, so turning drive indexing off stops the walking rather than
/// only the next launch.
///
/// ❌ Not `drive_index_should_run`, whose `persisted_scan_completed` arm means "a
/// scan finished on this drive once". A volume being covered has by definition
/// never finished one, so asking that would mean an external drive somebody just
/// turned on never gets its first index. The per-drive opt-IN was answered by
/// whoever started the volume; what is left to ask is the master switch and the
/// sticky veto.
///
/// It asks the same two facts [`branch_watch_allowed`] does, and stays a separate
/// question because walking and watching cost wildly different things: a per-drive
/// knob that ever splits them splits them here.
pub(crate) fn background_walk_allowed(master_on: bool, db_path: &Path) -> bool {
    master_on && !IndexStore::user_disabled(db_path)
}

/// Whether a drive may WATCH the branches a search walk covered on it.
///
/// The looser of the two gates, and deliberately so. Watching a walked branch is
/// what keeps the promise the walk made — that ground is as live as an indexed
/// drive's — and it costs one stream and no walking. So it asks only what the two
/// switches say, and NOT `persisted_scan_completed`: that fact means "the user
/// turned this drive on for background indexing", which is exactly what someone
/// searching an unindexed drive did not do and does not need to have done.
///
/// The veto's teeth are here. A drive the user explicitly turned off gets no
/// watcher, so what a search walked there is a snapshot: still covered, still
/// served, but stale from the moment the app stops (Decision 5's covered-but-
/// stale, which the read side renders as stale rather than current).
pub(crate) fn branch_watch_allowed(master_on: bool, db_path: &Path) -> bool {
    master_on && !IndexStore::user_disabled(db_path)
}

/// Every drive that should be indexing now that the master switch is back on,
/// and isn't already: the boot disk plus every registered volume whose persisted
/// per-drive intent says yes.
///
/// This is what makes the master switch RESTORE per-drive choices instead of
/// re-enabling everything: a drive the user never turned on, or explicitly turned
/// off, isn't in the list, and one they turned on is — however far its first index
/// got before the switch went off. The
/// caller routes each id through the normal per-drive enable, so each transport's
/// own gate (the direct-smb2 upgrade, MTP device presence) still applies.
pub(crate) fn drives_to_resume() -> Vec<String> {
    let mut ids = vec![crate::ROOT_VOLUME_ID.to_string()];
    ids.extend(
        crate::indexing::host::volumes::current()
            .volume_ids()
            .into_iter()
            .filter(|id| id != crate::ROOT_VOLUME_ID),
    );
    ids.retain(|id| {
        if super::state::is_active(id) {
            return false;
        }
        match super::state::resolved_index_db_path(id) {
            Ok(db_path) => drive_index_should_run(true, &db_path, id == crate::ROOT_VOLUME_ID),
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
        let db_path = enabled_but_never_finished_db(dir);
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp scan_completed_at");
        drop(conn);
        db_path
    }

    /// The same drive with no completed scan on record: either its first index is
    /// still running (or was interrupted), or a rescan is in flight right now —
    /// `start_scan` deletes `scan_completed_at` before it walks, so a drive that
    /// finished yesterday looks like this for the whole of today's rescan.
    fn enabled_but_never_finished_db(dir: &Path) -> std::path::PathBuf {
        let db_path = dir.join("index-smb-192-168-1-111-445-naspi.db");
        drop(IndexStore::open(&db_path).expect("open store"));
        IndexStore::set_drive_index_intent(&db_path, true).expect("record the enable");
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
        IndexStore::set_drive_index_intent(&db_path, false).expect("record the disable");

        assert!(
            !drive_index_should_run(true, &db_path, false),
            "a user-disabled drive stays off when the master comes back on",
        );
    }

    #[test]
    fn an_external_drive_nobody_turned_on_is_not_resumed() {
        // Opt-in, and this half of it is the guarantee the veto rests on: a share the
        // user never enabled — no DB at all, or a DB a search walk left behind with
        // no enable on it — must never be indexed uninvited.
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("index-smb-never-enabled.db");
        assert!(!drive_index_should_run(true, &missing, false), "no DB ⇒ never resumed");

        let walked = dir.path().join("index-smb-only-searched.db");
        drop(IndexStore::open(&walked).expect("open store"));
        assert!(
            !drive_index_should_run(true, &walked, false),
            "an index with neither an enable nor a completed scan is nobody's request",
        );
    }

    #[test]
    fn an_external_drive_whose_first_index_was_interrupted_is_resumed() {
        // The other half, and the bug this marker fixes: the user turned this drive
        // on, and the first index never got to finish (they quit, the NAS dropped, the
        // master switch went off and came back). Reading intent off `scan_completed_at`
        // forgot the drive in exactly that window — the one where the user is most
        // likely to be waiting for the index they asked for.
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = enabled_but_never_finished_db(dir.path());

        assert!(
            !IndexStore::persisted_scan_completed(&db_path),
            "precondition: nothing has completed a scan on this drive",
        );
        assert!(
            drive_index_should_run(true, &db_path, false),
            "an enabled drive resumes whether or not a scan ever finished on it",
        );
    }

    #[test]
    fn a_drive_whose_rescan_is_in_flight_is_still_resumed() {
        // Same absent marker, a different story, and the reason completion could never
        // carry intent: `start_scan` DELETES `scan_completed_at` before it walks. So a
        // drive that completed yesterday reads as never-completed for the whole of
        // today's rescan, and anything asking mid-rescan (a master toggle, a reconnect)
        // would drop it.
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = enabled_external_db(dir.path());
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        conn.execute("DELETE FROM meta WHERE key = 'scan_completed_at'", [])
            .expect("clear the completion marker, as a scan start does");
        drop(conn);

        assert!(
            drive_index_should_run(true, &db_path, false),
            "a rescan in flight must not read as a drive nobody turned on",
        );
    }

    #[test]
    fn turning_a_drive_off_withdraws_the_enable() {
        // The two markers are one fact. A disable that only wrote the veto would leave
        // the enable behind it, and any later re-read of intent would find both.
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = enabled_but_never_finished_db(dir.path());

        IndexStore::set_drive_index_intent(&db_path, false).expect("record the disable");
        assert!(
            !drive_index_should_run(true, &db_path, false),
            "a drive the user turned off stays off",
        );

        IndexStore::set_drive_index_intent(&db_path, true).expect("record the re-enable");
        assert!(
            drive_index_should_run(true, &db_path, false),
            "and turning it back on works, without waiting for a scan to finish",
        );
    }

    #[test]
    fn an_index_that_predates_the_enable_marker_still_resumes() {
        // Every drive enabled before intent was recorded carries only its completed
        // scan. ❌ Don't drop the completion arm: it's what stops this change from
        // silently un-enabling every drive already in the field.
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("index-smb-shipped-before.db");
        drop(IndexStore::open(&db_path).expect("open store"));
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp scan_completed_at");
        drop(conn);

        assert!(
            !IndexStore::user_enabled(&db_path),
            "precondition: an index from before the marker existed",
        );
        assert!(
            drive_index_should_run(true, &db_path, false),
            "a completed scan still means the user had this drive on",
        );
    }

    #[test]
    fn the_master_atomic_defaults_on() {
        // Default-on matters: a unit test or an early start that never seeds the
        // atomic must behave like today's default-on setting. Deliberately
        // read-only — flipping the process-wide atomic here would race every
        // parallel test that starts an index. Even READING it needs the seam lock,
        // because a test that legitimately turns the switch off (the search walk's
        // carve-out) holds that lock while it does.
        let _serialized = crate::indexing::handle::test_lock();
        assert!(master_enabled(), "the master switch defaults on");
    }
}
