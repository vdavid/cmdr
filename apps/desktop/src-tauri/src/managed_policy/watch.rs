//! The two background refresh triggers on macOS: app activation, and a file watch on the folder
//! configuration profiles write to. Both only call [`super::refresh`]; CFPreferences stays the
//! source of truth. Why both: `DETAILS.md` § Refresh.

use std::path::Path;
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock};

use crate::ignore_poison::IgnorePoison;

/// Where profiles land: `<bundle>.plist` for device scope, `<user>/<bundle>.plist` for user scope.
/// Undocumented by Apple, which is why activation is a trigger too.
const MANAGED_PREFERENCES_DIR: &str = "/Library/Managed Preferences";

static WATCHER: OnceLock<Mutex<Option<notify::RecommendedWatcher>>> = OnceLock::new();

pub(super) fn start() {
    install_activation_observer();
    start_folder_watch();
}

fn refresh_and_log(trigger: &str) {
    if super::refresh() {
        log::debug!(target: "managed_policy", "Re-read the managed policy after {trigger}: changed");
    }
}

fn install_activation_observer() {
    use objc2_app_kit::NSApplicationDidBecomeActiveNotification;
    use objc2_foundation::{NSNotification, NSNotificationCenter};

    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return;
    }
    let block = block2::RcBlock::new(|_notification: NonNull<NSNotification>| {
        // The CFPreferences read is an XPC round trip to `cfprefsd`; keep it off the main thread.
        tauri::async_runtime::spawn_blocking(|| refresh_and_log("app activation"));
    });
    // ❗ The DEFAULT center: `NSApplication` posts its own activation there (see
    // `restricted_paths/mod.rs` for the verification).
    // SAFETY: `NSApplicationDidBecomeActiveNotification` is a valid notification name constant,
    // the default center lives for the whole process, and `block` is a live `RcBlock` with the
    // expected `(NonNull<NSNotification>) -> ()` signature. The center retains the observer for the
    // app's lifetime; we never remove it, so dropping the returned token is intended.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSApplicationDidBecomeActiveNotification),
            None,
            None,
            &block,
        );
    }
}

fn start_folder_watch() {
    let dir = Path::new(MANAGED_PREFERENCES_DIR);
    if !dir.is_dir() {
        // No profile has ever written here. The first one creates the folder, and activation
        // picks it up from there.
        log::debug!(target: "managed_policy", "{MANAGED_PREFERENCES_DIR} doesn't exist; not watching it");
        return;
    }
    let watcher = notify::recommended_watcher(|result: Result<notify::Event, notify::Error>| match result {
        Ok(_) => refresh_and_log("a managed-preferences file change"),
        Err(e) => log::warn!(target: "managed_policy", "Managed-preferences watch error: {e}"),
    });
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(e) => {
            log::warn!(target: "managed_policy", "Couldn't create the managed-preferences watcher: {e}");
            return;
        }
    };
    if let Err(e) = notify::Watcher::watch(&mut watcher, dir, notify::RecursiveMode::Recursive) {
        log::warn!(target: "managed_policy", "Couldn't watch {MANAGED_PREFERENCES_DIR}: {e}");
        return;
    }
    *WATCHER.get_or_init(|| Mutex::new(None)).lock_ignore_poison() = Some(watcher);
    log::debug!(target: "managed_policy", "Watching {MANAGED_PREFERENCES_DIR}");
}
