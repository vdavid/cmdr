//! Native system notifications: sending one, and asking macOS whether Cmdr may.
//!
//! Sending goes through `tauri-plugin-notification`, which posts through the old
//! `NSUserNotificationCenter` API. That API can't say whether the user turned
//! Cmdr's notifications off, and the plugin's own permission check answers
//! "granted" on every desktop platform no matter what. So the permission
//! question goes to `UNUserNotificationCenter` instead, which reads the same
//! per-app switch in System Settings > Notifications.
//!
//! (verified on macOS 27.0, a probe `.app` posting through
//! `NSUserNotificationCenter` and reading `getNotificationSettings` while
//! System Settings' "Allow notifications" was toggled, 2026-10-06): never posted
//! reads `notDetermined`; after the first post, macOS asks the user and reads
//! `denied` until they answer; "Allow" reads `authorized`; switching it off and
//! on again reads `denied` and `authorized`, live, no relaunch needed.
//!
//! Two limits, both answered with [`NotificationPermission::Unknown`]:
//!
//! - `UNUserNotificationCenter` raises an Objective-C exception in a process
//!   that isn't a LaunchServices-known `.app` ("bundleProxyForCurrentProcess is
//!   nil"), which is what `pnpm dev` runs. Verified with the same probe run as a
//!   bare binary.
//! - Dev builds post as Terminal (the plugin's choice), so Cmdr's own switch
//!   says nothing about whether a dev banner shows.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// Whether macOS will show Cmdr's notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NotificationPermission {
    /// The user allowed them (including macOS's quiet "provisional" delivery).
    Allowed,
    /// The user switched them off, or hasn't answered macOS's first-post prompt yet.
    Denied,
    /// Cmdr hasn't posted yet. The first post makes macOS ask.
    NotDetermined,
    /// No way to tell: not macOS, a dev build, or macOS didn't answer.
    Unknown,
}

/// How long to wait for `usernoted` to answer. It's an XPC round-trip that
/// normally takes a few milliseconds.
#[cfg(target_os = "macos")]
const PERMISSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether macOS will show Cmdr's notifications right now. Asked per send, so a
/// user who switches them back on doesn't have to restart Cmdr.
#[tauri::command]
#[specta::specta]
pub async fn get_notification_permission() -> NotificationPermission {
    #[cfg(target_os = "macos")]
    {
        crate::deadline::blocking_with_timeout(
            PERMISSION_TIMEOUT,
            NotificationPermission::Unknown,
            macos::query_permission,
        )
        .await
    }
    #[cfg(not(target_os = "macos"))]
    {
        NotificationPermission::Unknown
    }
}

/// Send a native notification. An `Err` carries the plugin's message for the
/// log only; nobody reads it.
#[tauri::command]
#[specta::specta]
pub async fn show_notification(app: AppHandle, title: String, body: String) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|err| err.to_string())
}

#[cfg(target_os = "macos")]
mod macos {
    use std::panic::AssertUnwindSafe;
    use std::ptr::NonNull;
    use std::sync::mpsc;

    use objc2_foundation::NSBundle;
    use objc2_user_notifications::{UNAuthorizationStatus, UNNotificationSettings, UNUserNotificationCenter};

    use super::{NotificationPermission, PERMISSION_TIMEOUT};

    pub(super) fn from_authorization_status(status: UNAuthorizationStatus) -> NotificationPermission {
        match status {
            UNAuthorizationStatus::Authorized
            | UNAuthorizationStatus::Provisional
            | UNAuthorizationStatus::Ephemeral => NotificationPermission::Allowed,
            UNAuthorizationStatus::Denied => NotificationPermission::Denied,
            UNAuthorizationStatus::NotDetermined => NotificationPermission::NotDetermined,
            _ => NotificationPermission::Unknown,
        }
    }

    /// Blocks for the XPC round-trip; run it off the async runtime.
    pub(super) fn query_permission() -> NotificationPermission {
        if tauri::is_dev() || !NSBundle::mainBundle().bundlePath().to_string().ends_with(".app") {
            return NotificationPermission::Unknown;
        }

        let (tx, rx) = mpsc::sync_channel(1);
        let handler = block2::RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            // SAFETY: `getNotificationSettingsWithCompletionHandler:` calls this block with
            // a valid, non-null `UNNotificationSettings` that lives for the call.
            let status = unsafe { settings.as_ref() }.authorizationStatus();
            // A send fails only when the caller timed out and dropped `rx`; nobody is
            // waiting for the answer then.
            let _ = tx.send(status);
        });

        // The bundle check above keeps this from raising, and the catch keeps a
        // LaunchServices oddity we didn't foresee from aborting the app.
        let asked = objc2::exception::catch(AssertUnwindSafe(|| {
            UNUserNotificationCenter::currentNotificationCenter()
                .getNotificationSettingsWithCompletionHandler(&handler);
        }));
        if let Err(exception) = asked {
            log::warn!(
                target: "notifications",
                "macOS refused the notification permission question: {exception:?}",
            );
            return NotificationPermission::Unknown;
        }

        match rx.recv_timeout(PERMISSION_TIMEOUT) {
            Ok(status) => from_authorization_status(status),
            Err(_) => NotificationPermission::Unknown,
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use objc2_user_notifications::UNAuthorizationStatus;

    use super::NotificationPermission;
    use super::macos::from_authorization_status;

    #[test]
    fn every_kind_of_yes_is_allowed() {
        for status in [
            UNAuthorizationStatus::Authorized,
            UNAuthorizationStatus::Provisional,
            UNAuthorizationStatus::Ephemeral,
        ] {
            assert_eq!(from_authorization_status(status), NotificationPermission::Allowed);
        }
    }

    #[test]
    fn the_switched_off_state_is_denied() {
        assert_eq!(
            from_authorization_status(UNAuthorizationStatus::Denied),
            NotificationPermission::Denied
        );
    }

    #[test]
    fn never_posted_is_not_determined() {
        assert_eq!(
            from_authorization_status(UNAuthorizationStatus::NotDetermined),
            NotificationPermission::NotDetermined
        );
    }

    #[test]
    fn a_status_newer_than_this_build_is_unknown() {
        assert_eq!(
            from_authorization_status(UNAuthorizationStatus(99)),
            NotificationPermission::Unknown
        );
    }
}
