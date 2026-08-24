//! Local external drive indexing entry point and classification.
//!
//! A plain local external drive (USB stick, SD card, extra internal disk, or a
//! mounted disk image) is the first volume that is BOTH mount-rooted (its index
//! `ROOT_ID` is `/Volumes/X`, not `/`) AND scanned/watched by the LOCAL guarded
//! walker + FSEvents pipeline. It differs from the two existing external kinds at enable
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
//! flag, network-fs flag), never a volume-id or path substring.

use std::path::PathBuf;

use crate::indexing::host::volumes::MountFacts;
use crate::indexing::lifecycle::state;
use tokio::time::Duration;

/// How long to wait for the mount's filesystem-type probe before treating the
/// volume as non-local. A local mount's `statfs` returns in microseconds; the cap
/// only bites on a hung network mount, which we then route to the SMB gate.
pub(in crate::indexing) const FS_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

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
/// never run the local guarded walker (a network `readdir` can hang, and the
/// index would be mis-scanned). Pure so the routing decision is unit-testable
/// without a `VolumeManager` or an `AppHandle`.
pub(in crate::indexing) fn routes_to_local_external(is_smb_session: bool, fs_is_network: bool) -> bool {
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

/// Resolve the volume and classify it by typed facts. The fs-type probe runs on
/// the blocking pool under a hard timeout so a hung network mount can never stall
/// the IPC thread; a timeout is treated as network → fall through (the SMB path
/// has its own gating). Needs no `AppHandle`, so the classification is testable
/// with a registered fake volume.
async fn classify(volume_id: &str) -> Classified {
    let volumes = crate::indexing::host::volumes::current();
    let Some(volume) = volumes.get(volume_id) else {
        // Not registered — nothing to resolve a mount root from. Let the SMB path
        // report the typed `NotRegistered` refusal.
        return Classified::FallThrough;
    };
    let mount_root = volume.root().to_path_buf();
    let is_smb_session = volume.smb_connection_state().is_some();

    let probe_root = mount_root.clone();
    let facts = match tokio::time::timeout(
        FS_PROBE_TIMEOUT,
        tokio::task::spawn_blocking(move || crate::indexing::host::volumes::current().mount_facts(&probe_root)),
    )
    .await
    {
        Ok(Ok(facts)) => facts,
        // Timeout or join error: a probe that won't return means a slow/hung
        // mount — treat it as network (and inode-trust is moot; we fall through).
        _ => MountFacts::UNPROBEABLE,
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
/// [`start_indexing_for_local_external_inner`](crate::indexing::lifecycle::state::start_indexing_for_local_external_inner)
/// and caps external-DB accumulation (retention). No connection gate and no typed
/// refusal — a local mount is already readable. A no-op ([`Started`](LocalExternalEnable::Started))
/// if the volume's index is already active. Errors (a plain string for the IPC
/// surface) only on an internal start failure (DB open, manager spawn).
pub(crate) async fn start_indexing_for_local_external(volume_id: String) -> Result<LocalExternalEnable, String> {
    // ❌ Not `is_active`: a volume with a teardown claimed on it is active right up
    // to the moment it stops, and this is the enable that has to bring it back.
    if state::is_active_and_staying(&volume_id) {
        log::info!("start_indexing_for_local_external: '{volume_id}' already active, no-op");
        return Ok(LocalExternalEnable::Started);
    }

    match classify(&volume_id).await {
        Classified::FallThrough => Ok(LocalExternalEnable::NotLocalExternal),
        Classified::LocalExternal {
            mount_root,
            inodes_trustworthy,
        } => {
            state::start_indexing_for_local_external_inner(&volume_id, mount_root, inodes_trustworthy)?;

            // A new external index DB just came online (or resumed): cap
            // accumulation by evicting the least-recently-used OFFLINE external
            // DBs. Safe — never touches a registered/live volume, and this one is
            // now registered. See `retention`.
            crate::indexing::resources::retention::enforce_external_index_cap();
            Ok(LocalExternalEnable::Started)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::indexing::host::volumes::{self, FakeVolumeProvider};
    use cmdr_fs::volume::InMemoryVolume;

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
        // SMB gate (the local guarded walker must never walk a network mount).
        assert!(!routes_to_local_external(true, false), "smb2 session => SMB gate");
        assert!(!routes_to_local_external(false, true), "network fs => SMB gate");
        assert!(!routes_to_local_external(true, true), "both => SMB gate");
    }

    #[tokio::test]
    #[allow(
        clippy::await_holding_lock,
        reason = "the lock serializes the process-wide provider slot for the whole test; holding it across the await IS the point"
    )]
    async fn classify_resolves_a_registered_local_volume_to_its_mount_root() {
        // A registered volume on a local mount is a local external drive rooted at
        // that path. Exercises the real wiring: registry lookup + mount probe + the
        // typed decision, with the host answering both.
        let mount = std::path::Path::new("/media/LocalExternalClassifyTest");
        let vid = "local-external-classify-test";
        let provider = FakeVolumeProvider::shared();
        provider.register(vid, Arc::new(InMemoryVolume::new("Test drive").with_root(mount)));

        let _serialized = crate::indexing::handle::test_lock();
        let _installed = volumes::install_for_test(provider);

        match classify(vid).await {
            Classified::LocalExternal {
                mount_root,
                inodes_trustworthy,
            } => {
                assert_eq!(mount_root, mount, "resolves to the registered mount root");
                // A local drive keeps stable inodes, so the rename pre-pass may
                // trust them (FAT/exFAT is the case that wouldn't).
                assert!(inodes_trustworthy, "a local mount has trustworthy inodes");
            }
            Classified::FallThrough => panic!("a local volume must classify as LocalExternal"),
        }
    }

    /// The other side of the same decision: a NETWORK mount must fall through to the
    /// SMB gate, because the local guarded walker must never walk a share.
    #[tokio::test]
    #[allow(
        clippy::await_holding_lock,
        reason = "the lock serializes the process-wide provider slot for the whole test; holding it across the await IS the point"
    )]
    async fn classify_falls_through_for_a_network_mount() {
        let mount = std::path::Path::new("/Volumes/LocalExternalNetworkTest");
        let vid = "local-external-network-test";
        let provider = FakeVolumeProvider::shared();
        provider
            .register(vid, Arc::new(InMemoryVolume::new("Share").with_root(mount)))
            .mark_network(mount);

        let _serialized = crate::indexing::handle::test_lock();
        let _installed = volumes::install_for_test(provider);

        assert!(
            matches!(classify(vid).await, Classified::FallThrough),
            "a network mount must fall through to the SMB gate",
        );
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
