//! The pane-open upgrade: a pane landing on an SMB share macOS mounted, and Cmdr
//! hasn't upgraded yet, tries the direct connection for that one share.
//!
//! The adopter pass (`file_system::upgrade_existing_smb_mounts`) covers the shares
//! mounted at launch and on each networking intent. This is the net under it: a
//! share Finder mounted later, one whose launch-time attempt failed (a sleeping NAS,
//! a Wi-Fi that wasn't up yet), or one that was handed back to the OS mount. Only
//! the share someone is looking at, ❌ never a sweep over every mount.
//!
//! Hooked into `commands::file_system::list_directory_start_streaming`, which only a
//! pane's listing loader calls, so every pane counts (issue #123). It runs on each
//! navigation, which is why the gates are ordered cheapest first and a share that
//! already has its session costs one prefix check and one registry read.

use crate::file_system::volume::BackendKind;
use crate::ignore_poison::IgnorePoison;
use crate::network::os_mount_notice::FallbackNotice;
#[cfg(target_os = "macos")]
use crate::volumes::SmbMountInfo;
#[cfg(target_os = "linux")]
use crate::volumes_linux::SmbMountInfo;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// How long after one pane-open attempt on a share the next navigation may try it
/// again.
///
/// Browsing a share lists a directory per step, and an unreachable server takes
/// the full connect timeout to say so; without this, every step would re-dial it.
/// Short enough that a NAS waking from sleep is picked up while the person is still
/// on it. A share whose attempt succeeded never gets this far again: it's served by
/// an `SmbVolume` from then on.
pub(crate) const RETRY_COOLDOWN: Duration = Duration::from_secs(60);

/// A share a pane landed on, and the mount it rides on: what the upgrade dials.
#[derive(Debug)]
pub(crate) struct PaneUpgrade {
    mount_path: String,
    info: SmbMountInfo,
}

/// When each share last got a pane-open attempt, by volume id.
///
/// Holds one entry per share a pane has landed on this run (a handful).
#[derive(Default)]
pub(crate) struct Cooldowns {
    last_try: HashMap<String, Instant>,
}

impl Cooldowns {
    /// Records an attempt on `volume_id` at `now`, unless the last one was within
    /// [`RETRY_COOLDOWN`]. Returns whether this one may go ahead.
    fn try_claim(&mut self, volume_id: &str, now: Instant) -> bool {
        if let Some(&last) = self.last_try.get(volume_id)
            && now.saturating_duration_since(last) < RETRY_COOLDOWN
        {
            return false;
        }
        self.last_try.insert(volume_id.to_string(), now);
        true
    }
}

static COOLDOWNS: LazyLock<Mutex<Cooldowns>> = LazyLock::new(Mutex::default);

/// What a pane landing on `volume_id` should upgrade, if anything. Pure: the
/// registry's answer comes in as `backend` and `root`, the mount table's through
/// `mount_at`, the per-share switch through `switched_on`.
///
/// Gates, cheapest first:
/// 1. The id names an SMB share (`VolumeScheme::Smb`), so a local navigation stops
///    here without reading the mount table.
/// 2. It's registered, and served by something other than an `SmbVolume`: that
///    combination is exactly "an OS mount Cmdr hasn't upgraded". A share with a
///    session, `Disconnected` included, owns its own recovery.
/// 3. The kernel still lists an SMB mount at the volume's root, and that mount
///    derives this very id. A share unmounted a moment ago has nothing to dial.
/// 4. The per-share "Use Cmdr's fast direct connection" switch is on. A cheap early
///    read only: `register_smb_volume` asks again at act time, under the upgrade
///    lock, with the resolved address as a second spelling. Before the cooldown, so
///    a switched-off share doesn't spend it.
/// 5. The share's cooldown has run out ([`RETRY_COOLDOWN`]).
fn plan(
    volume_id: &str,
    backend: Option<BackendKind>,
    root: Option<&Path>,
    mount_at: impl FnOnce(&Path) -> Option<SmbMountInfo>,
    switched_on: impl FnOnce(&SmbMountInfo) -> bool,
    cooldowns: &mut Cooldowns,
    now: Instant,
) -> Option<PaneUpgrade> {
    if cmdr_fs::volume::VolumeScheme::of(volume_id) != cmdr_fs::volume::VolumeScheme::Smb {
        return None;
    }
    let root = match (backend, root) {
        (Some(backend), Some(root)) if backend != BackendKind::Smb => root,
        _ => return None,
    };
    let info = mount_at(root)?;
    if crate::file_system::volume::smb_volume_id(&info.server, info.port, &info.share) != volume_id {
        return None;
    }
    if !switched_on(&info) {
        log::debug!("A pane landed on {volume_id}, which is set to stay on the macOS mount; not upgrading it");
        return None;
    }
    if !cooldowns.try_claim(volume_id, now) {
        return None;
    }
    Some(PaneUpgrade {
        mount_path: root.to_string_lossy().into_owned(),
        info,
    })
}

/// Tries the direct connection for the share a pane just landed on, in the
/// background. Returns at once; a no-op for anything but an OS-mounted SMB share
/// Cmdr hasn't upgraded yet (see [`plan`]).
///
/// Dials through `resolve_and_register_smb_volume`, the same funnel as the other
/// auto paths, so the per-share switch, the per-volume upgrade lock, and the
/// `is_already_direct` re-check all hold here too. `FallbackNotice::Announce`: the
/// person is looking at this share, so a failure is news they can act on (once per
/// server per run).
pub(crate) fn upgrade_on_pane_open(volume_id: &str) {
    if !crate::file_system::is_direct_smb_enabled()
        || cmdr_fs::volume::VolumeScheme::of(volume_id) != cmdr_fs::volume::VolumeScheme::Smb
    {
        return;
    }
    let volume = crate::file_system::volume::manager::get_volume_manager().get(volume_id);
    let upgrade = plan(
        volume_id,
        volume.as_ref().map(|v| v.backend_kind()),
        volume.as_ref().map(|v| v.root()),
        smb_mount_at,
        |info| crate::network::known_shares::direct_connection_enabled(&[&info.server], &info.share),
        &mut COOLDOWNS.lock_ignore_poison(),
        Instant::now(),
    );
    let Some(upgrade) = upgrade else {
        return;
    };
    log::debug!(
        "A pane landed on {volume_id}, which rides the OS mount at {}; trying the direct connection",
        upgrade.mount_path
    );
    // The dial holds the mDNS browse for its own resolution (`discover_server`).
    tauri::async_runtime::spawn(dial(upgrade));
}

/// Dials `upgrade` through the shared auto-upgrade funnel.
async fn dial(upgrade: PaneUpgrade) {
    crate::network::smb_upgrade::resolve_and_register_smb_volume(
        &upgrade.info.server,
        &upgrade.info.share,
        &upgrade.mount_path,
        upgrade.info.port,
        FallbackNotice::Announce,
    )
    .await;
}

/// The SMB mount the kernel lists exactly at `root`, from the non-blocking mount
/// table: a pane on a hung share can't stall here.
fn smb_mount_at(root: &Path) -> Option<SmbMountInfo> {
    #[cfg(target_os = "macos")]
    use crate::volumes::smb_mounts;
    #[cfg(target_os = "linux")]
    use crate::volumes_linux::smb_mounts;

    smb_mounts()?
        .into_iter()
        .find(|(mount_point, _)| Path::new(mount_point) == root)
        .map(|(_, info)| info)
}

#[cfg(test)]
#[path = "smb_pane_upgrade_test.rs"]
mod tests;
