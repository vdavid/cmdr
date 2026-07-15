//! Local external drive indexing entry point and classification.
//!
//! A plain local external drive (USB stick, SD card, extra internal disk, or a
//! mounted disk image) is the first volume that is BOTH mount-rooted (its index
//! `ROOT_ID` is `/Volumes/X`, not `/`) AND scanned/watched by the LOCAL jwalk +
//! FSEvents pipeline. It differs from the two existing external kinds at enable
//! time:
//!
//! - **No connection gate** (unlike SMB). A local mount is already directly
//!   readable, so there's nothing to upgrade and no typed refusal to surface —
//!   enable just needs the volume registered and classified as local.
//! - **Uses the local scanner** (unlike MTP, which walks the `Volume` trait).
//!
//! The enable site (`commands/indexing.rs`) has only a volume id, so we classify
//! here: resolve the registered volume, read its mount root, and probe the
//! mount's filesystem type (timeout-guarded — a hung network mount must never
//! block the IPC thread, per `src-tauri/CLAUDE.md`). A volume that carries a live
//! smb2 session OR whose mount is a network filesystem (SMB os-mount, NFS, AFP,
//! ...) is NOT a local external drive; the caller falls through to the SMB gate.
//! Everything else — every local filesystem, disk images INCLUDED (plan
//! Decision 1) — indexes here. Classification is by typed facts (smb-session
//! flag, network-fs flag), never a volume-id or path substring
//! (`.claude/rules/no-string-matching.md`).

use std::path::PathBuf;

use tauri::AppHandle;
use tokio::time::Duration;

/// How long to wait for the mount's filesystem-type probe before treating the
/// volume as non-local. A local mount's `statfs` returns in microseconds; the cap
/// only bites on a hung network mount, which we then route to the SMB gate.
const FS_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// The outcome of routing a per-drive enable through the local-external branch.
pub(crate) enum LocalExternalEnable {
    /// The volume was a local external drive; its index is now active (scanning
    /// or resuming).
    Started,
    /// Not a local external drive (network mount, live smb2 session, or an
    /// unresolved id). The caller routes it to the SMB gate.
    NotLocalExternal,
}

/// Whether a registered, non-root, non-MTP volume is a plain local external
/// drive (index via the local scanner) rather than something that must fall
/// through to the SMB gate.
///
/// A volume falls through when it carries a live smb2 session OR its mount
/// filesystem is a network type (SMB os-mount, NFS, AFP, WebDAV, ...): those must
/// never run the local jwalk scanner (a network `readdir` can hang, and the
/// index would be mis-scanned). Pure so the routing decision is unit-testable
/// without a `VolumeManager` or an `AppHandle`.
fn routes_to_local_external(is_smb_session: bool, fs_is_network: bool) -> bool {
    !(is_smb_session || fs_is_network)
}

/// The result of classifying an enable target by its typed volume facts.
enum Classified {
    /// A local external drive rooted at this mount point. `inodes_trustworthy` is
    /// resolved from the mount's `FilesystemKind` (`false` for FAT/exFAT, whose
    /// derived inodes must not drive the rename pre-pass) and threaded to the scan.
    LocalExternal {
        mount_root: PathBuf,
        inodes_trustworthy: bool,
    },
    /// Route to the SMB gate (network mount, smb2 session, or unresolved).
    FallThrough,
}

/// The two typed filesystem facts the enable decision needs, read from ONE
/// `detect_filesystem_for_path` probe (a `statfs` on macOS, `/proc/mounts` on
/// Linux): whether the mount is a network type (must never be walked by the local
/// scanner) and whether its inode identity is trustworthy (FAT/exFAT is not).
/// Blocking — call only inside the timeout guard.
fn probe_fs_facts(path: &std::path::Path) -> FsFacts {
    let info = crate::file_system::filesystem_kind::detect_filesystem_for_path(path);
    #[cfg(target_os = "macos")]
    let is_network = crate::volumes::is_network_fs_type(info.raw_type.as_deref());
    #[cfg(target_os = "linux")]
    let is_network = info
        .raw_type
        .as_deref()
        .map(crate::file_system::linux_mounts::is_network_fs_type)
        .unwrap_or(false);
    FsFacts {
        is_network,
        inodes_trustworthy: info.kind.has_stable_inodes(),
    }
}

/// Typed filesystem facts for the enable decision (see [`probe_fs_facts`]).
struct FsFacts {
    is_network: bool,
    inodes_trustworthy: bool,
}

/// Resolve the volume and classify it by typed facts. The fs-type probe runs on
/// the blocking pool under a hard timeout so a hung network mount can never stall
/// the IPC thread; a timeout is treated as network → fall through (the SMB path
/// has its own gating). Needs no `AppHandle`, so the classification is testable
/// with a registered fake volume.
async fn classify(volume_id: &str) -> Classified {
    let Some(volume) = crate::file_system::get_volume_manager().get(volume_id) else {
        // Not registered — nothing to resolve a mount root from. Let the SMB path
        // report the typed `NotRegistered` refusal.
        return Classified::FallThrough;
    };
    let mount_root = volume.root().to_path_buf();
    let is_smb_session = volume.smb_connection_state().is_some();

    let probe_root = mount_root.clone();
    let facts = match tokio::time::timeout(
        FS_PROBE_TIMEOUT,
        tokio::task::spawn_blocking(move || probe_fs_facts(&probe_root)),
    )
    .await
    {
        Ok(Ok(facts)) => facts,
        // Timeout or join error: a probe that won't return means a slow/hung
        // mount — treat it as network (and inode-trust is moot; we fall through).
        _ => FsFacts {
            is_network: true,
            inodes_trustworthy: true,
        },
    };

    if routes_to_local_external(is_smb_session, facts.is_network) {
        Classified::LocalExternal {
            mount_root,
            inodes_trustworthy: facts.inodes_trustworthy,
        }
    } else {
        Classified::FallThrough
    }
}

/// Turn on indexing for a local external drive (the per-drive "Turn on indexing"
/// action, routed here by `commands/indexing.rs` for a non-root, non-MTP id).
///
/// Classifies the volume (see [`classify`]); a non-local one returns
/// [`LocalExternalEnable::NotLocalExternal`] so the caller falls through to the
/// SMB gate. A local one starts the mount-rooted local scan/watch pipeline via
/// [`start_indexing_for_local_external_inner`](super::state::start_indexing_for_local_external_inner)
/// and caps external-DB accumulation (retention). No connection gate and no typed
/// refusal — a local mount is already readable. A no-op ([`Started`](LocalExternalEnable::Started))
/// if the volume's index is already active. Errors (a plain string for the IPC
/// surface) only on an internal start failure (DB open, manager spawn).
pub(crate) async fn start_indexing_for_local_external(
    app: AppHandle,
    volume_id: String,
) -> Result<LocalExternalEnable, String> {
    if super::state::is_active(&volume_id) {
        log::info!("start_indexing_for_local_external: '{volume_id}' already active, no-op");
        return Ok(LocalExternalEnable::Started);
    }

    match classify(&volume_id).await {
        Classified::FallThrough => Ok(LocalExternalEnable::NotLocalExternal),
        Classified::LocalExternal {
            mount_root,
            inodes_trustworthy,
        } => {
            super::state::start_indexing_for_local_external_inner(&app, &volume_id, mount_root, inodes_trustworthy)?;

            // A new external index DB just came online (or resumed): cap
            // accumulation by evicting the least-recently-used OFFLINE external
            // DBs. Safe — never touches a registered/live volume, and this one is
            // now registered. See `retention`.
            super::retention::enforce_external_index_cap(&app);
            Ok(LocalExternalEnable::Started)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::LocalPosixVolume;

    #[test]
    fn a_plain_local_drive_routes_to_the_local_external_scanner_not_smb() {
        // The bug this milestone fixes: a healthy local external drive (no smb2
        // session, a local filesystem) used to fall through to the SMB gate and
        // be refused as `NotAnSmbVolume`. It must route to the local-external
        // scanner instead.
        assert!(
            routes_to_local_external(false, false),
            "no smb2 session + local fs => local external drive",
        );
        // A live smb2 session or a network filesystem must fall through to the
        // SMB gate (the local jwalk scanner must never walk a network mount).
        assert!(!routes_to_local_external(true, false), "smb2 session => SMB gate");
        assert!(!routes_to_local_external(false, true), "network fs => SMB gate");
        assert!(!routes_to_local_external(true, true), "both => SMB gate");
    }

    #[tokio::test]
    async fn classify_resolves_a_registered_local_volume_to_its_mount_root() {
        // A registered LocalPosixVolume on a local temp dir (APFS on macOS) is a
        // local external drive rooted at that path. This exercises the real
        // wiring: VolumeManager lookup + fs-type probe + the typed decision.
        let dir = tempfile::tempdir().expect("temp dir");
        let vid = "local-external-classify-test";
        get_volume_manager().register(vid, Arc::new(LocalPosixVolume::new("Test drive", dir.path())));

        match classify(vid).await {
            Classified::LocalExternal {
                mount_root,
                inodes_trustworthy,
            } => {
                assert_eq!(mount_root, dir.path(), "resolves to the registered mount root");
                // A temp dir sits on APFS (macOS) / ext4 or tmpfs (Linux), all of
                // which keep stable inodes, so the drive is inode-trustworthy.
                assert!(inodes_trustworthy, "a local temp dir has trustworthy inodes");
            }
            Classified::FallThrough => panic!("a local temp-dir volume must classify as LocalExternal"),
        }

        get_volume_manager().unregister(vid);
    }

    #[tokio::test]
    async fn classify_falls_through_for_an_unregistered_volume() {
        // No registration => no mount root to resolve => the SMB path handles it
        // (and reports the typed `NotRegistered`).
        assert!(
            matches!(
                classify("local-external-never-registered").await,
                Classified::FallThrough
            ),
            "an unregistered id must fall through to the SMB gate",
        );
    }
}
