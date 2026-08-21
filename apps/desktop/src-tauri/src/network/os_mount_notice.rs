//! Deciding when to tell someone a share is stuck on the macOS kernel mount.
//!
//! A direct-connect failure leaves the share working at kernel-mount speed, which
//! nothing else in the app announces beyond a small yellow dot. The notice fills
//! that gap, both halves of it: WHETHER to speak (once per server per run) and
//! the `smb-fell-back-to-os-mount` event that carries the message.
//!
//! **Once per SERVER per run, not once per share.** The two auto-upgrade paths
//! (the startup pass over existing mounts, and the FSEvents mount watcher) call
//! `register_smb_volume` once per MOUNTED SHARE, and the thing that failed is a
//! property of the connection to the server: a stale password or a sleeping host
//! rejects every share on it identically. A NAS whose shares all remount at login
//! would otherwise raise one notice per share, which is worse than the silence it
//! replaces.

use crate::ignore_poison::IgnorePoison;
use crate::network::NetworkHost;
use crate::network::server_identity::same_server;
use std::sync::{LazyLock, Mutex, OnceLock};
use tauri::AppHandle;

/// The `AppHandle` [`announce_os_mount_fallback`] emits through, stashed once
/// from `lib.rs::setup`. The notice is the one thing this module says out loud,
/// and a share's session-state transitions no longer come this way: those go
/// through the volume host's event seam.
static APP_HANDLE: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

/// Stores the `AppHandle` so the fallback notice can reach the frontend.
pub fn set_app_handle(handle: AppHandle) {
    let storage = APP_HANDLE.get_or_init(|| Mutex::new(None));
    *storage.lock_ignore_poison() = Some(handle);
}

fn app_handle() -> Option<AppHandle> {
    APP_HANDLE.get().map(|m| m.lock_ignore_poison().clone())?
}

/// Tells the frontend a share is staying on the macOS kernel mount, so it can
/// offer a retry instead of leaving someone on the slow path with no explanation.
fn emit_fell_back_to_os_mount(volume_id: &str, share: &str) {
    use tauri_specta::Event;
    if let Some(app) = app_handle()
        && let Err(e) = (crate::network::SmbFellBackToOsMount {
            volume_id: volume_id.to_string(),
            share: share.to_string(),
        })
        .emit(&app)
    {
        log::warn!("Failed to emit smb-fell-back-to-os-mount: {}", e);
    }
}

/// The servers the user has already been told are on the slow path this run.
///
/// A `Vec` rather than a `HashSet` because membership is an identity question,
/// not a string question: `same_server` pairs the name forms one server arrives
/// under (`Naspolya._smb._tcp.local`, `naspolya.local`, `192.168.1.111`) using the
/// live mDNS state, so there's no single key to hash. The list holds one entry per
/// server that ever fell back in a session, so the scan is over a handful.
#[derive(Default)]
struct OsMountNotices {
    told: Vec<String>,
}

impl OsMountNotices {
    /// Records `server` as told, returning `true` only the first time. The caller
    /// speaks on `true` and stays quiet on `false`.
    fn claim(&mut self, server: &str, hosts: &[NetworkHost]) -> bool {
        if self.told.iter().any(|told| same_server(told, server, hosts)) {
            return false;
        }
        self.told.push(server.to_string());
        true
    }

    /// Forgets `server`, so a later fallback on it earns a fresh notice.
    fn forget(&mut self, server: &str, hosts: &[NetworkHost]) {
        self.told.retain(|told| !same_server(told, server, hosts));
    }
}

static OS_MOUNT_NOTICES: LazyLock<Mutex<OsMountNotices>> = LazyLock::new(Mutex::default);

/// Tells the frontend `share` is staying on the macOS kernel mount, at most once
/// per server per app run.
///
/// Only the auto-upgrade paths call this. The manual "Connect directly" flow
/// surfaces its own failure to the person who clicked it, so a notice there would
/// say the same thing twice.
pub(crate) fn announce_os_mount_fallback(server: &str, volume_id: &str, share: &str) {
    let hosts = crate::network::get_discovered_hosts();
    if !OS_MOUNT_NOTICES.lock_ignore_poison().claim(server, &hosts) {
        return;
    }
    log::debug!("Telling the frontend about the kernel-mount fallback on {server}/{share}");
    emit_fell_back_to_os_mount(volume_id, share);
}

/// Forgets `server` once a direct session lands on it.
///
/// A notice describes a situation, not an event, so once the server is off the
/// slow path the next genuine regression is worth saying out loud again.
pub(crate) fn clear_os_mount_notice(server: &str) {
    let hosts = crate::network::get_discovered_hosts();
    OS_MOUNT_NOTICES.lock_ignore_poison().forget(server, &hosts);
}

#[cfg(test)]
#[path = "os_mount_notice_test.rs"]
mod tests;
