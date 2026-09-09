//! Putting Cmdr in the macOS Dock.
//!
//! Three questions and one action, all typed: may we offer the pin, is Cmdr already down there,
//! and put it there. The nudge that asks them lives in the frontend; this module holds every
//! decision that needs to look at the machine.
//!
//! ❌ **Never write to `com.apple.dock` from a test.** A test run would rearrange the machine's
//! real Dock. Reading it is fine; the write path is exercised against a scratch domain
//! (`prefs.rs`), and the real thing is a manual check (`DETAILS.md`).

mod entries;
pub mod menu;
mod prefs;
mod restart;

use serde::Serialize;
use std::path::PathBuf;

const LOG_TARGET: &str = "dock";

/// The Dock's preferences domain.
const DOCK_DOMAIN: &str = "com.apple.dock";

/// Whether we may offer to put Cmdr in the Dock.
///
/// One state rather than two booleans: "already pinned" and "can't pin" are different answers and
/// the caller shouldn't be able to hold both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DockPinState {
    /// Cmdr is installed where a tile can point at it, and no Cmdr tile is there yet.
    Offerable,
    /// A Cmdr tile is already in the Dock. Nothing to offer.
    AlreadyPinned,
    /// We're staying quiet, for `reason`.
    Unavailable { reason: DockPinBlocker },
}

/// Why we won't offer, or why a pin can't go ahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DockPinBlocker {
    /// Cmdr isn't running from a `.app` at all: a dev build out of `target/`, which conveniently
    /// keeps the whole feature out of development.
    NotABundle,
    /// The bundle is somewhere a Dock tile shouldn't point: `~/Downloads`, a mounted disk image,
    /// or a Gatekeeper-translocated copy. A tile there dies as soon as the copy moves.
    OutsideApplications,
    /// A configuration profile manages the Dock, so a write would be silently swallowed.
    ManagedDock,
    /// The Dock's `persistent-apps` couldn't be read, so we don't know what's down there.
    PreferencesUnreadable,
}

/// Why a pin didn't happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DockPinFailure {
    /// The pin was asked for in a situation where we'd never have offered it.
    Blocked { reason: DockPinBlocker },
    /// The new tile list didn't reach `cfprefsd`, so nothing changed.
    WriteRejected,
    /// The tile is stored, but the Dock didn't restart, so it won't show up until it next does.
    DockNotRestarted,
    /// The whole thing ran past its deadline, so nobody knows whether the tile landed. Minted by
    /// the IPC command rather than by [`pin_cmdr`], which has no deadline of its own.
    TimedOut,
}

/// Whether we may offer to put Cmdr in the Dock right now.
pub fn pin_state() -> DockPinState {
    let bundle_path = match installed_bundle() {
        Ok(path) => path,
        Err(reason) => return DockPinState::Unavailable { reason },
    };

    if prefs::is_forced(DOCK_DOMAIN, entries::PERSISTENT_APPS_KEY) {
        return DockPinState::Unavailable {
            reason: DockPinBlocker::ManagedDock,
        };
    }

    let Ok(tiles) = prefs::read_array(DOCK_DOMAIN, entries::PERSISTENT_APPS_KEY) else {
        return DockPinState::Unavailable {
            reason: DockPinBlocker::PreferencesUnreadable,
        };
    };

    if entries::holds_app(&tiles, bundle_id().as_deref(), &bundle_path) {
        DockPinState::AlreadyPinned
    } else {
        DockPinState::Offerable
    }
}

/// Adds Cmdr as the first tile in the Dock and restarts the Dock so it shows up.
///
/// First, so Cmdr lands immediately to the right of Finder: Finder's tile is drawn by the Dock
/// itself and isn't in `persistent-apps`, which makes index 0 the leftmost slot anyone can reach.
///
/// Already-pinned is success, not an error: the end state the caller asked for holds either way,
/// and a Dock restart nobody needed would be a rude thing to do about it.
pub fn pin_cmdr() -> Result<(), DockPinFailure> {
    let bundle_path = installed_bundle().map_err(|reason| DockPinFailure::Blocked { reason })?;

    if prefs::is_forced(DOCK_DOMAIN, entries::PERSISTENT_APPS_KEY) {
        return Err(DockPinFailure::Blocked {
            reason: DockPinBlocker::ManagedDock,
        });
    }

    let tiles = prefs::read_array(DOCK_DOMAIN, entries::PERSISTENT_APPS_KEY).map_err(|e| {
        log::warn!(target: LOG_TARGET, "Couldn't read the Dock's pinned apps: {e:?}");
        DockPinFailure::Blocked {
            reason: DockPinBlocker::PreferencesUnreadable,
        }
    })?;

    let id = bundle_id();
    if entries::holds_app(&tiles, id.as_deref(), &bundle_path) {
        log::info!(target: LOG_TARGET, "Cmdr is already in the Dock; nothing to add");
        return Ok(());
    }

    let tile = entries::app_tile(&bundle_path, id.as_deref(), fresh_guid());
    prefs::write_array(
        DOCK_DOMAIN,
        entries::PERSISTENT_APPS_KEY,
        &entries::with_tile_first(&tiles, tile),
    )
    .map_err(|e| {
        log::warn!(target: LOG_TARGET, "Couldn't store the new Dock tile: {e:?}");
        DockPinFailure::WriteRejected
    })?;

    restart::restart_dock().map_err(|e| {
        log::warn!(target: LOG_TARGET, "Stored the Dock tile but couldn't restart the Dock: {e:?}");
        DockPinFailure::DockNotRestarted
    })
}

/// The running app's bundle, if it's somewhere we're willing to pin from.
fn installed_bundle() -> Result<PathBuf, DockPinBlocker> {
    let bundle_path = crate::updater::installer::running_bundle().map_err(|_| DockPinBlocker::NotABundle)?;

    if crate::install_location::in_an_applications_folder(&bundle_path, dirs::home_dir().as_deref()) {
        Ok(bundle_path)
    } else {
        Err(DockPinBlocker::OutsideApplications)
    }
}

/// The running app's own bundle identifier, straight from its `Info.plist`.
///
/// Asked of `NSBundle` rather than kept as a constant, so the tile we write always names the app
/// that actually wrote it. A dev build has no bundle and answers `None`, which the path fallback in
/// `entries::holds_app` covers.
fn bundle_id() -> Option<String> {
    objc2_foundation::NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|id| id.to_string())
}

/// A fresh tile identity. The range matches the ten-digit GUIDs a live Dock carries.
fn fresh_guid() -> i64 {
    use rand::RngExt;
    rand::rng().random_range(1_000_000_000..10_000_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dev_build_is_never_offered_the_pin() {
        // The test binary runs out of `target/`, with no `.app` ancestor, so the whole feature is
        // off in development without anything having to remember to switch it off.
        assert_eq!(
            pin_state(),
            DockPinState::Unavailable {
                reason: DockPinBlocker::NotABundle
            }
        );
    }

    #[test]
    fn a_dev_build_refuses_to_pin() {
        assert_eq!(
            pin_cmdr(),
            Err(DockPinFailure::Blocked {
                reason: DockPinBlocker::NotABundle
            }),
            "the pin must refuse where the offer would never have been made"
        );
    }

    #[test]
    fn a_fresh_guid_is_ten_digits_like_the_ones_a_live_dock_carries() {
        for _ in 0..100 {
            let guid = fresh_guid();
            assert!((1_000_000_000..10_000_000_000).contains(&guid), "got {guid}");
        }
    }

    #[test]
    fn two_tiles_never_share_a_guid() {
        let first = fresh_guid();
        let second = fresh_guid();

        assert_ne!(first, second, "a collision here would be a 1-in-9-billion fluke");
    }
}
