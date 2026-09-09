//! Restarting the Dock, so it re-reads the preferences we just changed.
//!
//! The Dock keeps `persistent-apps` in memory and only looks at the stored value when it starts,
//! so a write nobody follows with a restart is invisible until the next login. `launchd` keeps the
//! Dock alive, so asking it to quit is the whole restart: it comes back on its own, in under a
//! second, with the new array.

use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

const LOG_TARGET: &str = "dock";

/// The Dock's own bundle identifier.
const DOCK_BUNDLE_ID: &str = "com.apple.dock";

/// Why a restart didn't happen. Typed, because the caller has to tell someone whether their new
/// tile is there yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RestartError {
    /// No Dock process to ask. Unusual but not fatal: whatever starts one next reads the stored
    /// preferences anyway.
    NotRunning,
    /// The Dock was there and declined the quit request.
    Refused,
}

/// Asks the running Dock to quit; `launchd` restarts it and it re-reads its preferences.
///
/// `NSRunningApplication` rather than a `killall Dock` subprocess: it's the OS-native route, it
/// needs no process spawn, and it's what `dockutil` reaches for first.
pub(super) fn restart_dock() -> Result<(), RestartError> {
    let dock = running_dock().ok_or(RestartError::NotRunning)?;

    if dock.terminate() {
        log::info!(target: LOG_TARGET, "Asked the Dock to restart so it picks up the new tile");
        Ok(())
    } else {
        Err(RestartError::Refused)
    }
}

/// The running Dock, if there is one.
fn running_dock() -> Option<objc2::rc::Retained<NSRunningApplication>> {
    let id = NSString::from_str(DOCK_BUNDLE_ID);
    NSRunningApplication::runningApplicationsWithBundleIdentifier(&id)
        .iter()
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_running_dock_is_found_by_its_bundle_identifier() {
        // The lookup half of the restart, exercised without asking anything to quit: on a Mac with
        // a desktop session the Dock is always running. ❌ Never call `restart_dock` from a test;
        // it would restart the machine's real Dock.
        assert!(running_dock().is_some(), "every logged-in macOS session has a Dock");
    }

    #[test]
    fn the_dock_we_find_is_the_dock() {
        let dock = running_dock().expect("a running Dock");

        assert_eq!(
            dock.bundleIdentifier().map(|id| id.to_string()),
            Some(DOCK_BUNDLE_ID.to_string())
        );
    }
}
