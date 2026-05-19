//! File system module - operations, watchers, volumes, and providers.

pub mod cloud_actions;
pub mod git;
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
#[cfg(target_os = "macos")]
pub mod sync_status;
pub mod validation;
pub mod volume;
pub(crate) mod watcher;
pub(crate) mod write_operations;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

// Re-export public types from the listing module
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use listing::ExtendedMetadata;
pub use listing::{
    BriefColumnsError, DirectorySortMode, FileEntry, ListingStartResult, ListingStats, ResortResult, SortColumn,
    SortOrder, StreamingListingStartResult, cancel_listing, compute_brief_column_text_widths, find_file_index,
    find_file_indices, fuzzy_find_first_match_in_listing, get_file_at, get_file_range, get_listing_stats,
    get_total_count, list_directory_end, list_directory_start_streaming, list_directory_start_with_volume,
    refresh_listing_index_sizes, resort_listing,
};
// Batch accessors (used by drag, clipboard, and transfer dialogs)
pub use listing::{get_files_at_indices, get_paths_at_indices};
// Re-export volume types (some not used externally yet)
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use volume::MtpVolume;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use volume::SmbVolume;
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use volume::manager::VolumeManager;
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use volume::{
    CopyScanResult, InMemoryVolume, LocalPosixVolume, MutationEvent, ScanConflict, SourceItemInfo, SpaceInfo, Volume,
    VolumeError,
};
// Watcher management - init_watcher_manager must be called from lib.rs
#[cfg(feature = "playwright-e2e")]
pub use watcher::flush_all_watchers;
pub use watcher::{init_watcher_manager, update_debounce_ms};
// Diff types for file watching (used by MTP module for unified diff events)
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use watcher::compute_diff;
// Re-export write operation types
pub use write_operations::{
    OperationStatus, OperationSummary, WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
    cancel_all_write_operations, cancel_write_operation, copy_files_start, delete_files_start, get_operation_status,
    list_active_operations, move_files_start, trash_files_start,
};
// Re-export volume copy types and functions
pub use write_operations::{
    VolumeCopyConfig, VolumeCopyScanResult, copy_between_volumes, move_between_volumes, scan_for_volume_copy,
};

/// Global volume manager instance
static VOLUME_MANAGER: LazyLock<VolumeManager> = LazyLock::new(VolumeManager::new);

/// Whether to auto-upgrade SMB mounts to direct smb2 connections.
/// Set from the `network.directSmbConnection` setting at startup.
static DIRECT_SMB_ENABLED: AtomicBool = AtomicBool::new(true);

/// Whether to filter macOS safe-save artifacts (.sb- files) in the SMB watcher.
/// Set from the `advanced.filterSafeSaveArtifacts` setting at startup.
static FILTER_SAFE_SAVE_ARTIFACTS: AtomicBool = AtomicBool::new(true);

/// Concurrent SMB ops per session: the `SmbVolume::max_concurrent_ops()` value.
/// Set from the `network.smbConcurrency` setting at startup. Default 10, clamped
/// to `1..=32` (above 32 exceeds smb2's `MAX_PIPELINE_WINDOW`; below 1 is nonsense).
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

/// Sets the safe-save artifact filter preference. Call from app setup after loading settings.
pub fn set_filter_safe_save_artifacts(enabled: bool) {
    FILTER_SAFE_SAVE_ARTIFACTS.store(enabled, Ordering::Relaxed);
}

/// Returns whether safe-save artifact filtering is enabled.
pub fn is_filter_safe_save_artifacts_enabled() -> bool {
    FILTER_SAFE_SAVE_ARTIFACTS.load(Ordering::Relaxed)
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
    VOLUME_MANAGER.register("root", root_volume);
    VOLUME_MANAGER.set_default("root");

    // Register attached volumes and cloud drives (macOS)
    #[cfg(target_os = "macos")]
    {
        let attached = crate::volumes::get_attached_volumes();
        log::debug!("Registering {} attached volume(s)", attached.len());
        for location in attached {
            let volume = Arc::new(LocalPosixVolume::new(&location.name, &location.path));
            VOLUME_MANAGER.register(&location.id, volume);
            log::debug!("  Registered attached volume: {} -> {}", location.id, location.path);
        }

        let cloud = crate::volumes::get_cloud_drives();
        log::debug!("Registering {} cloud drive(s)", cloud.len());
        for location in cloud {
            let volume = Arc::new(LocalPosixVolume::new(&location.name, &location.path));
            VOLUME_MANAGER.register(&location.id, volume);
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
            VOLUME_MANAGER.register(&location.id, volume);
            log::debug!("  Registered volume: {} -> {}", location.id, location.path);
        }
    }
}

/// Returns a reference to the global volume manager.
pub fn get_volume_manager() -> &'static VolumeManager {
    &VOLUME_MANAGER
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
/// - direct-SMB is disabled (`network.directSmbConnection`),
/// - or no SMB mounts are registered (no scan cost, no prompt).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn upgrade_existing_smb_mounts(app_handle: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    use crate::volumes::get_smb_mount_info;
    #[cfg(target_os = "linux")]
    use crate::volumes_linux::get_smb_mount_info;

    if !is_direct_smb_enabled() {
        log::debug!("Direct SMB connections disabled, skipping startup upgrade");
        return;
    }

    // Collect SMB volume paths to upgrade (don't hold the manager lock during async work)
    let volumes_to_upgrade: Vec<(String, String)> = {
        let all_volumes = VOLUME_MANAGER.list_volumes();
        all_volumes
            .into_iter()
            .map(|(id, _name)| id)
            .filter_map(|id| {
                let vol = VOLUME_MANAGER.get(&id)?;
                // Skip volumes that are already SmbVolume
                if vol.smb_connection_state().is_some() {
                    return None;
                }
                let path = vol.root().to_string_lossy().to_string();
                // Check if it's an SMB mount
                let info = get_smb_mount_info(&path)?;
                let _ = info; // We just need to know it's SMB
                Some((id, path))
            })
            .collect()
    };

    if volumes_to_upgrade.is_empty() {
        log::debug!("No SMB mounts to upgrade at startup");
        return;
    }

    log::info!(
        "Found {} SMB mount(s) to upgrade to direct connections",
        volumes_to_upgrade.len()
    );

    // Kick off mDNS so `resolve_ip_to_hostname` can find the host. Without this,
    // the Keychain lookup misses on auth-required shares (creds are keyed by
    // hostname like `smb://naspolya/share`, not by IP). Same pattern as the
    // manual `upgrade_to_smb_volume` and mount-time `try_upgrade_smb_mount`
    // paths. Idempotent: no-op if mDNS is already running.
    crate::network::ensure_mdns_started(app_handle);

    // Use tauri's runtime spawn (this runs during setup() before Tokio is fully available).
    // Wait for mDNS discovery to reach Active state (initial burst complete) so hostname
    // resolution is available for Keychain lookup.
    tauri::async_runtime::spawn(async move {
        wait_for_mdns_ready().await;

        let mut any_upgraded = false;
        for (_volume_id, mount_path) in volumes_to_upgrade {
            let info = match get_smb_mount_info(&mount_path) {
                Some(info) => info,
                None => continue,
            };

            // Resolve hostname from mDNS for Keychain lookup
            let hostname = crate::network::smb_upgrade::resolve_ip_to_hostname(&info.server);

            // Try Keychain creds
            let creds =
                crate::network::smb_upgrade::get_keychain_password(&info.server, hostname.as_deref(), &info.share)
                    .await;

            let (username, password) = match &creds {
                Some((u, p)) => (Some(u.as_str()), Some(p.as_str())),
                None => (None, None),
            };

            crate::network::smb_upgrade::register_smb_volume(
                &info.server,
                &info.share,
                &mount_path,
                username,
                password,
                info.port,
            )
            .await;
            any_upgraded = true;
        }

        // Notify frontend to refresh volume list so indicators update from yellow to green
        if any_upgraded {
            crate::volume_broadcast::emit_volumes_changed();
        }
    });
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
