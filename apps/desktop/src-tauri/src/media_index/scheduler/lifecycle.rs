//! Pass scheduling and wiring: how enrichment passes get kicked, wired to the app, and
//! spawned. The free-function layer around the [`MediaScheduler`] state machine ([the
//! `mod.rs` core]) — [`start`] wires the app and each volume's subscriptions, the
//! `kick_*` entry points fire immediate passes on user actions, [`wire_volume`] maps a
//! volume's kind to a pass and hangs its bus subscriptions, and [`spawn_pass`] drives a
//! coalesced pass on a blocking background task.
//!
//! [the `mod.rs` core]: super::MediaScheduler

use tauri::Manager;

use crate::indexing::IndexVolumeKind;
// The fake backend is production's fallback only off-macOS (macOS uses real Vision);
// tests import it themselves.
#[cfg(not(target_os = "macos"))]
use crate::media_index::backend::fake::FakeVisionBackend;

use super::*;

/// Whether a LOCAL image at index path `path` is COVERED this pass — the pure
/// coverage gate (override + importance threshold), unit-testable without a DB or an
/// app. The privacy exclusion is a SEPARATE, live hard veto applied in
/// [`enrich::enrich_and_gc`] (never here), so coverage stays snapshot-pure while the
/// veto reads live config.
///
/// - When importance HASN'T scored the volume yet (`scores` is `None`), DEFER the
///   importance-gated remainder but still honor an explicit "always index" override
///   (`config.covers`), so a user directive is never postponed on a fresh volume.
///   This mirrors the network `None` → override-only fallback, keeping the two paths
///   symmetric. ❌ Never fall back to "enrich all" here: a first-run race against
///   importance's multi-second recompute would over-index the whole volume, and
///   forward-only semantics make that permanent until a manual reclaim. The
///   unscored → scored bridge ([`wire_volume`]'s subscriber) re-kicks the remainder
///   once importance lands.
/// - When SCORED, cover an override-covered folder OR one whose parent folder met
///   the threshold (already filtered into `scores`).
pub(super) fn local_should_enrich(
    path: &str,
    scores: Option<&HashMap<String, f64>>,
    config: &network::config::NetworkEnrichConfig,
    volume_id: &str,
) -> bool {
    match scores {
        None => config.covers(volume_id, path),
        Some(map) => config.covers(volume_id, path) || map.contains_key(enrich::parent_dir(path)),
    }
}

/// Kick a coalesced enrichment pass for every volume ready to enrich right now —
/// the user-action entry point behind the master toggle, a persisted-on restart, and
/// a threshold decrease. Resolves the managed scheduler and delegates to
/// [`kick_all_ready_passes_with`]. A no-op when the scheduler isn't managed yet (an
/// early call before [`start`]).
pub fn kick_all_ready_passes(app: &AppHandle) {
    if let Some(scheduler) = app.try_state::<Arc<MediaScheduler>>() {
        kick_all_ready_passes_with(scheduler.inner());
    }
}

/// Kick a coalesced pass for every ready volume, given the scheduler handle
/// directly (so [`start`] can call it without a managed-state round-trip). Iterates
/// [`crate::indexing::ready_volumes_with_kind`] and spawns the kind-mapped pass
/// (Local → local, SMB → network which self-checks opt-in, MTP → never). The
/// [`PassCoordinator`] folds a kick that races a running pass into one re-run, and
/// each pass self-gates on the master toggle, so an errant kick while disabled is a
/// cheap no-op. Unconditional by design: staleness makes a redundant pass a fast
/// no-op, so there's no need to gate per volume (contrast importance, which gates on
/// "store has no generation").
pub fn kick_all_ready_passes_with(scheduler: &Arc<MediaScheduler>) {
    kick_ready_passes_from(scheduler, crate::indexing::ready_volumes_with_kind());
}

/// Kick a coalesced pass for each `(volume_id, kind)` in `ready`, mapping the kind to a
/// pass (Local → local, SMB → network which self-checks opt-in) and skipping the
/// never-swept kinds. Split from [`kick_all_ready_passes_with`] so the kind mapping +
/// spawn is testable against a controlled volume list, without the process-global index
/// registry.
pub(super) fn kick_ready_passes_from(scheduler: &Arc<MediaScheduler>, ready: Vec<(String, IndexVolumeKind)>) {
    for (volume_id, kind) in ready {
        let pass_kind = match kind {
            IndexVolumeKind::Local => PassKind::Local,
            IndexVolumeKind::Smb => PassKind::Network,
            // MTP is never background-swept (on-demand only); nothing to kick.
            IndexVolumeKind::Mtp => continue,
            // A LocalExternal (USB/SD) drive's index paths are MOUNT-RELATIVE, so the
            // local pass (which treats stored paths as OS paths) would hand Vision
            // relative paths — the phantom-path bug class. Skip it until mount-root
            // mapping lands (parked: mount-relative paths aren't mapped yet).
            IndexVolumeKind::LocalExternal => continue,
        };
        spawn_pass(Arc::clone(scheduler), volume_id, pass_kind);
    }
}

/// Wire the scheduler to the app: seed the master toggle + network opt-in/override
/// state from settings, register the memory-watchdog stop hook, subscribe to
/// registrations, sweep the registry for already-ready volumes, and wire each
/// volume's scan-completion subscription by kind (local + opted-in SMB enrich; MTP
/// never background-sweeps).
pub fn start(app: &AppHandle) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!(target: "media_index", "media scheduler not started: {e}");
            return;
        }
    };

    // Tell the CLIP module where the model installs, so the query-time text tower and the
    // enrichment image tower can load it (a no-op off macOS).
    crate::media_index::clip::set_data_dir(&data_dir);

    // Seed the master toggle + the network opt-in / always-index overrides from
    // settings (all off/empty by default; sparse-persisted, so absent keys mean off).
    let settings = crate::settings::load_settings(app);
    gate::set_enabled(settings.image_index_enabled == Some(true));
    gate::set_importance_threshold(
        settings
            .media_index_importance_threshold
            .unwrap_or(gate::DEFAULT_IMPORTANCE_THRESHOLD),
    );
    network::config::set_config(network::config::NetworkEnrichConfig {
        opted_in_volumes: settings.media_index_network_volumes.iter().cloned().collect(),
        always_index_volumes: settings.media_index_always_index_volumes.iter().cloned().collect(),
        always_index_folders: settings.media_index_always_index_folders.iter().cloned().collect(),
        excluded_folders: settings.media_index_excluded_folders.iter().cloned().collect(),
    });

    // Share the ONE resident-memory ceiling: the indexing memory watchdog's stop
    // action runs this hook, telling in-flight enrichment to yield — rather than a
    // second independent 16 GB ceiling over the same pool (plan Resources).
    crate::indexing::register_subsystem_stop_hook(Box::new(|| {
        gate::request_cancel();
        // Release the resident vector caches too, so they're counted against the ONE
        // shared ceiling (plan § Query-time vector residency): they reload lazily.
        crate::media_index::vector::cache::clear_all();
    }));

    // Production selects the REAL Vision OCR backend on macOS; other platforms (where
    // Vision doesn't exist) fall back to the deterministic fake so the crate still
    // builds and the scheduler still runs. Tests inject their own fake directly via
    // `MediaScheduler::new`, never through `start`.
    #[cfg(target_os = "macos")]
    let backend: Arc<dyn VisionBackend> = Arc::new(crate::media_index::backend::vision::VisionOcrBackend::new());
    #[cfg(not(target_os = "macos"))]
    let backend: Arc<dyn VisionBackend> = Arc::new(FakeVisionBackend::new());
    log::info!(target: "media_index", "media enrichment scheduler starting");
    let scheduler = Arc::new(MediaScheduler::new_with_app(data_dir, backend, app.clone()));
    app.manage(Arc::clone(&scheduler));

    // Subscribe to registrations FIRST (before the sweep) so a volume registering in
    // the gap isn't dropped (late-registering volumes).
    let reg_scheduler = Arc::clone(&scheduler);
    let mut reg_rx = crate::indexing::lifecycle_bus::subscribe_registrations();
    tauri::async_runtime::spawn(async move {
        loop {
            match reg_rx.recv().await {
                Ok(reg) => wire_volume(Arc::clone(&reg_scheduler), reg.volume_id, reg.kind),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Startup sweep: wire each ready volume's subscriptions. A volume Fresh at launch
    // keeps a `Pending` bus and never re-fires `ScanCompleted`, so wiring alone never
    // enriches it — the kick below is what starts work.
    for (volume_id, kind) in crate::indexing::ready_volumes_with_kind() {
        wire_volume(Arc::clone(&scheduler), volume_id, kind);
    }

    // The persisted-on restart case: with the master toggle already on, kick every
    // ready volume now. Without this, a user whose toggle is on gets "0 of N indexed"
    // after every restart until some volume happens to rescan. Each pass
    // self-gates, and coalescing folds this into any pass a concurrent scan starts.
    if gate::is_enabled() {
        kick_all_ready_passes_with(&scheduler);
    }
}

/// Whether a volume's pass reads bytes locally or off the network (SMB). The
/// coalescing + bus wiring is identical; only which pass method runs differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PassKind {
    /// A local volume: the backend reads on-disk paths directly.
    Local,
    /// An opted-in SMB volume: the conservative byte-fetch pass reads off the mount.
    Network,
}

/// Wire one volume into the scheduler by its typed kind.
///
/// - **Local**: subscribe to the scan-completion bus and enrich locally.
/// - **SMB**: subscribe the same way and run the CONSERVATIVE network pass; the
///   per-volume opt-in is checked INSIDE the pass, so flipping the opt-in on takes
///   effect on the next scan completion (and the opt-in command kicks an immediate
///   pass — see [`kick_network_pass`]).
/// - **MTP**: NEVER background-swept: a phone/camera on MTP is transient
///   and slow, so enrichment is on-demand-per-visit, not a background sweep. The
///   on-demand trigger is a later slice; this gate is real now.
pub(super) fn wire_volume(scheduler: Arc<MediaScheduler>, volume_id: String, kind: IndexVolumeKind) {
    let pass_kind = match kind {
        IndexVolumeKind::Local => PassKind::Local,
        IndexVolumeKind::Smb => PassKind::Network,
        IndexVolumeKind::Mtp => {
            log::debug!(
                target: "media_index",
                "media enrichment skips MTP '{volume_id}': never background-swept (on-demand-per-visit only)"
            );
            return;
        }
        // A LocalExternal (USB/SD) drive's index paths are MOUNT-RELATIVE, not OS paths,
        // so running the local pass (which reads stored paths as OS paths) would feed
        // Vision relative paths — the phantom-path bug class. NOT `PassKind::Local`. Skip
        // it until mount-root mapping lands (parked: mount-relative paths aren't mapped yet).
        IndexVolumeKind::LocalExternal => {
            log::debug!(
                target: "media_index",
                "media enrichment skips LocalExternal '{volume_id}': mount-relative index paths not yet mapped"
            );
            return;
        }
    };

    // The Fresh-at-launch dead-start: this volume's lifecycle bus stays `Pending` and
    // never re-fires `ScanCompleted`, so the subscription below never kicks it — and the
    // `start()`-time sweep kick can race the volume's registration (the sweep runs before
    // the volume is ready, then the registration bus wires it here). So kick an initial
    // coalesced pass for the volume we just wired when the master toggle is on, mirroring
    // importance's `enqueue_initial_full_pass_if_unscored`. The `PassCoordinator` folds
    // this with any sweep-time kick, so a double-kick is a harmless no-op; the network
    // pass self-checks opt-in inside itself.
    if gate::is_enabled() {
        spawn_pass(Arc::clone(&scheduler), volume_id.clone(), pass_kind);
    }

    // Live enrichment follows the index: a modified/new/deleted image under a
    // covered folder re-enriches (or GCs) within the throttle window, without waiting for
    // the next completed scan. LOCAL only: the tick treats stored paths as OS paths (no
    // mount mapping), and SMB's live path never publishes dirs_changed anyway, so wiring
    // it for network would be dead. MTP/LocalExternal already returned above.
    if pass_kind == PassKind::Local {
        live::start_live_follow(Arc::clone(&scheduler), volume_id.clone());
    }

    // Privacy retro-delete re-fire: a folder excluded while this volume was
    // OFFLINE never got purged (the retro-delete had no mount root then). On
    // (re)registration the volume is mounted, so purge any currently-excluded folder
    // that falls under it now. Idempotent and cheap: skipped entirely when nothing is
    // excluded, and a folder on another volume maps to `None` and no-ops.
    {
        let excluded = network::config::snapshot().excluded_folders;
        if !excluded.is_empty()
            && let Some(mount_root) = crate::file_system::get_volume_manager()
                .get(&volume_id)
                .map(|v| v.root().to_string_lossy().into_owned())
        {
            let re_scheduler = Arc::clone(&scheduler);
            let re_volume = volume_id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let mounts = [(re_volume, mount_root)];
                for folder in &excluded {
                    re_scheduler.retro_delete_excluded_folder(folder, &mounts);
                }
            });
        }
    }

    // The unscored → scored bridge (defer-until-scored). Subscribe to
    // importance's recompute-completed `watch` SYNCHRONOUSLY here — BEFORE and
    // independent of the first pass. Watch semantics: a receiver is caught up to the
    // current version at subscribe time, so `changed()` fires only on the NEXT bump. A
    // lazy "a pass reads `None` → then subscribe" flow has a hole: importance can
    // complete in the gap, the receiver comes up already-caught-up, and the volume
    // defers forever. Subscribing up front (mirroring `search`'s
    // `start_importance_weight_subscriber`) closes it. Re-kick only the unscored →
    // scored transition: `take_deferred_for_importance` gates on a per-volume flag a
    // deferring pass set, so a normal volume never re-kicks and a later incremental
    // bump doesn't re-walk the index for nothing.
    let bridge_scheduler = Arc::clone(&scheduler);
    let bridge_volume = volume_id.clone();
    let mut imp_rx = crate::importance::read::subscribe(&volume_id);
    tauri::async_runtime::spawn(async move {
        // Catch up to the current version so `changed()` fires only on a later bump.
        imp_rx.borrow_and_update();
        while imp_rx.changed().await.is_ok() {
            imp_rx.borrow_and_update();
            if bridge_scheduler.take_deferred_for_importance(&bridge_volume) {
                spawn_pass(Arc::clone(&bridge_scheduler), bridge_volume.clone(), pass_kind);
            }
        }
    });

    let sub_scheduler = Arc::clone(&scheduler);
    let sub_volume = volume_id.clone();
    let mut rx = crate::indexing::lifecycle_bus::subscribe(&volume_id);
    tauri::async_runtime::spawn(async move {
        // Observe the retained value EDGE-triggered: `borrow_and_update` marks it
        // seen, so a later `changed()` fires only on a NEW completion, never on a
        // re-read of the retained `Completed`. This is the data-safety property —
        // GC (inside the pass) never runs off a stale retained `Completed`.
        if matches!(
            *rx.borrow_and_update(),
            crate::indexing::lifecycle_bus::ScanState::Completed { .. }
        ) {
            spawn_pass(Arc::clone(&sub_scheduler), sub_volume.clone(), pass_kind);
        }
        while rx.changed().await.is_ok() {
            if matches!(
                *rx.borrow_and_update(),
                crate::indexing::lifecycle_bus::ScanState::Completed { .. }
            ) {
                spawn_pass(Arc::clone(&sub_scheduler), sub_volume.clone(), pass_kind);
            }
        }
    });
}

/// Kick an immediate network pass for a volume (used when the user opts a volume in,
/// so enrichment starts without waiting for the next scan completion). Coalesces with
/// any running pass.
pub fn kick_network_pass(scheduler: Arc<MediaScheduler>, volume_id: String) {
    spawn_pass(scheduler, volume_id, PassKind::Network);
}

/// Request a coalesced enrichment pass and, if this request starts it, drive it
/// (plus any coalesced re-run) on a blocking background task — never on the IPC
/// thread, and on a dedicated worker (SQLite + backend), not rayon.
fn spawn_pass(scheduler: Arc<MediaScheduler>, volume_id: String, kind: PassKind) {
    if scheduler.coordinator.request(&volume_id) == BeginOutcome::Coalesced {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let sched = Arc::clone(&scheduler);
            let vid = volume_id.clone();
            let result = tauri::async_runtime::spawn_blocking(move || match kind {
                PassKind::Local => sched.run_pass_blocking(&vid),
                // Unify the return shape (the network pass reports via its own logs).
                PassKind::Network => sched.run_network_pass_blocking(&vid).map(|()| 0usize),
            })
            .await;
            match result {
                Ok(Ok(count)) => log::debug!(
                    target: "media_index",
                    "enrichment of '{volume_id}' ({kind:?}) enriched {}",
                    crate::pluralize::pluralize(count as u64, "image")
                ),
                Ok(Err(e)) => log::warn!(target: "media_index", "enrichment of '{volume_id}' failed: {e}"),
                Err(e) => log::warn!(target: "media_index", "enrichment task for '{volume_id}' panicked: {e}"),
            }
            if scheduler.coordinator.finish(&volume_id) == FinishOutcome::Done {
                break;
            }
        }
    });
}
