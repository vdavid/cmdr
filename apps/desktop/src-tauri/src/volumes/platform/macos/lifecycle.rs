//! What a volume's mount, unmount, and rename mean to Cmdr (macOS).
//!
//! `watcher.rs` observes the `NSWorkspace` notifications and hands each volume path here. These
//! handlers register and unregister the volume, stop or move its index, try the SMB upgrade, and
//! tell the frontend. Why a renamed volume moves rather than leaving and arriving:
//! `../file_system/volume/DETAILS.md` § "A renamed drive".

use cmdr_fs::log_private::LogPrivate;
use log::{debug, error};
use std::sync::OnceLock;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::file_system::volume::manager::{RootMove, RootRemoval};
use crate::volume_broadcast::{RootChangeKind, VolumeMounted, VolumeRootChanged, VolumeUnmounted};

/// Global app handle for emitting events from the handlers.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Hand the handlers the app handle they emit through. `false` when one was already set, which
/// is what keeps `start_volume_watcher` idempotent.
pub(super) fn set_app_handle(app: &AppHandle) -> bool {
    APP_HANDLE.set(app.clone()).is_ok()
}

/// Handle a mount notification: register the volume, attempt SMB upgrade,
/// emit the per-volume Tauri event, and broadcast a volume-list refresh.
///
/// Public for tests so the handler logic can be exercised without posting
/// real `NSWorkspace` notifications.
pub(crate) fn handle_volume_mounted(volume_path: &str) {
    // Another account's own mount is nothing this account can open, so it's not
    // news here: no registration, no event, no refresh. The startup sweep and
    // discovery skip the same rows (`mounts::mount_is_private_to_another_user`).
    if super::is_private_to_another_user(volume_path) {
        debug!(
            "Another account's mount appeared; leaving it alone: {}",
            LogPrivate::dir_path(volume_path)
        );
        return;
    }

    debug!("Volume mounted: {}", LogPrivate::dir_path(volume_path));

    // Same funnel the startup sweep uses, so a drive that arrives now and one
    // that was already there land under the same ID with the same backend.
    crate::file_system::volume::mount_registration::register_mount_root(volume_path);

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    try_upgrade_smb_mount(volume_path);

    if let Some(app) = APP_HANDLE.get() {
        let payload = VolumeMounted {
            volume_path: volume_path.to_string(),
        };
        if let Err(e) = payload.emit(app) {
            error!("Failed to emit volume-mounted event: {}", e);
        }
    }

    crate::volume_broadcast::emit_volumes_changed();
}

/// Handle an unmount notification: drop the gone mount root from the volume that
/// owned it, emit the per-volume Tauri event, and broadcast.
///
/// A volume can be reached through several mounts, so losing one usually means
/// promoting a survivor rather than saying goodbye; only the LAST root going
/// away tears the volume down (`on_unmount`, index stop). The
/// `volume-unmounted` event fires either way, because the PATH really is gone
/// and a pane sitting under it has to move.
///
/// Public for tests so the handler logic can be exercised without posting
/// real `NSWorkspace` notifications.
pub(crate) fn handle_volume_unmounted(volume_path: &str) {
    debug!("Volume unmounted: {}", LogPrivate::dir_path(volume_path));

    // Drop the gone mount from the volume that owned it. Look up by root rather
    // than by path-derived ID: by the time the unmount notification fires,
    // `statfs(volume_path)` no longer returns the SMB mount info, so a
    // path-derived ID would miss the SMB volume we care about. See
    // `VolumeManager::remove_root`.
    let manager = crate::file_system::volume::manager::get_volume_manager();
    match manager.remove_root(std::path::Path::new(volume_path)) {
        RootRemoval::Unregistered { id, volume } => {
            // The last mount of this filesystem is gone, so the volume really is
            // leaving. `on_unmount` lets an `SmbVolume` disconnect its session.
            volume.on_unmount();
            // Cleanup (the volume is ALREADY gone — not wedge-prevention): a
            // LocalExternal drive that unmounts leaves a dangling index instance
            // holding an FSEvents watcher and open SQLite handles on a path that no
            // longer exists. Stop it so those resources are released. The wedge-safe
            // point is BEFORE the unmount (Cmdr's own eject-stop, and the best-effort
            // `WillUnmount` handler); by here the unmount has already happened.
            // SMB/MTP tear their indexes down through their own paths, so this acts
            // only for a `LocalExternal`.
            stop_local_external_index_off_main(id.clone());
            // The frontend retires a slow-connection notice about this share now, so
            // its server's next genuine fallback has to be news again.
            crate::network::os_mount_notice::forget_unmounted_volume(&id);
            debug!("Unregistered volume: {} ({})", id, LogPrivate::dir_path(volume_path));
        }
        RootRemoval::Promoted { id, new_root } => {
            // ❌ No `on_unmount` and no index stop here: the filesystem is still
            // reachable, just through another mount point.
            log::info!(
                target: "cmdr_lib::volumes",
                "{volume_path} unmounted, but volume {id} is still mounted at {}; promoted it to that root.",
                LogPrivate::dir_path(&new_root),
                volume_path = LogPrivate::dir_path(volume_path),
            );
            // A local drive's index read the gone root itself, so it restarts at the
            // survivor, the same as after a rename. Off this main-thread observer: the
            // restart drains.
            std::thread::spawn(move || follow_with_the_index(&id));
        }
        RootRemoval::ActiveRootStranded { id } => {
            log::warn!(
                target: "cmdr_lib::volumes",
                "{volume_path} unmounted and volume {id} can't move to one of its other mounts, so it stays there. Its own transport decides whether it keeps working.",
                volume_path = LogPrivate::dir_path(volume_path),
            );
        }
        RootRemoval::SiblingDropped { id } => {
            debug!(
                "{volume_path} unmounted; volume {id} still serves from its active root",
                volume_path = LogPrivate::dir_path(volume_path),
            );
        }
        RootRemoval::Unknown => {
            // Nothing knew this root. Fall back to a path-derived ID, which is
            // only unambiguous for a local volume, and is what registration
            // would have used for one.
            unregister_volume_from_manager(volume_path);
        }
    }

    if let Some(app) = APP_HANDLE.get() {
        let payload = VolumeUnmounted {
            volume_path: volume_path.to_string(),
            // A mount watcher speaks in paths: the id it resolved doesn't always
            // mean "gone" (a promoted volume keeps serving from another mount),
            // so the consumer looks the row up as it always has.
            volume_id: None,
        };
        if let Err(e) = payload.emit(app) {
            error!("Failed to emit volume-unmounted event: {}", e);
        }
    }

    crate::volume_broadcast::emit_volumes_changed();
}

/// Handle a **pre-unmount** notification: best-effort stop of a `LocalExternal`
/// index BEFORE the volume actually unmounts.
///
/// This is the earliest hook macOS offers for an OS/Finder-initiated eject. It's
/// RACY — the OS doesn't wait for our observer, so the unmount can proceed
/// concurrently — which is why Cmdr's own eject command (which stops the index
/// synchronously before `diskutil`) is the RELIABLE wedge-safe path. Still worth
/// doing: releasing the FSEvents watcher + SQLite handles before the unmount is the
/// only thing that can keep an open stream/handle from wedging a FSKit (`msdos`)
/// unmount. Acts only for a `LocalExternal` index (SMB/MTP have their own teardown).
///
/// Public for tests so the handler logic can be exercised without posting real
/// `NSWorkspace` notifications.
pub(crate) fn handle_volume_will_unmount(volume_path: &str) {
    debug!("Volume will unmount: {}", LogPrivate::dir_path(volume_path));
    // The volume is still mounted here, so look it up by root the same way the
    // post-unmount path does (robust to SMB/case-folded ids). `find_by_root`
    // also matches a FALLBACK root, and a volume losing one of its spare mounts
    // keeps working — stopping its index then would be a wedge risk traded for a
    // needless rescan — so act only when the ACTIVE root is the one going away.
    if let Some((id, volume)) =
        crate::file_system::volume::manager::get_volume_manager().find_by_root(std::path::Path::new(volume_path))
        && volume.root() == std::path::Path::new(volume_path)
    {
        stop_local_external_index_off_main(id);
    }
}

/// How long an unmount hook's stop waits for a drive's index to let go of it.
///
/// Nobody waits on the answer: the OS unmounts on its own schedule, so this only
/// decides when the stopping thread gives up and says so in the log. The same tier
/// as the eject's own index-stop deadline, so both paths call a stop "still
/// releasing" by one clock.
const INDEX_RELEASE_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

/// Stop a registered `LocalExternal` index for `volume_id`, releasing its FSEvents
/// watcher and SQLite handles, and wait up to [`INDEX_RELEASE_WAIT`] for it to let
/// go of the drive. `NothingToStop` for a non-`LocalExternal` or unindexed volume:
/// SMB and MTP indexes tear down through their own disconnect paths, and stopping
/// them here would fight those.
///
/// Through the drive-release gate, like every stop Cmdr decides on: it waits for a
/// start in flight to return, and moves the volume's epoch so no resume recorded
/// before it brings the index back.
///
/// Synchronous. The `NSWorkspace` observer blocks run on the MAIN THREAD and
/// `stop_indexing`'s drain can take a few seconds, so the observer callers wrap this
/// via [`stop_local_external_index_off_main`]; tests call it directly for a
/// deterministic result.
fn stop_local_external_index(volume_id: &str) -> cmdr_index::RemovableStop {
    use crate::file_system::volume::drive_release::{self, VolumeRelease};
    use cmdr_index::RemovableStop;

    let release = drive_release::gate().release(
        &[volume_id.to_string()],
        std::time::Instant::now() + INDEX_RELEASE_WAIT,
        |id| crate::index_host::index().stop_removable_volume(id, INDEX_RELEASE_WAIT),
        |late| {
            debug!(
                "The index for {} answered its unmount stop late: {:?}",
                late.volume_id, late.outcome
            )
        },
    );
    match release.outcome(volume_id) {
        Some(VolumeRelease::Released { .. }) => RemovableStop::Released,
        Some(VolumeRelease::StillReleasing) => RemovableStop::StillReleasing,
        Some(VolumeRelease::NothingToStop) | None => RemovableStop::NothingToStop,
    }
}

/// Run [`stop_local_external_index`] off the main thread. The `NSWorkspace` observer
/// blocks fire on the main thread and `stop_indexing` blocks for the drain (up to a
/// few seconds), so it must never run inline or the UI would hang mid-unmount.
///
/// Neither hook can refuse anything, so the answer goes to the log. A drive that
/// unmounts while its index is still letting go of it is the shape the FSKit wedge
/// needs, so that one is a `warn`: it's the line to find after a hung unmount.
fn stop_local_external_index_off_main(volume_id: String) {
    // With the DiskArbitration approver installed, a drive that went away is stopped by the cause
    // machine that also knows WHY it went (`unmount_approver/causes.rs`), so this hook would only
    // race it to the same stop and log half the story. It stays the whole cleanup on a Mac where the
    // approver couldn't install, which is also where the `WillUnmount` fallback lives.
    #[cfg(target_os = "macos")]
    if crate::volumes::unmount_approver::is_installed() {
        return;
    }
    // Skip the thread spawn entirely for the common non-LocalExternal case (root,
    // SMB, MTP): the kind check is a cheap registry lock.
    if crate::index_host::index().volume_kind(&volume_id) != Some(cmdr_index::IndexVolumeKind::LocalExternal) {
        return;
    }
    std::thread::spawn(move || match stop_local_external_index(&volume_id) {
        cmdr_index::RemovableStop::StillReleasing => log::warn!(
            target: "cmdr_lib::volumes",
            "{volume_id} unmounted while its index was still letting go of it ({} s wait ran out)",
            INDEX_RELEASE_WAIT.as_secs()
        ),
        outcome @ (cmdr_index::RemovableStop::Released | cmdr_index::RemovableStop::NothingToStop) => {
            debug!("Index stop for unmounting volume {volume_id}: {outcome:?}");
        }
    });
}

/// Unregister a volume no registration claimed by root, by deriving its ID from
/// the path.
///
/// The last-resort arm of the unmount path. Only safe for a local volume, where
/// `volume_id_for_mount` is unambiguous; for a gone SMB mount it derives the
/// WRONG id (`statfs` can no longer recover the share), which is exactly why the
/// root-keyed lookup runs first.
fn unregister_volume_from_manager(volume_path: &str) {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume_id = super::volume_id_for_mount(volume_path);
    get_volume_manager().unregister(&volume_id);
    debug!(
        "Unregistered volume: {} ({})",
        volume_id,
        LogPrivate::dir_path(volume_path)
    );
}

/// Tries to upgrade an SMB mount to a direct smb2 connection in the background.
///
/// Best-effort: if the upgrade fails, the volume stays as a `LocalPosixVolume`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn try_upgrade_smb_mount(volume_path: &str) {
    use crate::file_system::is_direct_smb_enabled;
    use crate::volumes::get_smb_mount_info;

    if !is_direct_smb_enabled() {
        return;
    }

    let Some(info) = get_smb_mount_info(volume_path) else {
        return;
    };

    // The upgrade holds the mDNS browse for its own hostname resolution
    // (`network::smb_server_address::discover_server`).
    let mount_path = volume_path.to_string();
    // A share that mounts while the app runs is usually one someone mounted in
    // Finder a moment ago, so they're watching: a failure is worth saying.
    tauri::async_runtime::spawn(async move {
        crate::network::smb_upgrade::resolve_and_register_smb_volume(
            &info.server,
            &info.share,
            &mount_path,
            info.port,
            crate::network::os_mount_notice::FallbackNotice::Announce,
        )
        .await;
    });
}

/// Handle a rename notification: a mounted volume's mount point moved from `old_path` to
/// `new_path`, and the filesystem never left (macOS posts no unmount or mount around it).
///
/// The volume keeps its id, so the registry moves the root (`VolumeManager::move_root`), the
/// panes follow it (`volume-root-changed`, kind `Moved`, so a pane deep inside keeps its place),
/// and the drive's index restarts at the new root through the drive-release gate, like every
/// non-root start. ❌ Never model it as unmount + mount: that stops the index and sends every pane
/// on the drive home.
///
/// A volume whose id doesn't survive the rename (no UUID, so it keys on its path) IS a different
/// id afterwards, and for that one unmount + mount is the honest model.
///
/// Blocking (an NSURL read for the id, then the index's drain): the observer runs it on a thread
/// of its own. Public for tests so the handler logic can be exercised without posting real
/// `NSWorkspace` notifications.
pub(crate) fn handle_volume_renamed(old_path: &str, new_path: &str) {
    debug!(
        "Volume renamed: {old_path} -> {new_path}",
        old_path = LogPrivate::dir_path(old_path),
        new_path = LogPrivate::dir_path(new_path),
    );
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let keeps_its_id = manager
        .find_by_root(std::path::Path::new(old_path))
        .is_some_and(|(id, _)| super::volume_id_for_mount(new_path) == id);
    if !keeps_its_id {
        handle_volume_unmounted(old_path);
        handle_volume_mounted(new_path);
        return;
    }

    match manager.move_root(std::path::Path::new(old_path), std::path::Path::new(new_path)) {
        RootMove::Moved { id } => {
            crate::volume_broadcast::emit_volume_root_changed(VolumeRootChanged {
                volume_id: id.clone(),
                old_root: old_path.to_string(),
                new_root: new_path.to_string(),
                old_landing: old_path.to_string(),
                new_landing: new_path.to_string(),
                kind: RootChangeKind::Moved,
            });
            follow_with_the_index(&id);
        }
        RootMove::SiblingMoved { id } => debug!(
            "{old_path} renamed; volume {id} still serves from its active root",
            old_path = LogPrivate::dir_path(old_path),
        ),
        RootMove::BackendCantReroot { id } => log::warn!(
            target: "cmdr_lib::volumes",
            "{old_path} was renamed to {new_path}, and volume {id} can't move with it, so it stays on the old root",
            old_path = LogPrivate::dir_path(old_path),
            new_path = LogPrivate::dir_path(new_path),
        ),
        // Checked a moment ago, so only a concurrent unmount gets here: it's gone either way.
        RootMove::Unknown => handle_volume_mounted(new_path),
    }
    crate::volume_broadcast::emit_volumes_changed();
}

/// Restart a renamed drive's index at its new root, through the drive-release gate: an eject
/// landing beside the rename must not find a watcher this restart stood up behind its stop.
fn follow_with_the_index(volume_id: &str) {
    use crate::file_system::volume::drive_release::{self, Gated, StartKind};

    let gated = drive_release::gate().start_blocking(volume_id, StartKind::DriveRenamed, || {
        crate::index_host::index().follow_volume_move(volume_id)
    });
    match gated {
        Gated::Ran(true) => log::info!(target: "cmdr_lib::volumes", "{volume_id}'s index follows it to its new name"),
        Gated::Ran(false) => debug!("{volume_id} was renamed with no index to follow it"),
        Gated::Skipped(reason) => {
            log::info!(target: "cmdr_lib::volumes", "{volume_id} was renamed but its index didn't follow: {reason:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_serializes_with_camel_case_key() {
        let payload = VolumeMounted {
            volume_path: "/Volumes/MyDrive".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("volumePath"), "expected camelCase 'volumePath' in {json}");
        assert!(json.contains("/Volumes/MyDrive"));
    }

    #[test]
    fn handle_volume_mounted_registers_with_volume_manager() {
        use crate::file_system::volume::manager::get_volume_manager;

        // Unique path so this test doesn't collide with parallel tests.
        let volume_path = "/Volumes/cmdr-test-mount-register";
        let volume_id = super::super::volume_id_for_mount(volume_path);

        // Make sure we start clean.
        get_volume_manager().unregister(&volume_id);
        assert!(
            get_volume_manager().get(&volume_id).is_none(),
            "precondition: volume should not be registered"
        );

        handle_volume_mounted(volume_path);

        assert!(
            get_volume_manager().get(&volume_id).is_some(),
            "expected volume registered after mount handler"
        );

        get_volume_manager().unregister(&volume_id);
    }

    #[test]
    fn handle_volume_unmounted_unregisters_from_volume_manager() {
        use crate::file_system::volume::manager::get_volume_manager;
        use cmdr_local::LocalPosixVolume;
        use std::sync::Arc;

        let volume_path = "/Volumes/cmdr-test-mount-unregister";
        let volume_id = super::super::volume_id_for_mount(volume_path);

        // Pre-register so the unmount handler has something to remove.
        let volume = Arc::new(LocalPosixVolume::local_folder(
            "cmdr-test",
            volume_path,
            crate::volume_host::host(),
        ));
        get_volume_manager().register_if_absent(&volume_id, volume);
        assert!(
            get_volume_manager().get(&volume_id).is_some(),
            "precondition: volume should be registered"
        );

        handle_volume_unmounted(volume_path);

        assert!(
            get_volume_manager().get(&volume_id).is_none(),
            "expected volume unregistered after unmount handler"
        );
    }

    #[test]
    fn unmounting_the_active_root_promotes_a_surviving_sibling() {
        use crate::file_system::volume::manager::get_volume_manager;
        use cmdr_local::LocalPosixVolume;
        use std::sync::Arc;

        // One share, two mount points: macOS suffixes the later mount. Both
        // derive the same volume ID, so ejecting the first must hand the ID to
        // the mount that's still live rather than dropping the share until the
        // app restarts (discovery only runs at launch).
        let first = "/Volumes/cmdr-test-promote";
        let second = "/Volumes/cmdr-test-promote-1";
        let volume_id = "cmdr-test-promote-share";
        let manager = get_volume_manager();
        manager.unregister(volume_id);

        manager.register(
            volume_id,
            Arc::new(LocalPosixVolume::local_folder(
                "share",
                first,
                crate::volume_host::host(),
            )),
        );
        // The second mount event: the incumbent keeps the ID, the new root is
        // recorded as a fallback.
        manager.register_if_absent(
            volume_id,
            Arc::new(LocalPosixVolume::local_folder(
                "share",
                second,
                crate::volume_host::host(),
            )),
        );

        handle_volume_unmounted(first);

        let promoted = manager
            .get(volume_id)
            .expect("a live sibling mount keeps the volume registered");
        assert_eq!(
            promoted.root(),
            std::path::Path::new(second),
            "the surviving mount becomes the active root"
        );

        // The last root going away still unregisters.
        handle_volume_unmounted(second);
        assert!(manager.get(volume_id).is_none(), "the last root gone means gone");
    }

    #[test]
    fn mount_then_unmount_round_trip_leaves_no_registration() {
        use crate::file_system::volume::manager::get_volume_manager;

        let volume_path = "/Volumes/cmdr-test-roundtrip";
        let volume_id = super::super::volume_id_for_mount(volume_path);
        get_volume_manager().unregister(&volume_id);

        handle_volume_mounted(volume_path);
        assert!(get_volume_manager().get(&volume_id).is_some());

        handle_volume_unmounted(volume_path);
        assert!(get_volume_manager().get(&volume_id).is_none());
    }

    #[test]
    fn stopping_a_registered_local_external_index_removes_its_instance() {
        use cmdr_index::IndexVolumeKind;
        use cmdr_index::testing::{is_index_active, reserve_initializing_index_for_test};

        // A LocalExternal drive whose volume unmounts: the cleanup must stop the
        // index so its dangling FSEvents watcher + SQLite handles are released.
        // Pre-fix this would have passed wrongly: the unmount path never touched
        // indexing, so the instance survived (a leaked watcher/handles on a gone volume).
        let vid = "volumes-cmdr-test-unmount-cleanup";
        let _tmp = reserve_initializing_index_for_test(vid, IndexVolumeKind::LocalExternal);
        assert!(is_index_active(vid), "precondition: the index is active");

        assert_eq!(
            stop_local_external_index(vid),
            cmdr_index::RemovableStop::Released,
            "a registered LocalExternal index must be stopped on unmount"
        );
        assert!(!is_index_active(vid), "the instance must be removed after the stop");
    }

    #[test]
    fn unmount_cleanup_leaves_a_non_local_external_index_alone() {
        use cmdr_index::IndexVolumeKind;
        use cmdr_index::testing::{is_index_active, reserve_initializing_index_for_test};

        // SMB/MTP indexes tear down through their own disconnect paths; the local
        // unmount cleanup must never stop one (it would fight that teardown).
        let vid = "volumes-cmdr-test-unmount-smb-like";
        let _tmp = reserve_initializing_index_for_test(vid, IndexVolumeKind::Smb);
        assert!(is_index_active(vid), "precondition: the SMB index is active");

        assert_eq!(
            stop_local_external_index(vid),
            cmdr_index::RemovableStop::NothingToStop,
            "the local-external unmount cleanup must not stop an SMB index"
        );
        assert!(is_index_active(vid), "the SMB instance must be left intact");

        // Cleanup so the shared registry doesn't carry this test's instance.
        let _ = crate::index_host::index().forget_volume(vid);
    }
}
