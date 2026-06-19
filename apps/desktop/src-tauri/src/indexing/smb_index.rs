//! SMB-volume indexing entry point and the `direct`-connection gate.
//!
//! Indexing an SMB share requires Cmdr's own smb2 (`direct`) session, not the
//! macOS `os_mount` (plan rabbit hole #13): `CHANGE_NOTIFY` watching runs over
//! smb2 anyway, and smb2 parallelizes listing far better than per-`readdir`
//! round-trips through the kernel mount. So `start_indexing_for_smb` gates on
//! the volume being a live `SmbVolume` in `Direct` state; an `os_mount` share
//! (registered as a `LocalPosixVolume` on an `smbfs` mount, no smb2 session) is
//! upgraded first via the existing `upgrade_to_smb_volume` path, and if that
//! upgrade can't complete, indexing stays disabled with a TYPED reason (no
//! string-matching, per `.claude/rules/no-string-matching.md`).
//!
//! The FDA gate does NOT apply here (rabbit hole #12): network paths aren't
//! TCC-protected, so per-volume SMB enable is FDA-independent and never routes
//! through `should_auto_start_indexing`.

use std::path::PathBuf;

use tauri::AppHandle;

use crate::file_system::get_volume_manager;
use crate::file_system::volume::SmbConnectionState;

/// Why an SMB volume couldn't be indexed. Typed (and serialized as a
/// snake_case tag) so callers and the M3 UX classify by variant on BOTH sides
/// of the IPC boundary, never by message substring (`.claude/rules/no-string-matching.md`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SmbIndexGateReason {
    /// No volume is registered for this id (unmounted, or never seen).
    NotRegistered,
    /// The volume isn't an SMB share at all (no `smb_connection_state`).
    NotAnSmbVolume,
    /// The share is OS-mounted but the upgrade to a direct smb2 session failed
    /// (network unreachable, server refused). Indexing stays disabled.
    UpgradeFailed,
    /// The upgrade needs credentials Cmdr doesn't have cached. The user must
    /// sign in (the FE reconnect/credentials flow) before indexing can start.
    CredentialsNeeded,
    /// The volume's smb2 session is currently `Disconnected`. Reconnect first.
    Disconnected,
}

impl std::fmt::Display for SmbIndexGateReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Diagnostic / log text only. Classification is by variant, never by
        // parsing this string.
        let s = match self {
            Self::NotRegistered => "no volume registered for this id",
            Self::NotAnSmbVolume => "not an SMB volume",
            Self::UpgradeFailed => "upgrade to a direct smb2 connection failed",
            Self::CredentialsNeeded => "a direct smb2 connection needs credentials",
            Self::Disconnected => "the smb2 session is disconnected",
        };
        f.write_str(s)
    }
}

/// Map an SMB mount path to its index volume id, if the path is on an SMB mount.
///
/// Returns `Some(smb_volume_id(server, port, share))` when `path` resolves to an
/// `smbfs`/`cifs` mount, else `None`. Keyed by `(server, port, share)` (via
/// `smb_volume_id`), the SAME id the `VolumeManager` registers the share under,
/// so a listing under `/Volumes/<share>` resolves to the SMB volume's index, not
/// `root`. Platform-split because the mount-info probe lives in the macOS-only
/// `volumes` / Linux-only `volumes_linux` module.
pub(crate) fn smb_volume_id_for_path(path: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    use crate::volumes::get_smb_mount_info;
    #[cfg(target_os = "linux")]
    use crate::volumes_linux::get_smb_mount_info;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let info = get_smb_mount_info(path)?;
        Some(crate::file_system::volume::smb_volume_id(
            &info.server,
            info.port,
            &info.share,
        ))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        None
    }
}

/// Whether the volume registered under `volume_id` is a live, direct (smb2)
/// SMB volume ready to index right now. Pure inspection — no upgrade attempt.
fn is_direct_smb(volume_id: &str) -> bool {
    get_volume_manager()
        .get(volume_id)
        .and_then(|v| v.smb_connection_state())
        .is_some_and(|s| s == SmbConnectionState::Direct)
}

/// Ensure the SMB volume is in the `Direct` (smb2) state, upgrading from
/// `os_mount` if needed. Returns the volume's mount-path root on success, or a
/// typed gate reason.
///
/// Mirrors the FE "Turn on indexing" intent: an `os_mount` share triggers/awaits
/// `upgrade_to_smb_volume_inner`; a failed/credential-needing upgrade keeps
/// indexing disabled with a typed reason.
async fn ensure_direct_smb(volume_id: &str) -> Result<PathBuf, SmbIndexGateReason> {
    let manager = get_volume_manager();

    let volume = manager.get(volume_id).ok_or(SmbIndexGateReason::NotRegistered)?;
    match volume.smb_connection_state() {
        // Already a direct smb2 session: ready to index.
        Some(SmbConnectionState::Direct) => return Ok(volume.root().to_path_buf()),
        // A live SmbVolume whose session dropped. Don't silently index a stale
        // session; the FE reconnect flow owns recovery.
        Some(SmbConnectionState::Disconnected) => return Err(SmbIndexGateReason::Disconnected),
        // os_mount: a LocalPosixVolume on an smbfs mount. Fall through to upgrade.
        Some(SmbConnectionState::OsMount) | None => {}
    }

    // The `None` case is the os_mount one in practice (LocalPosixVolume on an
    // smbfs mount returns `None` from `smb_connection_state`). But a `None` that
    // ISN'T an smbfs mount is a non-SMB volume — reject it rather than trying to
    // upgrade a local disk.
    if volume.smb_connection_state().is_none() && smb_volume_id_for_path(&volume.root().to_string_lossy()).is_none() {
        return Err(SmbIndexGateReason::NotAnSmbVolume);
    }

    // os_mount → trigger/await the upgrade to a direct smb2 session.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use crate::network::smb_upgrade::UpgradeResult;
        match crate::commands::network::upgrade_to_smb_volume_inner(volume_id.to_string()).await {
            Ok(UpgradeResult::Success) => {}
            Ok(UpgradeResult::CredentialsNeeded { .. }) => return Err(SmbIndexGateReason::CredentialsNeeded),
            Ok(UpgradeResult::NetworkError { message }) => {
                log::warn!("SMB index gate: upgrade network error for '{volume_id}': {message}");
                return Err(SmbIndexGateReason::UpgradeFailed);
            }
            Err(e) => {
                log::warn!("SMB index gate: upgrade failed for '{volume_id}': {e}");
                return Err(SmbIndexGateReason::UpgradeFailed);
            }
        }
    }

    // Re-fetch: the upgrade replaced the LocalPosixVolume with an SmbVolume.
    if is_direct_smb(volume_id) {
        let volume = manager.get(volume_id).ok_or(SmbIndexGateReason::NotRegistered)?;
        Ok(volume.root().to_path_buf())
    } else {
        Err(SmbIndexGateReason::UpgradeFailed)
    }
}

/// Turn on indexing for an SMB volume (M2's per-volume enable).
///
/// Gates on a direct smb2 connection (upgrading from os_mount if needed), then
/// starts a `Volume`-trait scan into the volume's own index DB. FDA-independent
/// by design (network paths aren't TCC-protected). Returns the typed gate reason
/// on refusal so the caller (and M3's UX) can show an honest, non-string-matched
/// status. A no-op if the volume's index is already active.
pub async fn start_indexing_for_smb(app: AppHandle, volume_id: String) -> Result<(), SmbIndexGateReason> {
    if super::state::is_active(&volume_id) {
        log::info!("start_indexing_for_smb: '{volume_id}' already active, no-op");
        return Ok(());
    }

    let mount_root = ensure_direct_smb(&volume_id).await?;

    // The direct gate passed: start the per-volume index over the Volume trait.
    // `start_indexing_for` handles the lock-first reservation, load-as-Stale
    // freshness seeding, and SMB scan-path selection.
    if let Err(e) = super::state::start_indexing_for_smb_inner(&app, &volume_id, mount_root) {
        log::warn!("start_indexing_for_smb: start failed for '{volume_id}': {e}");
        // A start failure here isn't a gate reason — it's an internal error
        // (DB open, manager spawn). Treat as UpgradeFailed for the caller's
        // typed surface; the log carries the detail.
        return Err(SmbIndexGateReason::UpgradeFailed);
    }

    // A new external index DB just came online (or resumed): cap accumulation by
    // evicting the least-recently-used OFFLINE external DBs. Safe — never touches
    // a registered/live volume, and this one is now registered. See `retention`.
    super::retention::enforce_external_index_cap(&app);
    Ok(())
}

/// Record that an SMB volume's live watcher died (session drop, disconnect, or
/// the watcher task returning on a fatal `next_events` error). Flips a Fresh
/// index to Stale via the freshness state machine.
///
/// This is the M2-B `WatcherDied` call site (the seam M2-A declared). A reconnect
/// respawns the watcher, but continuity already broke (events were lost while
/// disconnected), so the index stays Stale until the user rescans — the model's
/// "Stale ⇒ Fresh only via rescan" rule. No-op for an unindexed volume.
///
/// Deliberately NOT fired on a clean watcher cancel (volume unmount / deliberate
/// stop): that's a teardown, not a continuity break to surface as Stale.
pub(crate) fn on_smb_watcher_died(volume_id: &str) {
    super::state::apply_freshness_event(volume_id, super::freshness::FreshnessEvent::WatcherDied);
}

/// Record a `CHANGE_NOTIFY` overflow (`STATUS_NOTIFY_ENUM_DIR`) on an SMB volume.
///
/// Policy (M2-B): overflow means the server dropped change records we can't
/// recover, so the index may have drifted. The watcher only ever signals
/// overflow for the share ROOT (it emits a root-scoped `FullRefresh`), so the
/// only honest repair is a full rescan — there's no narrower subtree to target.
/// We therefore fire `OverflowUnrecoverable` ⇒ Stale, surfacing the one-click
/// rescan affordance, rather than silently serving a possibly-drifted index as
/// Fresh. A future optimization (deferred, see DETAILS): if the watcher ever
/// scopes overflow to a subtree, rescan just that subtree and keep Fresh.
///
/// Distinct from `on_smb_watcher_died`: overflow keeps the watcher alive (the
/// session is fine), so it's a different code path, never conflated with a
/// disconnect. No-op for an unindexed volume.
pub(crate) fn on_smb_overflow(volume_id: &str) {
    super::state::apply_freshness_event(volume_id, super::freshness::FreshnessEvent::OverflowUnrecoverable);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_reason_variants_are_distinct() {
        // Classification is by variant: every reason must be its own value so a
        // `matches!` / `==` check never conflates two. (Guards against an
        // accidental same-variant alias during edits.)
        let all = [
            SmbIndexGateReason::NotRegistered,
            SmbIndexGateReason::NotAnSmbVolume,
            SmbIndexGateReason::UpgradeFailed,
            SmbIndexGateReason::CredentialsNeeded,
            SmbIndexGateReason::Disconnected,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                assert_eq!(i == j, a == b, "{a:?} vs {b:?} equality must track identity");
            }
        }
    }

    #[test]
    fn smb_volume_id_for_non_smb_path_is_none() {
        // A plain local path is never on an SMB mount, so it maps to no SMB
        // volume id (the caller then treats it as the local `root` index). This
        // is the boundary that keeps `/Users/...` resolving to `root`, not an
        // SMB volume.
        assert!(smb_volume_id_for_path("/Users/someone/Documents").is_none());
        assert!(smb_volume_id_for_path("/").is_none());
    }

    // ── Freshness call sites (the M2-B seam) ──────────────────────────────

    use std::sync::Arc;

    use crate::indexing::enrichment::{ReadPool, uninstall_read_pool};
    use crate::indexing::freshness::Freshness;
    use crate::indexing::pending_sizes::{PendingSizes, uninstall_pending_sizes};
    use crate::indexing::state::{INDEX_REGISTRY, get_freshness, try_reserve_initializing_phase};
    use crate::indexing::store::IndexStore;

    /// Reserve a volume's registry instance at a given freshness, run the test
    /// body, then remove it. Keeps these freshness-seam tests independent of the
    /// registry's other tests. Freshness lives on the instance, so removing it
    /// (plus uninstalling the read-path handles) fully resets the volume.
    fn with_reserved_volume(vid: &str, initial: Freshness, body: impl FnOnce()) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join(format!("{vid}.db"));
        let store = IndexStore::open(&db_path).expect("open store");
        let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
        let pending = Arc::new(PendingSizes::new());
        INDEX_REGISTRY.lock().expect("registry").remove(vid);
        assert!(
            try_reserve_initializing_phase(vid, store, pool, pending, Some(initial)).is_ok(),
            "reserve must succeed",
        );
        body();
        INDEX_REGISTRY.lock().expect("registry").remove(vid);
        uninstall_read_pool(vid);
        uninstall_pending_sizes(vid);
    }

    #[test]
    fn watcher_died_flips_a_fresh_smb_volume_to_stale() {
        // The headline M2-B transition wired at the call site: when the live SMB
        // watcher dies, `on_smb_watcher_died` must drive the index Fresh ⇒ Stale.
        with_reserved_volume("smb-watcher-died-test", Freshness::Fresh, || {
            on_smb_watcher_died("smb-watcher-died-test");
            assert_eq!(
                get_freshness("smb-watcher-died-test"),
                Some(Freshness::Stale),
                "a dead watcher must mark a Fresh index Stale",
            );
        });
    }

    #[test]
    fn watcher_died_is_a_noop_for_an_unindexed_volume() {
        // No registered instance ⇒ nothing to transition; must not panic or
        // spuriously register anything.
        on_smb_watcher_died("smb-never-registered");
        assert_eq!(get_freshness("smb-never-registered"), None);
    }

    #[test]
    fn overflow_flips_a_fresh_smb_volume_to_stale() {
        // Overflow policy: an unrecoverable CHANGE_NOTIFY overflow drives
        // Fresh ⇒ Stale (the index may have drifted), distinct from a disconnect.
        with_reserved_volume("smb-overflow-test", Freshness::Fresh, || {
            on_smb_overflow("smb-overflow-test");
            assert_eq!(
                get_freshness("smb-overflow-test"),
                Some(Freshness::Stale),
                "an unrecoverable overflow must mark a Fresh index Stale",
            );
        });
    }
}
