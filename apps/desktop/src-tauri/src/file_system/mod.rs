//! File system module - operations, watchers, volumes, and providers.

pub mod backend_settings;
pub mod cloud_actions;
pub mod filesystem_kind;
pub mod git;
/// The app's `VolumeProvider`: what the index asks about mounted volumes.
pub(crate) mod index_provider;
#[cfg(target_os = "linux")]
pub(crate) mod linux_mounts;
pub(crate) mod listing;
#[cfg(target_os = "macos")]
mod macos_metadata;
#[cfg(test)]
mod mock_provider;
#[cfg(target_os = "macos")]
pub mod open_with;
#[cfg(test)]
mod provider;
#[cfg(test)]
mod real_provider;
pub(crate) mod staging;
#[cfg(target_os = "macos")]
pub mod sync_status;
pub mod tags;
pub mod validation;
pub mod volume;
pub(crate) mod watcher;
pub(crate) mod write_operations;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use volume::manager::get_volume_manager;

// Re-export public types from the listing module
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use listing::ExtendedMetadata;
pub use listing::{
    BriefColumnWidths, BriefColumnsIpcError, DirectorySortMode, FileEntry, ListingStartResult, ListingStats,
    ResortResult, RowBeside, SortColumn, SortOrder, StreamingListingStartResult, cancel_listing,
    compute_brief_column_text_widths, find_file_index, find_file_indices, fuzzy_find_first_match_in_listing,
    get_file_at, get_file_beside, get_file_range, get_listing_stats, get_total_count, list_directory_end,
    list_directory_start_streaming, list_directory_start_with_volume, refresh_listing_index_sizes, resort_listing,
};
// Batch accessors (used by drag, clipboard, and transfer dialogs)
pub use listing::{get_files_at_indices, get_paths_at_indices};
// Backstop reaper for orphaned listings - start_orphan_listing_reaper must be called from lib.rs
pub(crate) use listing::start_orphan_listing_reaper;
// Re-export volume types (some not used externally yet)
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use volume::MtpVolume;
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use volume::manager::VolumeManager;
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use volume::{
    BatchScanResult, CopyScanResult, InMemoryVolume, LocalPosixVolume, MutationEvent, ScanConflict, SourceItemInfo,
    SpaceInfo, Volume, VolumeError,
};
// Watcher management - init_watcher_manager must be called from lib.rs
#[cfg(feature = "playwright-e2e")]
pub use watcher::flush_all_watchers;
pub use watcher::{init_watcher_manager, update_debounce_ms};
// Re-export write operation types
pub use write_operations::{
    OperationEventSink, OperationStatus, OperationSummary, TauriEventSink, WriteOperationConfig, WriteOperationError,
    WriteOperationStartResult, busy_volume_ids, cancel_all_write_operations, cancel_write_operation, copy_files_start,
    delete_files_start, get_operation_status, init_busy_volume_emitter, list_active_operations, move_files_start,
    trash_files_start,
};
// Re-export the operation manager surface (queue + lifecycle). `LifecycleStatus`
// and `OperationsChanged` are reached directly via `write_operations::` (the IPC
// event registration and snapshot field), so they don't need a re-export here.
pub use write_operations::{
    OperationSnapshot, PauseAllOutcome, PauseOutcome, cancel_operation, cancel_operations,
    dismiss_all_failed_operations, dismiss_failed_operation, init_operation_event_emitter, list_operations, pause_all,
    pause_operation, resume_all, resume_operation,
};
// Cross-volume transfers. The three `start_volume_*` entry points own the volume
// and destination-path resolution and every archive fork (extract out, copy/move
// into a zip); the IPC commands and any backend caller both go through them, so
// there is one routing to keep right. `write_operations/routing.rs`.
pub use write_operations::{VolumeCopyConfig, VolumeCopyScanResult, scan_for_volume_copy};
pub(crate) use write_operations::{
    resolve_dest_path, resolve_source_volume, start_volume_compress, start_volume_copy, start_volume_move,
    transfer_would_land_on_its_source,
};

/// Whether to auto-upgrade SMB mounts to direct smb2 connections.
/// Set from the `network.directSmbConnection` setting at startup.
static DIRECT_SMB_ENABLED: AtomicBool = AtomicBool::new(true);

/// Whether to filter macOS safe-save artifacts (.sb- files) in the SMB watcher.
/// Concurrent SMB ops per session: the `SmbVolume::max_concurrent_ops()` value.
/// Set from the `network.smbConcurrency` setting at startup. Default 10, clamped
/// to `1..=32` (above 32 exceeds smb2's `MAX_PIPELINE_WINDOW`; below 1 is nonsense).
///
/// ❌ SMB's alone, in name, in help text, and in scope: what one server sustains
/// says nothing about another protocol's. Another backend gets its own setting
/// and its own row in `backend_settings.rs`, never a second reader here.
///
/// `AtomicUsize` because `SmbVolume::max_concurrent_ops()` reads this on every
/// batch-copy dispatch, so lock-free matters.
static SMB_CONCURRENCY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(10);

/// Sets the direct SMB connection preference. Call from app setup after loading settings.
pub fn set_direct_smb_enabled(enabled: bool) {
    DIRECT_SMB_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Returns whether direct SMB connection is enabled.
pub fn is_direct_smb_enabled() -> bool {
    DIRECT_SMB_ENABLED.load(Ordering::Relaxed)
}

/// Sets the SMB concurrency value. Call from app setup after loading settings.
/// Clamps the input to `1..=32` defensively: a misconfigured settings file
/// shouldn't be able to starve or overwhelm the copy engine.
pub fn set_smb_concurrency(value: usize) {
    let clamped = value.clamp(1, 32);
    SMB_CONCURRENCY.store(clamped, Ordering::Relaxed);
}

/// Returns the SMB concurrency limit (1..=32). Read on every batch-copy
/// dispatch by `SmbVolume::max_concurrent_ops()`.
pub fn smb_concurrency() -> usize {
    SMB_CONCURRENCY.load(Ordering::Relaxed)
}

/// Initializes the global volume manager with all discovered volumes.
///
/// This should be called during app startup (after init_watcher_manager).
/// Registers:
/// - "root" volume pointing to "/" (the entire filesystem)
/// - Attached volumes (external drives, USB, etc.)
/// - Cloud drives (Dropbox, iCloud, Google Drive, etc.)
pub fn init_volume_manager() {
    // Register root volume
    #[cfg(target_os = "macos")]
    let root_name = "Macintosh HD";
    #[cfg(not(target_os = "macos"))]
    let root_name = "Root";

    let root_volume = Arc::new(LocalPosixVolume::new(root_name, "/"));
    let manager = get_volume_manager();
    manager.register("root", root_volume);
    manager.set_default("root");

    // Attached volumes and cloud drives are discovered and registered OFF the main
    // thread. `init_volume_manager` runs inside the Tauri `setup` closure (the main
    // thread), and volume discovery touches per-mount metadata that can block for
    // many seconds on a hung network mount; doing it synchronously here froze the
    // whole app at launch, past the point where it could even be force-quit. Root
    // is enough for the default pane; the rest register a moment later and the
    // `volumes-changed` re-emit refreshes the switcher (and SMB connection state).
    // See `volumes/DETAILS.md` § "Hung mounts".
    std::thread::Builder::new()
        .name("volume-init".into())
        .spawn(|| {
            register_discovered_volumes();
            crate::volume_broadcast::emit_volumes_changed();
        })
        .expect("spawn volume-init thread");
}

/// Register the attached volumes, cloud drives, and (Linux) network mounts with
/// the `VolumeManager`. Runs on the `volume-init` helper thread so its blocking
/// metadata syscalls never touch the main thread. See `init_volume_manager`.
fn register_discovered_volumes() {
    // Register attached volumes and cloud drives (macOS)
    #[cfg(target_os = "macos")]
    {
        let attached = crate::volumes::get_attached_volumes();
        log::debug!("Registering {} attached volume(s)", attached.len());
        for location in attached {
            let volume = Arc::new(LocalPosixVolume::new(&location.name, &location.path));
            get_volume_manager().register(&location.id, volume);
            log::debug!("  Registered attached volume: {} -> {}", location.id, location.path);
        }

        let cloud = crate::volumes::get_cloud_drives();
        log::debug!("Registering {} cloud drive(s)", cloud.len());
        for location in cloud {
            let volume = Arc::new(LocalPosixVolume::new(&location.name, &location.path));
            get_volume_manager().register(&location.id, volume);
            log::debug!("  Registered cloud drive: {} -> {}", location.id, location.path);
        }
    }

    // Register mounted volumes, cloud drives, and network mounts (Linux)
    #[cfg(target_os = "linux")]
    {
        let locations = crate::volumes_linux::list_locations();
        let non_fav: Vec<_> = locations
            .iter()
            .filter(|l| l.category != crate::volumes_linux::LocationCategory::Favorite)
            .collect();
        log::debug!("Registering {} volume(s)", non_fav.len());
        for location in non_fav {
            let volume = Arc::new(LocalPosixVolume::new(&location.name, &location.path));
            get_volume_manager().register(&location.id, volume);
            log::debug!("  Registered volume: {} -> {}", location.id, location.path);
        }
    }
}

/// Upgrades all existing SMB mounts to direct smb2 connections (background task).
///
/// Scans all registered volumes, finds those on `smbfs`, and tries to establish
/// a parallel smb2 session for each. Non-blocking: failures are logged and skipped.
///
/// If any SMB mounts are found, kicks off mDNS via `ensure_mdns_started` so the
/// upgrade's Keychain lookup (keyed by hostname, not IP) can find stored creds.
/// This mirrors the manual "Connect directly" and mount-time auto-upgrade paths,
/// so existing OS-mounted SMB shares get the same treatment as new ones — see
/// the "SMB upgrade waits briefly for mDNS to warm" gotcha in
/// `network/CLAUDE.md`. Kicking off mDNS will pop the macOS Local Network prompt
/// once per app on first launch; that's the trade-off for not requiring users
/// to click "Connect directly" on every relaunch when they have direct-SMB on
/// and an existing mount.
///
/// Returns silently when:
/// - the run may not adopt pre-existing network mounts (an E2E run: the only
///   mounts here are the developer's own — see
///   `test_mode::may_adopt_preexisting_network_mounts`),
/// - direct-SMB is disabled (`network.directSmbConnection`),
/// - or no SMB mounts are registered (no scan cost, no prompt).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn upgrade_existing_smb_mounts(app_handle: tauri::AppHandle) {
    use crate::network::smb_upgrade::UpgradePass;

    // Before the scan, not after: an E2E run must not so much as look at what the
    // developer has mounted, let alone connect to it or speak about it.
    if !crate::test_mode::may_adopt_preexisting_network_mounts() {
        log::debug!("Under an E2E run; not adopting the machine's pre-existing SMB mounts");
        return;
    }

    if !is_direct_smb_enabled() {
        log::debug!("Direct SMB connections disabled, skipping startup upgrade");
        return;
    }

    // Cheap gate only: is there anything here at all? Kicking off mDNS pops the
    // macOS Local Network prompt, so a machine with no SMB mounts must not reach
    // it. The list this returns is NOT what we act on (see below).
    if os_mounted_smb_shares().is_empty() {
        log::debug!("No SMB mounts to upgrade");
        return;
    }

    // One pass at a time. `ensure_network_discovery_started` calls us on every
    // user networking action, and each pass then sits in `wait_for_mdns_ready`
    // for up to 15 s: two clicks nine seconds apart used to stack two passes
    // that both fired blind, replacing an already-healthy volume twice.
    let Some(pass) = UpgradePass::begin() else {
        log::debug!("An SMB upgrade pass is already in flight; not starting another");
        return;
    };

    // Kick off mDNS so the per-volume hostname resolution can find the host.
    // Without this, the Keychain lookup misses on auth-required shares (creds are
    // keyed by hostname like `smb://naspolya/share`, not by IP). Same pattern as
    // the manual `upgrade_to_smb_volume` and mount-time `try_upgrade_smb_mount`
    // paths. Idempotent: no-op if mDNS is already running.
    crate::network::ensure_mdns_started(app_handle);

    // Use tauri's runtime spawn (this runs during setup() before Tokio is fully available).
    // Wait for mDNS discovery to reach Active state (initial burst complete) so hostname
    // resolution is available for Keychain lookup.
    tauri::async_runtime::spawn(async move {
        let _pass = pass; // released when this task ends, whatever the outcome
        wait_for_mdns_ready().await;

        // Scan AFTER the wait, never before. The wait is up to 15 s long, and in
        // that window another path (a manual "Connect directly", the FSEvents
        // mount-time upgrade) can have made these volumes direct already, while
        // a share mounted during the wait wouldn't be in a pre-scan at all. Both
        // halves of that were real: acting on a 15-second-old list is what
        // replaced a healthy volume out from under a running copy.
        let volumes_to_upgrade = os_mounted_smb_shares();
        if volumes_to_upgrade.is_empty() {
            log::debug!("Nothing left to upgrade once mDNS settled");
            return;
        }

        log::info!(
            "Upgrading {} SMB mount(s) to direct connections",
            volumes_to_upgrade.len()
        );

        for (mount_path, info) in volumes_to_upgrade {
            // Shared with the mount-time auto-upgrade path. Uses the mDNS
            // host-cache wait so creds keyed by hostname (the common case) are
            // found instead of falling back to guest. Re-checks freshness once
            // more just before connecting (`smb_upgrade::is_already_direct`),
            // since each volume's own 1.5 s mDNS wait reopens the window.
            crate::network::smb_upgrade::resolve_and_register_smb_volume(
                &info.server,
                &info.share,
                &mount_path,
                info.port,
            )
            .await;
        }

        // Notify frontend to refresh volume list so indicators update from yellow to green
        crate::volume_broadcast::emit_volumes_changed();
    });
}

/// The per-platform mount-info struct `get_smb_mount_info` returns.
#[cfg(target_os = "macos")]
type SmbMountInfo = crate::volumes::SmbMountInfo;
#[cfg(target_os = "linux")]
type SmbMountInfo = crate::volumes_linux::SmbMountInfo;

/// The OS-mounted SMB shares that don't have a Cmdr smb2 session yet, as
/// `(mount_path, mount_info)`.
///
/// A registered `SmbVolume` (whatever its connection state) is excluded: it
/// already has a session, and a `Disconnected` one owns its own recovery through
/// `attempt_reconnect` rather than through a replacement.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn os_mounted_smb_shares() -> Vec<(String, SmbMountInfo)> {
    #[cfg(target_os = "macos")]
    use crate::volumes::get_smb_mount_info;
    #[cfg(target_os = "linux")]
    use crate::volumes_linux::get_smb_mount_info;

    let manager = get_volume_manager();
    manager
        .list_volumes()
        .into_iter()
        .filter_map(|(id, _name)| {
            let vol = manager.get(&id)?;
            if vol.smb_connection_state().is_some() {
                return None;
            }
            let path = vol.root().to_string_lossy().to_string();
            let info = get_smb_mount_info(&path)?;
            Some((path, info))
        })
        .collect()
}

/// Waits until mDNS discovery reaches the `Active` state (initial burst complete).
///
/// Polls every 500ms for up to 15 seconds. If discovery never reaches Active,
/// proceeds anyway: the upgrade will try without hostname resolution and may
/// fall back to guest access.
#[cfg(any(target_os = "macos", target_os = "linux"))]
async fn wait_for_mdns_ready() {
    use crate::network::{DiscoveryState, get_discovery_state_value};

    for _ in 0..30 {
        match get_discovery_state_value() {
            DiscoveryState::Active => {
                log::debug!("mDNS discovery is Active, proceeding with SMB upgrades");
                return;
            }
            _ => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
        }
    }
    log::debug!("mDNS discovery didn't reach Active within 15s, proceeding anyway");
}

#[cfg(test)]
mod watcher_test;
