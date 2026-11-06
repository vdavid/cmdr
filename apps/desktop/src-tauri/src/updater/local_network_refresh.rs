//! Keeps macOS's Local Network permission on the binary an in-place update just installed (#1019).
//!
//! Local Network privacy matches a process to its app's rule by main-executable UUID, and `nehelper`
//! fills that UUID list from LaunchServices when an "apps installed" notification arrives. The
//! updater's sync triggers that notification, but `nehelper` reads the user's LaunchServices
//! record before it catches up with the new binary, so the rule keeps the OLD UUID and the
//! relaunched app falls outside it: blocked until someone switches Cmdr off and on in System
//! Settings. Re-registering alone (`LSRegisterURL`) doesn't notify `nehelper`; touching the bundle
//! does. So on the first launch after an update: re-register (making the record current), then
//! touch (making `nehelper` re-read it). Evidence and the measurements behind each step:
//! `docs/notes/local-network-after-update-2026-11-06.md`.
//!
//! Done at launch rather than right after the sync because the still-running old process needs
//! the OLD UUID in the rule until it restarts. It also means the release that ships this fixes its
//! own update.

use core_foundation::base::TCFType;
use core_foundation::url::{CFURL, CFURLRef};
use std::path::{Path, PathBuf};

// SAFETY: the documented CoreServices signature, `OSStatus LSRegisterURL(CFURLRef inURL, Boolean
// inUpdate)`. `Boolean` is an unsigned char and `OSStatus` a 32-bit signed int. The URL is
// borrowed (Get rule): LaunchServices doesn't take ownership.
#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSRegisterURL(in_url: CFURLRef, in_update: u8) -> i32;
}

/// Re-announces the running bundle on its own 8 MB-stack thread when this is the first launch
/// after an update and the copy is an installed production one. Never blocks the caller:
/// LaunchServices answers over XPC.
pub(crate) fn spawn_after_update() {
    let Some(bundle) = eligible_bundle() else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("local-network-refresh".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || reannounce(&bundle));
    if let Err(e) = spawned {
        log::warn!("Couldn't start the Local Network refresh after the update: {e}");
    }
}

/// The bundle to re-announce, or `None` for a copy no update could have written into: a dev
/// build, a tooling instance, or a copy outside an Applications folder.
fn eligible_bundle() -> Option<PathBuf> {
    if let Some(reason) = super::skip_reason() {
        log::debug!("Not refreshing the Local Network registration: {reason}");
        return None;
    }
    if !crate::install_location::running_copy_is_installed() {
        log::debug!("Not refreshing the Local Network registration: this copy isn't in an Applications folder");
        return None;
    }
    super::installer::running_bundle().ok()
}

/// Why LaunchServices didn't take the re-registration.
#[derive(Debug)]
enum RegisterRefusal {
    /// The bundle path doesn't make a file URL.
    NoUrl,
    /// `LSRegisterURL` answered this `OSStatus`.
    Status(i32),
}

impl std::fmt::Display for RegisterRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoUrl => f.write_str("its path doesn't make a file URL"),
            Self::Status(status) => write!(f, "LSRegisterURL answered OSStatus {status}"),
        }
    }
}

fn reannounce(bundle: &Path) {
    match register(bundle) {
        Ok(()) => log::debug!("Re-registered the updated bundle with LaunchServices"),
        // Still touch: an older record can only get fresher, and the touch is what nehelper hears.
        Err(refusal) => log::warn!("LaunchServices didn't re-register the updated bundle: {refusal}"),
    }
    super::installer::touch_bundle(bundle);
    log::info!("Asked macOS to re-read Cmdr's Local Network rule for the updated binary");
}

fn register(bundle: &Path) -> Result<(), RegisterRefusal> {
    let url = CFURL::from_path(bundle, true).ok_or(RegisterRefusal::NoUrl)?;
    // SAFETY: `url` is a live CFURL for the call's duration, passed by its concrete ref; `1` asks
    // LaunchServices to update an existing registration (the `inUpdate` flag).
    let status = unsafe { LSRegisterURL(url.as_concrete_TypeRef(), 1) };
    if status == 0 {
        Ok(())
    } else {
        Err(RegisterRefusal::Status(status))
    }
}
