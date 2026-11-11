//! Volume mount/unmount watcher for macOS.
//!
//! Subscribes to `NSWorkspace`'s mount, unmount, and rename notifications. When the OS
//! mounts a volume (USB drive, disk image, SMB share, etc.), `diskarbitrationd`
//! posts `NSWorkspaceDidMountNotification` on the shared workspace's
//! notification center. By the time our observer fires, the volume is fully
//! mounted and `NSFileManager` metadata is ready. No fsid settle dance needed.
//!
//! This file is only the observer plumbing: each block hands the volume path to its handler in
//! `lifecycle.rs`, which decides what the event means.
//!
//! See `apps/desktop/src-tauri/src/volumes/CLAUDE.md` for the rationale on
//! choosing NSWorkspace over FSEvents and DiskArbitration.

use block2::RcBlock;
use log::debug;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceDidMountNotification, NSWorkspaceDidRenameVolumeNotification,
    NSWorkspaceDidUnmountNotification, NSWorkspaceVolumeOldURLKey, NSWorkspaceVolumeURLKey,
    NSWorkspaceWillUnmountNotification,
};
use objc2_foundation::{NSDictionary, NSNotification, NSString, NSURL};
use std::ptr::NonNull;
use std::sync::OnceLock;
use tauri::AppHandle;

use super::lifecycle::{
    handle_volume_mounted, handle_volume_renamed, handle_volume_unmounted, handle_volume_will_unmount, set_app_handle,
};

/// Marker: set after the NSWorkspace observer has been installed.
/// Idempotency gate so repeat calls to `start_volume_watcher` don't double-subscribe.
static OBSERVER_INSTALLED: OnceLock<()> = OnceLock::new();

/// Start observing volume mount/unmount notifications. Idempotent.
///
/// Call once at app setup. Subsequent calls are no-ops.
pub fn start_volume_watcher(app: &AppHandle) {
    if !set_app_handle(app) {
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

    let rename_block = RcBlock::new(|n: NonNull<NSNotification>| {
        // SAFETY: NSNotificationCenter delivers a valid notification pointer.
        let notification = unsafe { n.as_ref() };
        // SAFETY: `NSWorkspaceVolumeOldURLKey` is a `&'static NSString` constant from AppKit (an
        // `extern "C"` static, so reading it requires `unsafe`).
        let old_key: &NSString = unsafe { NSWorkspaceVolumeOldURLKey };
        match (
            volume_path_from_notification(notification),
            path_under_key(notification, old_key),
        ) {
            // Off the main thread: following the rename derives an id (an NSURL read) and
            // restarts the drive's index, which drains for up to a few seconds.
            (Some(new_path), Some(old_path)) => {
                std::thread::spawn(move || handle_volume_renamed(&old_path, &new_path));
            }
            _ => debug!("NSWorkspaceDidRenameVolumeNotification missing a volume URL"),
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
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidRenameVolumeNotification),
            None,
            None,
            &rename_block,
        );
    }

    debug!("NSWorkspace volume mount/unmount/rename observer installed");
}

/// Marker: set after the will-unmount observer has been installed.
static WILL_UNMOUNT_OBSERVER_INSTALLED: OnceLock<()> = OnceLock::new();

/// Install the `NSWorkspaceWillUnmountNotification` observer: the FALLBACK pre-unmount hook, for
/// when the DiskArbitration approver can't install (`volumes/platform/macos/unmount_approver/`). It's racy — the
/// OS doesn't wait for the observer — so it only ever runs instead of the approver, never beside
/// it, or one unmount would stop the same index twice. Idempotent.
pub(crate) fn install_will_unmount_observer() {
    if WILL_UNMOUNT_OBSERVER_INSTALLED.set(()).is_err() {
        return;
    }
    let will_unmount_block = RcBlock::new(|n: NonNull<NSNotification>| {
        // SAFETY: NSNotificationCenter delivers a valid notification pointer.
        let notification = unsafe { n.as_ref() };
        if let Some(path) = volume_path_from_notification(notification) {
            handle_volume_will_unmount(&path);
        } else {
            debug!("NSWorkspaceWillUnmountNotification missing NSWorkspaceVolumeURLKey");
        }
    });

    // SAFETY: the notification name constant is a valid AppKit global, and
    // `addObserverForName:object:queue:usingBlock:` retains the block for the lifetime of the
    // observer registration, which is never removed (as above).
    unsafe {
        NSWorkspace::sharedWorkspace()
            .notificationCenter()
            .addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceWillUnmountNotification),
                None,
                None,
                &will_unmount_block,
            );
    }

    debug!("NSWorkspace will-unmount observer installed as the pre-unmount fallback");
}

/// Extract the volume path from an `NSWorkspace` mount/unmount notification's
/// `userInfo` dictionary.
///
/// `NSWorkspaceVolumeURLKey` carries the file URL of the (un)mounted volume.
/// Returns `None` if `userInfo` is missing the key (defensive: AppKit always
/// includes it for these notifications, but synthetic posts (e.g. tests) might
/// not).
pub(crate) fn volume_path_from_notification(notification: &NSNotification) -> Option<String> {
    // SAFETY: `NSWorkspaceVolumeURLKey` is a `&'static NSString` constant from AppKit (an
    // `extern "C"` static, so reading it requires `unsafe`).
    let key: &NSString = unsafe { NSWorkspaceVolumeURLKey };
    path_under_key(notification, key)
}

/// The file path of the `NSURL` a volume notification's `userInfo` carries under `key`.
fn path_under_key(notification: &NSNotification, key: &NSString) -> Option<String> {
    let user_info = notification.userInfo()?;

    // SAFETY: the notification's `userInfo` is `NSDictionary<NSString *, id>` per Apple docs, and
    // every observed volume notification carries an `NSURL` under `NSWorkspaceVolumeURLKey` (a
    // rename also under `NSWorkspaceVolumeOldURLKey`), the only two keys callers pass. We narrow the
    // value type to `NSURL` so `objectForKey` returns the URL directly; the cast only refines the
    // generic value type of the same live dictionary, not its identity, and the values read through
    // it are the URL ones.
    let typed: Retained<NSDictionary<NSString, NSURL>> = unsafe { Retained::cast_unchecked(user_info) };

    let url = typed.objectForKey(key)?;

    let ns_path = url.path()?;
    Some(ns_path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::{NSDictionary, NSString, NSURL};

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
        use crate::file_system::volume::manager::get_volume_manager;

        // Ensure the observer is wired up. Idempotent, safe to call from
        // multiple tests; only the first call actually installs.
        install_observers();

        let volume_path = "/Volumes/cmdr-test-e2e-post";
        let volume_id = super::super::volume_id_for_mount(volume_path);

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
