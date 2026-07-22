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
/// snake_case tag) so callers and the per-drive UX classify by variant on BOTH sides
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

    let Some(volume) = manager.get(volume_id) else {
        // No volume registered for this id. Logged (like every gate refusal) so a
        // future refusal isn't invisible in the logs — the reason the missing
        // local-drive branch stayed hidden for so long.
        log::warn!(target: "indexing::smb_index", "SMB index gate: no volume registered for '{volume_id}'");
        return Err(SmbIndexGateReason::NotRegistered);
    };
    match volume.smb_connection_state() {
        // Already a direct smb2 session: ready to index.
        Some(SmbConnectionState::Direct) => return Ok(volume.root().to_path_buf()),
        // A live SmbVolume whose session dropped. Don't silently index a stale
        // session; the FE reconnect flow owns recovery.
        Some(SmbConnectionState::Disconnected) => {
            log::warn!(target: "indexing::smb_index", "SMB index gate: '{volume_id}' smb2 session is disconnected");
            return Err(SmbIndexGateReason::Disconnected);
        }
        // os_mount: a LocalPosixVolume on an smbfs mount. Fall through to upgrade.
        Some(SmbConnectionState::OsMount) | None => {}
    }

    // The `None` case is the os_mount one in practice (LocalPosixVolume on an
    // smbfs mount returns `None` from `smb_connection_state`). But a `None` that
    // ISN'T an smbfs mount is a non-SMB volume — reject it rather than trying to
    // upgrade a local disk.
    if volume.smb_connection_state().is_none() && smb_volume_id_for_path(&volume.root().to_string_lossy()).is_none() {
        log::warn!(target: "indexing::smb_index", "SMB index gate: '{volume_id}' is not an SMB volume");
        return Err(SmbIndexGateReason::NotAnSmbVolume);
    }

    // os_mount → trigger/await the upgrade to a direct smb2 session.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use crate::network::smb_upgrade::UpgradeResult;
        match crate::commands::network::upgrade_to_smb_volume_inner(volume_id.to_string()).await {
            Ok(UpgradeResult::Success) => {}
            Ok(UpgradeResult::CredentialsNeeded { .. }) => {
                log::info!(target: "indexing::smb_index", "SMB index gate: '{volume_id}' needs credentials for a direct smb2 connection");
                return Err(SmbIndexGateReason::CredentialsNeeded);
            }
            Ok(UpgradeResult::NetworkError { message }) => {
                log::warn!(target: "indexing::smb_index", "SMB index gate: upgrade network error for '{volume_id}': {message}");
                return Err(SmbIndexGateReason::UpgradeFailed);
            }
            Err(e) => {
                log::warn!(target: "indexing::smb_index", "SMB index gate: upgrade failed for '{volume_id}': {e}");
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

/// Turn on indexing for an SMB volume (the per-volume enable).
///
/// Gates on a direct smb2 connection (upgrading from os_mount if needed), then
/// starts a `Volume`-trait scan into the volume's own index DB. FDA-independent
/// by design (network paths aren't TCC-protected). Returns the typed gate reason
/// on refusal so the caller (and the per-drive UX) can show an honest, non-string-matched
/// status. A no-op if the volume's index is already active.
pub async fn start_indexing_for_smb(app: AppHandle, volume_id: String) -> Result<(), SmbIndexGateReason> {
    if crate::indexing::state::is_active(&volume_id) {
        log::info!("start_indexing_for_smb: '{volume_id}' already active, no-op");
        return Ok(());
    }

    let mount_root = ensure_direct_smb(&volume_id).await?;

    // (Re-)enabling clears any sticky `user_disabled` marker, so this reflects the
    // user's current intent and future reconnects auto-resume again. Safe here: the
    // early `is_active` return means no writer thread is running for this volume yet,
    // so the brief write connection can't contend (`SQLITE_BUSY`). Only touch an
    // existing DB — a first-ever enable has no marker to clear. Reached by both the
    // manual enable command and the auto-resume hook; on the latter the marker is
    // already absent (the resume gate required it), so this is a no-op there.
    if let Ok(db_path) = crate::indexing::state::resolved_index_db_path(&app, &volume_id)
        && db_path.exists()
    {
        if let Err(e) = crate::indexing::store::IndexStore::set_user_disabled(&db_path, false) {
            log::warn!(target: "indexing::smb_index", "start_indexing_for_smb: clearing user_disabled for '{volume_id}' failed: {e}");
        }
        // Heal `volume_path` for an SMB index written before that meta existed (only
        // the local scan-completion path wrote it), so search can strip the mount
        // root off scope paths without the volume being mounted. No rescan needed.
        if let Err(e) = crate::indexing::store::IndexStore::set_volume_path(&db_path, &mount_root.to_string_lossy()) {
            log::warn!(target: "indexing::smb_index", "start_indexing_for_smb: healing volume_path for '{volume_id}' failed: {e}");
        }
    }

    // The direct gate passed: start the per-volume index over the Volume trait.
    // `start_indexing_for` handles the lock-first reservation, load-as-Stale
    // freshness seeding, and SMB scan-path selection.
    if let Err(e) = crate::indexing::state::start_indexing_for_smb_inner(&app, &volume_id, mount_root) {
        log::warn!("start_indexing_for_smb: start failed for '{volume_id}': {e}");
        // A start failure here isn't a gate reason — it's an internal error
        // (DB open, manager spawn). Treat as UpgradeFailed for the caller's
        // typed surface; the log carries the detail.
        return Err(SmbIndexGateReason::UpgradeFailed);
    }

    // A new external index DB just came online (or resumed): cap accumulation by
    // evicting the least-recently-used OFFLINE external DBs. Safe — never touches
    // a registered/live volume, and this one is now registered. See `retention`.
    crate::indexing::resources::retention::enforce_external_index_cap(&app);
    Ok(())
}

/// Whether a reconnect should auto-resume indexing for this SMB volume. Both must
/// hold on the PERSISTED per-volume state:
/// - a completed scan is recorded (`persisted_scan_completed`) — the "the user
///   enabled indexing here and it finished at least once" signal; a never-enabled
///   share has no such DB, so it's never indexed uninvited, AND
/// - the user hasn't turned indexing OFF (`user_disabled` marker absent).
///
/// The two facts are separate on purpose: `disable_drive_index` KEEPS the DB (with
/// its completed-scan marker) on disk so a re-enable resumes fast rather than
/// rescanning, but writes the sticky `user_disabled` marker to record intent — so a
/// reconnect never turns back on what the user turned off. Enabling
/// (`start_indexing_for_smb`) clears the marker; `forget_drive_index` deletes the
/// whole DB.
pub(crate) fn smb_index_was_enabled(app: &AppHandle, volume_id: &str) -> bool {
    match crate::indexing::state::resolved_index_db_path(app, volume_id) {
        Ok(db_path) => {
            crate::indexing::store::IndexStore::persisted_scan_completed(&db_path)
                && !crate::indexing::store::IndexStore::user_disabled(&db_path)
        }
        Err(e) => {
            log::debug!(target: "indexing::smb_index", "resume gate: can't resolve db path for '{volume_id}': {e}");
            false
        }
    }
}

/// Resume drive indexing for an SMB volume that just came online — after a
/// launch/upgrade session install (`register_smb_volume`) or an in-place
/// reconnect (`do_attempt_reconnect`) — IF the user had it enabled. This is the
/// backend-autonomous half of index recovery: without it, an enabled NAS index
/// silently stays dark after any disconnect or restart until the user re-enables
/// by hand.
///
/// Fire-and-forget and idempotent:
/// - No-op unless a persisted index DB with a completed scan exists
///   (`smb_index_was_enabled`) — never indexes a never-enabled share.
/// - No-op if the index is already active.
/// - Spawns off-thread, so a caller fires it AFTER the session install completes
///   and OUTSIDE any lock (per `indexing/CLAUDE.md`): `start_indexing_for_smb` is
///   async and reserves the registry slot itself. Registering flows through the
///   lifecycle registration bus, so the media scheduler resumes enrichment with
///   no scheduler changes. The resumed index loads Stale (we weren't watching
///   while disconnected); a rescan is what restores Fresh (the honest-sizes model).
///
/// Handle-free: pulls the app handle stashed in `indexing::init` (a no-op before
/// setup or in unit tests). Keyed on the canonical volume id both install paths
/// agree on.
pub(crate) fn resume_smb_index_if_enabled(volume_id: String) {
    let Some(app) = crate::indexing::state::app_handle() else {
        return;
    };
    if crate::indexing::state::is_active(&volume_id) {
        return;
    }
    if !smb_index_was_enabled(&app, &volume_id) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        log::info!(target: "indexing::smb_index", "SMB '{volume_id}' online with a persisted index; resuming indexing");
        if let Err(reason) = start_indexing_for_smb(app, volume_id.clone()).await {
            log::warn!(target: "indexing::smb_index", "auto-resume indexing for '{volume_id}' refused: {reason}");
        }
    });
}

/// Record that an SMB volume's live watcher died (session drop, disconnect, or
/// the watcher task returning on a fatal `next_events` error). Flips a Fresh
/// index to Stale via the freshness state machine.
///
/// This is the `WatcherDied` call site. A reconnect
/// respawns the watcher, but continuity already broke (events were lost while
/// disconnected), so the index stays Stale until the user rescans — the model's
/// "Stale ⇒ Fresh only via rescan" rule. No-op for an unindexed volume.
///
/// Deliberately NOT fired on a clean watcher cancel (volume unmount / deliberate
/// stop): that's a teardown, not a continuity break to surface as Stale.
pub(crate) fn on_smb_watcher_died(volume_id: &str) {
    // Continuity broke: bump the epoch so the persisted dirs read stale (the
    // honest-sizes model), then flip the badge Stale.
    crate::indexing::state::bump_current_epoch_for(volume_id);
    crate::indexing::state::apply_freshness_event(volume_id, crate::indexing::freshness::FreshnessEvent::WatcherDied);
}

/// Record a `CHANGE_NOTIFY` overflow (`STATUS_NOTIFY_ENUM_DIR`) on an SMB volume.
///
/// Policy: overflow means the server dropped change records we can't
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
    // Continuity broke (the index may have drifted): bump the epoch so persisted
    // dirs read stale, then flip the badge Stale.
    crate::indexing::state::bump_current_epoch_for(volume_id);
    crate::indexing::state::apply_freshness_event(
        volume_id,
        crate::indexing::freshness::FreshnessEvent::OverflowUnrecoverable,
    );
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

    // ── Freshness call sites (the live-watch wiring) ──────────────────────

    use std::sync::Arc;

    use crate::indexing::enrichment::{ReadPool, uninstall_read_pool};
    use crate::indexing::freshness::Freshness;
    use crate::indexing::pending_sizes::{PendingSizes, uninstall_pending_sizes};
    use crate::indexing::state::{INDEX_REGISTRY, IndexVolumeKind, get_freshness, try_reserve_initializing_phase};
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
            try_reserve_initializing_phase(
                vid,
                IndexVolumeKind::Smb,
                store,
                pool,
                pending,
                Arc::new(std::sync::Mutex::new(Some(initial)))
            )
            .is_ok(),
            "reserve must succeed",
        );
        body();
        INDEX_REGISTRY.lock().expect("registry").remove(vid);
        uninstall_read_pool(vid);
        uninstall_pending_sizes(vid);
    }

    #[test]
    fn watcher_died_flips_a_fresh_smb_volume_to_stale() {
        // The headline live-watch transition wired at the call site: when the live SMB
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
