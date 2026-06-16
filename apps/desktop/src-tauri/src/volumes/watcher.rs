//! Volume mount/unmount watcher for macOS.
//!
//! Subscribes to `NSWorkspace`'s mount/unmount notifications. When the OS
//! mounts a volume (USB drive, disk image, SMB share, etc.), `diskarbitrationd`
//! posts `NSWorkspaceDidMountNotification` on the shared workspace's
//! notification center. By the time our observer fires, the volume is fully
//! mounted and `NSFileManager` metadata is ready. No fsid settle dance needed.
//!
//! See `apps/desktop/src-tauri/src/volumes/CLAUDE.md` for the rationale on
//! choosing NSWorkspace over FSEvents and DiskArbitration.

use block2::RcBlock;
use log::{debug, error};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceDidMountNotification, NSWorkspaceDidUnmountNotification, NSWorkspaceVolumeURLKey,
};
use objc2_foundation::{NSDictionary, NSNotification, NSString, NSURL};
use std::ptr::NonNull;
use std::sync::OnceLock;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::volume_broadcast::{VolumeMounted, VolumeUnmounted};

/// Global app handle for emitting events from the observer.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Marker: set after the NSWorkspace observer has been installed.
/// Idempotency gate so repeat calls to `start_volume_watcher` don't double-subscribe.
static OBSERVER_INSTALLED: OnceLock<()> = OnceLock::new();

/// Start observing volume mount/unmount notifications. Idempotent.
///
/// Call once at app setup. Subsequent calls are no-ops.
pub fn start_volume_watcher(app: &AppHandle) {
    if APP_HANDLE.set(app.clone()).is_err() {
        debug!("Volume watcher already initialized");
        return;
    }
    install_observers();
}

fn install_observers() {
    if OBSERVER_INSTALLED.set(()).is_err() {
        return;
    }

    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();

    let mount_block = RcBlock::new(|n: NonNull<NSNotification>| {
        // SAFETY: NSNotificationCenter delivers a valid notification pointer.
        let notification = unsafe { n.as_ref() };
        if let Some(path) = volume_path_from_notification(notification) {
            handle_volume_mounted(&path);
        } else {
            debug!("NSWorkspaceDidMountNotification missing NSWorkspaceVolumeURLKey");
        }
    });

    let unmount_block = RcBlock::new(|n: NonNull<NSNotification>| {
        // SAFETY: NSNotificationCenter delivers a valid notification pointer.
        let notification = unsafe { n.as_ref() };
        if let Some(path) = volume_path_from_notification(notification) {
            handle_volume_unmounted(&path);
        } else {
            debug!("NSWorkspaceDidUnmountNotification missing NSWorkspaceVolumeURLKey");
        }
    });

    // SAFETY: the notification name constants are valid AppKit globals, and
    // `addObserverForName:object:queue:usingBlock:` retains the block for the
    // lifetime of the observer registration. We never remove the observer
    // (mirrors the pattern in `file_system/open_with.rs`), so the block lives
    // for the rest of the process.
    unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidMountNotification),
            None,
            None,
            &mount_block,
        );
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidUnmountNotification),
            None,
            None,
            &unmount_block,
        );
    }

    debug!("NSWorkspace volume mount/unmount observer installed");
}

/// Extract the volume path from an `NSWorkspace` mount/unmount notification's
/// `userInfo` dictionary.
///
/// `NSWorkspaceVolumeURLKey` carries the file URL of the (un)mounted volume.
/// Returns `None` if `userInfo` is missing the key (defensive: AppKit always
/// includes it for these notifications, but synthetic posts (e.g. tests) might
/// not).
pub(crate) fn volume_path_from_notification(notification: &NSNotification) -> Option<String> {
    let user_info = notification.userInfo()?;

    // SAFETY: the notification's `userInfo` is `NSDictionary<NSString *, id>` per Apple docs, and
    // every observed mount/unmount notification carries an `NSURL` under `NSWorkspaceVolumeURLKey`.
    // We narrow the value type to `NSURL` so `objectForKey` returns the URL directly; the cast only
    // refines the generic value type of the same live dictionary, not its identity.
    let typed: Retained<NSDictionary<NSString, NSURL>> = unsafe { Retained::cast_unchecked(user_info) };

    // SAFETY: `NSWorkspaceVolumeURLKey` is a `&'static NSString` constant from AppKit (an
    // `extern "C"` static, so reading it requires `unsafe`).
    let key: &NSString = unsafe { NSWorkspaceVolumeURLKey };
    let url = typed.objectForKey(key)?;

    let ns_path = url.path()?;
    Some(ns_path.to_string())
}

/// Handle a mount notification: register the volume, attempt SMB upgrade,
/// emit the per-volume Tauri event, and broadcast a volume-list refresh.
///
/// Public for tests so the handler logic can be exercised without posting
/// real `NSWorkspace` notifications.
pub(crate) fn handle_volume_mounted(volume_path: &str) {
    debug!("Volume mounted: {}", volume_path);

    register_volume_with_manager(volume_path);

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

/// Handle an unmount notification: tear down the SMB session if any,
/// unregister the volume, emit the per-volume Tauri event, and broadcast.
///
/// Public for tests so the handler logic can be exercised without posting
/// real `NSWorkspace` notifications.
pub(crate) fn handle_volume_unmounted(volume_path: &str) {
    debug!("Volume unmounted: {}", volume_path);

    // Call `on_unmount` before unregistering so an `SmbVolume` can disconnect
    // its smb2 session cleanly. Look up by root rather than by path-derived ID:
    // by the time the unmount notification fires, `statfs(volume_path)` no longer
    // returns the SMB mount info, so a path-derived ID would miss the SMB volume
    // we actually need to clean up. See `VolumeManager::find_by_root`.
    let registered_id = {
        let manager = crate::file_system::get_volume_manager();
        let lookup = manager.find_by_root(std::path::Path::new(volume_path));
        if let Some((id, volume)) = &lookup {
            volume.on_unmount();
            Some(id.clone())
        } else {
            None
        }
    };

    unregister_volume_from_manager(volume_path, registered_id.as_deref());

    if let Some(app) = APP_HANDLE.get() {
        let payload = VolumeUnmounted {
            volume_path: volume_path.to_string(),
        };
        if let Err(e) = payload.emit(app) {
            error!("Failed to emit volume-unmounted event: {}", e);
        }
    }

    crate::volume_broadcast::emit_volumes_changed();
}

/// Register a mounted volume with the `VolumeManager`.
///
/// Uses `register_if_absent` so a pre-registered `SmbVolume` (from the mount
/// flow) is not replaced by a `LocalPosixVolume`.
fn register_volume_with_manager(volume_path: &str) {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::LocalPosixVolume;
    use std::path::Path;
    use std::sync::Arc;

    let volume_id = super::volume_id_for_mount(volume_path);

    let name = Path::new(volume_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let volume = Arc::new(LocalPosixVolume::new(&name, volume_path));
    let was_registered = get_volume_manager().register_if_absent(&volume_id, volume);
    if was_registered {
        debug!("Registered mounted volume: {} -> {}", volume_id, volume_path);
    } else {
        debug!(
            "Skipped registration for {} (already registered, likely SmbVolume)",
            volume_id
        );
    }
}

/// Unregister a volume from the `VolumeManager`.
///
/// If `registered_id` is `Some`, unregister that exact entry. Use this when the
/// caller has already looked up the volume via `find_by_root` (the unmount path
/// must do this because `statfs` no longer recovers SMB info after the mount is
/// gone). Otherwise, fall back to deriving the ID from the path, which is only
/// safe for local volumes where `path_to_id` is unambiguous.
fn unregister_volume_from_manager(volume_path: &str, registered_id: Option<&str>) {
    use crate::file_system::get_volume_manager;

    let volume_id = registered_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| super::volume_id_for_mount(volume_path));
    get_volume_manager().unregister(&volume_id);
    debug!("Unregistered volume: {} ({})", volume_id, volume_path);
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

    // Kick mDNS off here (idempotent, no-op if already running or if
    // `network.enabled` is off). In dev mode `network.firstTriggerDone` is
    // typically `false`, so the launch-time mDNS gate doesn't fire and
    // hostname resolution would otherwise miss when macOS auto-remounts an
    // SMB share at login. See `network::smb_upgrade::resolve_ip_to_hostname_with_wait`.
    if let Some(app) = APP_HANDLE.get() {
        crate::network::ensure_mdns_started(app.clone());
    }

    let mount_path = volume_path.to_string();
    tauri::async_runtime::spawn(async move {
        crate::network::smb_upgrade::resolve_and_register_smb_volume(&info.server, &info.share, &mount_path, info.port)
            .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::{NSDictionary, NSString, NSURL};

    #[test]
    fn payload_serializes_with_camel_case_key() {
        let payload = VolumeMounted {
            volume_path: "/Volumes/MyDrive".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("volumePath"), "expected camelCase 'volumePath' in {json}");
        assert!(json.contains("/Volumes/MyDrive"));
    }

    /// Builds a synthetic `NSNotification` whose `userInfo` carries a file URL
    /// under `NSWorkspaceVolumeURLKey`, matching the shape AppKit posts for
    /// real mount/unmount events.
    fn synthetic_volume_notification(volume_path: &str) -> Retained<NSNotification> {
        let path_ns = NSString::from_str(volume_path);
        let url = NSURL::fileURLWithPath(&path_ns);
        // SAFETY: (test) `NSWorkspaceVolumeURLKey` is an `extern "C"` `&'static NSString` AppKit
        // constant, valid for the process lifetime; reading the static requires `unsafe`.
        let key: &NSString = unsafe { NSWorkspaceVolumeURLKey };

        let user_info: Retained<NSDictionary<NSString, NSURL>> =
            NSDictionary::from_slices::<NSString>(&[key], &[&*url]);

        // SAFETY: (test) `NSWorkspaceDidMountNotification` is an `extern "C"` `&'static NSString`
        // AppKit constant, valid for the process lifetime.
        let name = unsafe { NSWorkspaceDidMountNotification };
        // SAFETY: (test) `user_info` is a live `NSDictionary<NSString, NSURL>`; the cast only erases
        // the generic value type to the base `NSDictionary` of the same live object.
        let user_info_any: Retained<NSDictionary> = unsafe { Retained::cast_unchecked(user_info) };
        // SAFETY: (test) `name` and `user_info_any` are live retained AppKit objects; the
        // initializer copies them, so no aliasing or lifetime concern.
        unsafe { NSNotification::notificationWithName_object_userInfo(name, None, Some(&user_info_any)) }
    }

    #[test]
    fn extracts_volume_path_from_well_formed_notification() {
        let notification = synthetic_volume_notification("/Volumes/MyDrive");
        let path = volume_path_from_notification(&notification);
        assert_eq!(path.as_deref(), Some("/Volumes/MyDrive"));
    }

    #[test]
    fn extracts_volume_path_with_unicode_name() {
        // Cyrillic and CJK characters in a single code point each. These
        // aren't decomposable, so they round-trip through `NSURL` cleanly.
        // Latin diacritics like "Útikönyv" do *not* round-trip: macOS file
        // URLs canonicalize to NFD (e.g. "Ú" → "U" + combining acute), which
        // is normal for paths returned from `NSURL.path()` and is what real
        // mount notifications also deliver.
        let notification = synthetic_volume_notification("/Volumes/Привет東京");
        let path = volume_path_from_notification(&notification);
        assert_eq!(path.as_deref(), Some("/Volumes/Привет東京"));
    }

    #[test]
    fn returns_none_when_user_info_missing() {
        // SAFETY: (test) `NSWorkspaceDidMountNotification` is an `extern "C"` `&'static NSString`
        // AppKit constant, valid for the process lifetime.
        let name = unsafe { NSWorkspaceDidMountNotification };
        // Notification with no userInfo. Defensive against malformed posts.
        // SAFETY: (test) `name` is a live retained `NSString`; the initializer copies it.
        let notification = unsafe { NSNotification::notificationWithName_object_userInfo(name, None, None) };
        assert!(volume_path_from_notification(&notification).is_none());
    }

    #[test]
    fn returns_none_when_volume_url_key_absent() {
        // userInfo is present but lacks NSWorkspaceVolumeURLKey.
        let other_key = NSString::from_str("UnrelatedKey");
        let other_value = NSString::from_str("UnrelatedValue");
        let user_info: Retained<NSDictionary<NSString, NSString>> =
            NSDictionary::from_slices::<NSString>(&[&other_key], &[&*other_value]);
        // SAFETY: (test) `user_info` is a live `NSDictionary<NSString, NSString>`; the cast only
        // erases the generic value type to the base `NSDictionary` of the same live object.
        let user_info_any: Retained<NSDictionary> = unsafe { Retained::cast_unchecked(user_info) };

        // SAFETY: (test) `NSWorkspaceDidMountNotification` is an `extern "C"` `&'static NSString`
        // AppKit constant, valid for the process lifetime.
        let name = unsafe { NSWorkspaceDidMountNotification };
        // SAFETY: (test) `name` and `user_info_any` are live retained AppKit objects; the
        // initializer copies them.
        let notification =
            unsafe { NSNotification::notificationWithName_object_userInfo(name, None, Some(&user_info_any)) };
        assert!(volume_path_from_notification(&notification).is_none());
    }

    #[test]
    fn handle_volume_mounted_registers_with_volume_manager() {
        use crate::file_system::get_volume_manager;

        // Unique path so this test doesn't collide with parallel tests.
        let volume_path = "/Volumes/cmdr-test-mount-register";
        let volume_id = super::super::path_to_id(volume_path);

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
        use crate::file_system::get_volume_manager;
        use crate::file_system::volume::LocalPosixVolume;
        use std::sync::Arc;

        let volume_path = "/Volumes/cmdr-test-mount-unregister";
        let volume_id = super::super::path_to_id(volume_path);

        // Pre-register so the unmount handler has something to remove.
        let volume = Arc::new(LocalPosixVolume::new("cmdr-test", volume_path));
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
    fn mount_then_unmount_round_trip_leaves_no_registration() {
        use crate::file_system::get_volume_manager;

        let volume_path = "/Volumes/cmdr-test-roundtrip";
        let volume_id = super::super::path_to_id(volume_path);
        get_volume_manager().unregister(&volume_id);

        handle_volume_mounted(volume_path);
        assert!(get_volume_manager().get(&volume_id).is_some());

        handle_volume_unmounted(volume_path);
        assert!(get_volume_manager().get(&volume_id).is_none());
    }

    /// End-to-end wire-up test: install the real NSWorkspace observer, post a
    /// synthetic mount notification on the workspace's notification center,
    /// and verify our handler actually ran (the volume becomes registered).
    ///
    /// `addObserverForName:object:queue:usingBlock:` with `queue: nil` delivers
    /// the block synchronously on the posting thread, so by the time
    /// `postNotification:` returns, our handler has already executed.
    ///
    /// This is the gold-standard test: it exercises the entire observer chain,
    /// not just the extraction helper. If the observer block isn't retained
    /// correctly, or if the cast/key lookup is wrong, this test catches it.
    #[test]
    fn end_to_end_post_notification_runs_handler() {
        use crate::file_system::get_volume_manager;

        // Ensure the observer is wired up. Idempotent, safe to call from
        // multiple tests; only the first call actually installs.
        install_observers();

        let volume_path = "/Volumes/cmdr-test-e2e-post";
        let volume_id = super::super::path_to_id(volume_path);

        // Start clean.
        get_volume_manager().unregister(&volume_id);
        assert!(get_volume_manager().get(&volume_id).is_none());

        // Build and post the notification on the actual NSWorkspace center
        // (same channel real mount events arrive on).
        let notification = synthetic_volume_notification(volume_path);
        let workspace = NSWorkspace::sharedWorkspace();
        let center = workspace.notificationCenter();
        center.postNotification(&notification);

        assert!(
            get_volume_manager().get(&volume_id).is_some(),
            "observer block did not fire for posted NSWorkspaceDidMountNotification"
        );

        // Cleanup.
        get_volume_manager().unregister(&volume_id);
    }
}
