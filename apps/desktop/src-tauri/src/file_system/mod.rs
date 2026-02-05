//! File system module - operations, watchers, volumes, and providers.

pub(crate) mod listing;
#[cfg(target_os = "macos")]
mod macos_metadata;
#[cfg(test)]
mod mock_provider;
#[cfg(test)]
mod provider;
#[cfg(test)]
mod real_provider;
#[cfg(target_os = "macos")]
pub mod sync_status;
pub mod volume;
pub(crate) mod watcher;
pub(crate) mod write_operations;

use std::sync::{Arc, LazyLock};

// Re-export public types from the listing module
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use listing::ExtendedMetadata;
pub use listing::{
    FileEntry, ListingStartResult, ListingStats, ResortResult, SortColumn, SortOrder, StreamingListingStartResult,
    cancel_listing, find_file_index, get_file_at, get_file_range, get_listing_stats, get_max_filename_width,
    get_total_count, list_directory_end, list_directory_start_streaming, list_directory_start_with_volume,
    resort_listing,
};
// macOS-only exports (used by drag operations)
#[cfg(target_os = "macos")]
pub use listing::get_paths_at_indices;
// Re-export volume types (some not used externally yet)
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use volume::manager::VolumeManager;
#[allow(unused_imports, reason = "Public API re-exports for future use")]
pub use volume::{
    ConflictInfo, CopyScanResult, InMemoryVolume, LocalPosixVolume, MtpVolume, SourceItemInfo, SpaceInfo, Volume,
    VolumeError,
};
// Watcher management - init_watcher_manager must be called from lib.rs
pub use watcher::{init_watcher_manager, update_debounce_ms};
// Diff types for file watching (used by MTP module for unified diff events)
pub(crate) use watcher::{DirectoryDiff, compute_diff};
// Re-export write operation types
pub use write_operations::{
    OperationStatus, OperationSummary, WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
    cancel_write_operation, copy_files_start, delete_files_start, get_operation_status, list_active_operations,
    move_files_start,
};
// Re-export volume copy types and functions
// TODO: Remove this allow once volume_copy is integrated into Tauri commands (Phase 5)
#[allow(unused_imports, reason = "Volume copy not yet integrated into Tauri commands")]
pub use write_operations::{VolumeCopyConfig, VolumeCopyScanResult, copy_between_volumes, scan_for_volume_copy};

/// Global volume manager instance
static VOLUME_MANAGER: LazyLock<VolumeManager> = LazyLock::new(VolumeManager::new);

/// Initializes the global volume manager with all discovered volumes.
///
/// This should be called during app startup (after init_watcher_manager).
/// Registers:
/// - "root" volume pointing to "/" (the entire filesystem)
/// - Attached volumes (external drives, USB, etc.)
/// - Cloud drives (Dropbox, iCloud, Google Drive, etc.)
pub fn init_volume_manager() {
    // Register root volume
    let root_volume = Arc::new(LocalPosixVolume::new("Macintosh HD", "/"));
    VOLUME_MANAGER.register("root", root_volume);
    VOLUME_MANAGER.set_default("root");

    // Register attached volumes (external drives)
    let attached = crate::volumes::get_attached_volumes();
    log::info!("Registering {} attached volume(s)", attached.len());
    for location in attached {
        let volume = Arc::new(LocalPosixVolume::new(&location.name, &location.path));
        VOLUME_MANAGER.register(&location.id, volume);
        log::info!("  Registered attached volume: {} -> {}", location.id, location.path);
    }

    // Register cloud drives
    let cloud = crate::volumes::get_cloud_drives();
    log::info!("Registering {} cloud drive(s)", cloud.len());
    for location in cloud {
        let volume = Arc::new(LocalPosixVolume::new(&location.name, &location.path));
        VOLUME_MANAGER.register(&location.id, volume);
        log::info!("  Registered cloud drive: {} -> {}", location.id, location.path);
    }
}

/// Returns a reference to the global volume manager.
pub fn get_volume_manager() -> &'static VolumeManager {
    &VOLUME_MANAGER
}

#[cfg(test)]
mod watcher_test;
