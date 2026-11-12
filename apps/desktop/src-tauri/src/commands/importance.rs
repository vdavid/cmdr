//! The `record_visit` IPC command: the navigation-visit signal feeder.
//!
//! The frontend's navigation-commit point calls this fire-and-forget when a pane
//! settles on a folder. It persists a compact per-volume visit count + last-visit
//! timestamp in `importance.db` (counts and timestamps only — no content,
//! local-only; privacy posture in `docs/security.md`). The scorer's visit-activity
//! signal reads this on the next recompute.
//!
//! Thin per the commands-layer rule: resolve the shared writer, hand off the
//! visit. Failure-silent by contract — a visit that can't be recorded must never
//! break or block navigation, so the command returns `Ok(())` even on a write
//! hiccup (it logs at debug). Recorded for any background-scored volume
//! (Local/SMB), skipped for MTP (on-demand only, never scored — a typed check on
//! the registered kind, not the volume-id string).
//!
//! The pane hands over an APP path; the store keys visits by the path the
//! recompute looks them up by, its volume-relative index path. So the visit is
//! translated first ([`store_path_of_visit`], through `crate::location::store_path`
//! and the one root rule): `/Volumes/testshare-1/documents` is stored as `/documents`, and a
//! cloud drive's folder relative to the drive's own root.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::location::Location;
use crate::location::resolve::{RootSources, volume_root};
use crate::location::store_path::to_store_path;

use cmdr_index::importance::is_background_scored;
use cmdr_index::importance::scheduler::ImportanceScheduler;

/// Record that the user navigated into `location`. Fire-and-forget and
/// failure-silent: never blocks or breaks navigation.
///
/// Recorded for any background-scored volume — Local and SMB. A volume
/// that isn't registered, or is MTP (on-demand only, never scored), is silently
/// ignored: recording a visit no recompute ever reads is dead weight, so the gate
/// is the volume's TYPED kind, never its id string. The
/// write goes through the scheduler's SHARED long-lived writer for the volume (one
/// writer thread per DB — the subsystem invariant held in spirit, not absorbed by
/// WAL busy-timeouts), reached through Tauri managed state. If the scheduler isn't
/// managed yet (startup raced ahead of `start`), the visit is silently dropped —
/// the next navigation records it.
#[tauri::command]
#[specta::specta]
pub async fn record_visit(app: AppHandle, location: Location) -> Result<(), String> {
    // Gate on the registered volume's typed kind: record for a background-scored
    // volume (Local/SMB), skip an unregistered or on-demand-only (MTP) one.
    match crate::index_host::index().volume_kind(&location.volume_id) {
        Some(kind) if is_background_scored(kind) => {}
        _ => return Ok(()),
    }

    let Some(scheduler) = app.try_state::<Arc<ImportanceScheduler>>().map(|s| Arc::clone(&s)) else {
        log::debug!(target: "importance", "record_visit skipped (scheduler not managed yet)");
        return Ok(());
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Do the enqueue off the IPC thread (the shared writer's channel send is quick
    // but resolving/creating the writer can open a DB). A failure is swallowed to
    // `Ok(())`: the visit signal is best-effort, navigation never depends on it.
    let live_root = crate::file_system::volume::manager::get_volume_manager()
        .get(&location.volume_id)
        .map(|volume| volume.root().to_string_lossy().into_owned());
    let Some(path) = store_path_of_visit(&location, live_root.as_deref()) else {
        log::debug!(target: "importance", "record_visit skipped (path not on its volume's root)");
        return Ok(());
    };
    let volume_id = location.volume_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let writer = scheduler.writer_for(&volume_id)?;
        writer.record_visit(&path, now)?;
        Ok::<(), cmdr_index::importance::ImportanceStoreError>(())
    })
    .await;

    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::debug!(target: "importance", "record_visit write failed: {e}"),
        Err(e) => log::debug!(target: "importance", "record_visit task panicked: {e}"),
    }
    Ok(())
}

/// The visit's path in its volume's importance store, the path space the recompute
/// looks visits up in. `None` for a volume that isn't registered, or a path off it.
fn store_path_of_visit(location: &Location, live_root: Option<&str>) -> Option<String> {
    let root = volume_root(RootSources {
        live: live_root,
        ..RootSources::default()
    })?;
    to_store_path(&location.path, &root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(volume_id: &str, path: &str) -> Location {
        Location {
            volume_id: volume_id.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn a_share_visit_is_stored_relative_to_the_share() {
        let visit = at("smb-192-168-50-10-445-testshare", "/Volumes/testshare-1/documents/x");
        assert_eq!(
            store_path_of_visit(&visit, Some("/Volumes/testshare-1")).as_deref(),
            Some("/documents/x")
        );
    }

    /// The drive's store is relative to the DRIVE, though `root`'s index walks it.
    #[test]
    fn a_cloud_drive_visit_is_stored_relative_to_the_drive() {
        let drive = "/Users/test/Library/CloudStorage/GoogleDrive-test@example.com";
        let visit = at("cloud-google-drive", &format!("{drive}/Apps"));
        assert_eq!(store_path_of_visit(&visit, Some(drive)).as_deref(), Some("/Apps"));
    }

    #[test]
    fn a_boot_disk_visit_keeps_its_absolute_path() {
        let visit = at("root", "/Users/test/Projects");
        assert_eq!(
            store_path_of_visit(&visit, Some("/")).as_deref(),
            Some("/Users/test/Projects")
        );
    }

    #[test]
    fn a_visit_with_no_volume_root_or_off_its_root_is_not_recorded() {
        let visit = at("smb-192-168-50-10-445-testshare", "/Volumes/testshare/documents");
        assert_eq!(store_path_of_visit(&visit, None), None, "not registered");
        assert_eq!(
            store_path_of_visit(&visit, Some("/Volumes/testshare-1")),
            None,
            "a path the volume doesn't hold"
        );
    }
}
