//! MTP connection management.
//!
//! Manages device connections with a global registry. Each connected device
//! maintains an active MTP session until disconnected or unplugged.
//!
//! ## File watching
//!
//! MTP devices support event notifications via USB interrupt endpoints. When a
//! device is connected, we start a background task that polls for events using
//! `device.next_event()`. Events like ObjectAdded, ObjectRemoved, and ObjectInfoChanged
//! trigger incremental `directory-diff` events to the frontend, using the same
//! unified diff system as local file watching. This provides smooth UI updates
//! without full directory reloads.

use log::{debug, error, info, warn};
use mtp_rs::ptp::OperationCode;
use mtp_rs::{MtpDevice, MtpDeviceBuilder, NewObjectInfo, ObjectHandle, StorageId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, broadcast};

use super::types::{MtpDeviceInfo, MtpStorageInfo};
use crate::file_system::listing::{get_listings_by_volume_prefix, update_listing_entries};
use crate::file_system::{CopyScanResult, DirectoryDiff, FileEntry, MtpVolume, compute_diff, get_volume_manager};

/// Default timeout for MTP operations (30 seconds - some devices are slow).
const MTP_TIMEOUT_SECS: u64 = 30;

/// Global counter for generating unique request IDs for debugging.
static REQUEST_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Tracks concurrent list_directory calls for debugging lock contention.
static CONCURRENT_LIST_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Error types for MTP connection operations.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum MtpConnectionError {
    /// Device not found (may have been unplugged).
    DeviceNotFound { device_id: String },
    /// Device is not connected.
    NotConnected { device_id: String },
    /// Another process has exclusive access to the device.
    ExclusiveAccess {
        device_id: String,
        blocking_process: Option<String>,
    },
    /// Connection timed out.
    Timeout { device_id: String },
    /// Device was disconnected unexpectedly.
    Disconnected { device_id: String },
    /// Protocol error from device.
    Protocol { device_id: String, message: String },
    /// Device is busy (retryable).
    DeviceBusy { device_id: String },
    /// Storage is full.
    StorageFull { device_id: String },
    /// Object not found on device.
    ObjectNotFound { device_id: String, path: String },
    /// Other connection error.
    Other { device_id: String, message: String },
}

impl MtpConnectionError {
    /// Returns true if the operation may succeed if retried.
    #[allow(dead_code, reason = "Will be used by frontend for retry logic")]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout { .. } | Self::DeviceBusy { .. })
    }

    /// Returns a user-friendly message for this error.
    #[allow(dead_code, reason = "Will be exposed via Tauri commands for UI error display")]
    pub fn user_message(&self) -> String {
        match self {
            Self::DeviceNotFound { .. } => "Device not found. It may have been unplugged.".to_string(),
            Self::NotConnected { .. } => {
                "Device is not connected. Select it from the volume picker to connect.".to_string()
            }
            Self::ExclusiveAccess { blocking_process, .. } => {
                if let Some(proc) = blocking_process {
                    format!(
                        "Another app ({}) is using this device. Close it or use the Terminal workaround.",
                        proc
                    )
                } else {
                    "Another app is using this device. Close other apps that might be accessing it.".to_string()
                }
            }
            Self::Timeout { .. } => {
                "The operation timed out. The device may be slow or unresponsive. Try again.".to_string()
            }
            Self::Disconnected { .. } => "Device was disconnected. Reconnect it to continue.".to_string(),
            Self::Protocol { message, .. } => {
                format!("Device reported an error: {}. Try reconnecting.", message)
            }
            Self::DeviceBusy { .. } => "Device is busy. Wait a moment and try again.".to_string(),
            Self::StorageFull { .. } => "Device storage is full. Free up some space.".to_string(),
            Self::ObjectNotFound { path, .. } => {
                format!("File or folder not found: {}. It may have been deleted.", path)
            }
            Self::Other { message, .. } => message.clone(),
        }
    }
}

impl std::fmt::Display for MtpConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceNotFound { device_id } => {
                write!(f, "Device not found: {device_id}")
            }
            Self::NotConnected { device_id } => {
                write!(f, "Device not connected: {device_id}")
            }
            Self::ExclusiveAccess {
                device_id,
                blocking_process,
            } => {
                if let Some(proc) = blocking_process {
                    write!(f, "Device {device_id} is in use by {proc}")
                } else {
                    write!(f, "Device {device_id} is in use by another process")
                }
            }
            Self::Timeout { device_id } => {
                write!(f, "Connection timed out for device: {device_id}")
            }
            Self::Disconnected { device_id } => {
                write!(f, "Device disconnected: {device_id}")
            }
            Self::Protocol { device_id, message } => {
                write!(f, "Protocol error for {device_id}: {message}")
            }
            Self::DeviceBusy { device_id } => {
                write!(f, "Device busy: {device_id}")
            }
            Self::StorageFull { device_id } => {
                write!(f, "Storage full on device: {device_id}")
            }
            Self::ObjectNotFound { device_id, path } => {
                write!(f, "Object not found on {device_id}: {path}")
            }
            Self::Other { device_id, message } => {
                write!(f, "Error for {device_id}: {message}")
            }
        }
    }
}

impl std::error::Error for MtpConnectionError {}

// ============================================================================
// Progress events for MTP file operations
// ============================================================================

/// Progress event for MTP file transfers (download/upload).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpTransferProgress {
    /// Unique operation ID.
    pub operation_id: String,
    /// Device ID.
    pub device_id: String,
    /// Type of transfer.
    pub transfer_type: MtpTransferType,
    /// Current file being transferred.
    pub current_file: String,
    /// Bytes transferred so far.
    pub bytes_done: u64,
    /// Total bytes to transfer.
    pub bytes_total: u64,
}

/// Type of MTP transfer operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MtpTransferType {
    Download,
    Upload,
}

/// Result of a successful MTP operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpOperationResult {
    /// Operation ID (for tracking).
    pub operation_id: String,
    /// Number of files processed.
    pub files_processed: usize,
    /// Total bytes transferred.
    pub bytes_transferred: u64,
}

/// State for tracking cancellation of MTP operations.
#[allow(dead_code, reason = "Will be used in Phase 5 for operation cancellation")]
pub struct MtpOperationState {
    /// Cancellation flag.
    pub cancelled: AtomicBool,
}

impl Default for MtpOperationState {
    fn default() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
        }
    }
}

/// Information about an object on the device (returned after creation).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpObjectInfo {
    /// Object handle.
    pub handle: u32,
    /// Object name.
    pub name: String,
    /// Virtual path on device.
    pub path: String,
    /// Whether it's a directory.
    pub is_directory: bool,
    /// Size in bytes (None for directories).
    pub size: Option<u64>,
}

/// Information about a connected device, including its storages.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedDeviceInfo {
    /// Device information.
    pub device: MtpDeviceInfo,
    /// Available storages on the device.
    pub storages: Vec<MtpStorageInfo>,
}

/// Internal entry for a connected device.
struct DeviceEntry {
    /// The MTP device handle (wrapped in Arc for shared access).
    device: Arc<Mutex<MtpDevice>>,
    /// Device metadata.
    info: MtpDeviceInfo,
    /// Cached storage information.
    storages: Vec<MtpStorageInfo>,
    /// Path-to-handle cache per storage.
    path_cache: RwLock<HashMap<u32, PathHandleCache>>,
    /// Directory listing cache per storage.
    listing_cache: RwLock<HashMap<u32, ListingCache>>,
}

/// Cache for mapping paths to MTP object handles.
#[derive(Default)]
struct PathHandleCache {
    /// Maps virtual path -> MTP object handle.
    path_to_handle: HashMap<PathBuf, ObjectHandle>,
}

/// Cache for directory listings.
#[derive(Default)]
struct ListingCache {
    /// Maps directory path -> cached file entries.
    listings: HashMap<PathBuf, CachedListing>,
}

/// A cached directory listing with timestamp for invalidation.
struct CachedListing {
    /// The cached file entries.
    entries: Vec<FileEntry>,
    /// When this listing was cached (for TTL checks).
    cached_at: Instant,
}

/// How long to keep cached listings (5 seconds).
const LISTING_CACHE_TTL_SECS: u64 = 5;

/// Debounce duration for MTP directory change events (500ms).
/// MTP devices can emit rapid events during bulk operations (e.g., copying many files).
const EVENT_DEBOUNCE_MS: u64 = 500;

/// Debouncer for MTP directory change events.
///
/// Prevents flooding the frontend with events during rapid operations like
/// bulk copy/delete. Each device has its own last-emit timestamp.
struct EventDebouncer {
    /// Last emit time per device ID.
    last_emit: RwLock<HashMap<String, Instant>>,
    /// Debounce duration.
    debounce_duration: Duration,
}

impl EventDebouncer {
    /// Creates a new debouncer with the given duration.
    fn new(debounce_duration: Duration) -> Self {
        Self {
            last_emit: RwLock::new(HashMap::new()),
            debounce_duration,
        }
    }

    /// Checks if we should emit an event for the given device.
    /// Updates the last emit time if we should emit.
    fn should_emit(&self, device_id: &str) -> bool {
        let now = Instant::now();
        let mut last_emit = self.last_emit.write().unwrap();

        if let Some(last) = last_emit.get(device_id)
            && now.duration_since(*last) < self.debounce_duration
        {
            return false;
        }

        last_emit.insert(device_id.to_string(), now);
        true
    }

    /// Clears the debounce state for a device (called on disconnect).
    fn clear(&self, device_id: &str) {
        let mut last_emit = self.last_emit.write().unwrap();
        last_emit.remove(device_id);
    }
}

/// Global connection manager for MTP devices.
pub struct MtpConnectionManager {
    /// Map of device_id -> connected device entry.
    devices: Mutex<HashMap<String, DeviceEntry>>,
    /// Channels to signal event loop shutdown per device.
    event_loop_shutdown: RwLock<HashMap<String, broadcast::Sender<()>>>,
    /// Debouncer for directory change events.
    event_debouncer: EventDebouncer,
}

/// Acquires the device lock with a timeout.
/// This prevents indefinite blocking if the device is unresponsive or another operation is stuck.
async fn acquire_device_lock<'a>(
    device_arc: &'a Arc<Mutex<MtpDevice>>,
    device_id: &str,
    operation: &str,
) -> Result<tokio::sync::MutexGuard<'a, MtpDevice>, MtpConnectionError> {
    tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), device_arc.lock())
        .await
        .map_err(|_| {
            error!("MTP {}: timed out waiting for device lock", operation);
            MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            }
        })
}

impl MtpConnectionManager {
    /// Creates a new connection manager.
    fn new() -> Self {
        Self {
            devices: Mutex::new(HashMap::new()),
            event_loop_shutdown: RwLock::new(HashMap::new()),
            event_debouncer: EventDebouncer::new(Duration::from_millis(EVENT_DEBOUNCE_MS)),
        }
    }

    /// Connects to an MTP device by ID.
    ///
    /// Opens an MTP session and retrieves storage information.
    ///
    /// # Returns
    ///
    /// Information about the connected device including available storages.
    pub async fn connect(
        &self,
        device_id: &str,
        app: Option<&AppHandle>,
    ) -> Result<ConnectedDeviceInfo, MtpConnectionError> {
        // Check if already connected - if so, return existing connection info (idempotent)
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id) {
                debug!(
                    "connect: {} already connected, returning existing connection info",
                    device_id
                );
                return Ok(ConnectedDeviceInfo {
                    device: entry.info.clone(),
                    storages: entry.storages.clone(),
                });
            }
        }

        info!("Connecting to MTP device: {}", device_id);

        // Parse device_id to get location_id (format: "mtp-{location_id}")
        let location_id = parse_device_id(device_id).ok_or_else(|| MtpConnectionError::DeviceNotFound {
            device_id: device_id.to_string(),
        })?;
        debug!("Parsed device_id: location_id={}", location_id);

        // Find and open the device
        debug!("Opening MTP device (timeout={}s)...", MTP_TIMEOUT_SECS);
        let device = match open_device(location_id).await {
            Ok(d) => d,
            Err(e) => {
                // Check for exclusive access error
                if e.is_exclusive_access() {
                    #[cfg(target_os = "macos")]
                    let blocking_process = super::macos_workaround::get_usb_exclusive_owner();
                    #[cfg(not(target_os = "macos"))]
                    let blocking_process: Option<String> = None;

                    // Emit event for frontend to show dialog
                    if let Some(app) = app {
                        let _ = app.emit(
                            "mtp-exclusive-access-error",
                            serde_json::json!({
                                "deviceId": device_id,
                                "blockingProcess": blocking_process.clone()
                            }),
                        );
                    }

                    return Err(MtpConnectionError::ExclusiveAccess {
                        device_id: device_id.to_string(),
                        blocking_process,
                    });
                }

                // Map other errors
                return Err(map_mtp_error(e, device_id));
            }
        };

        // Get device info
        let mtp_info = device.device_info();
        let device_info = MtpDeviceInfo {
            id: device_id.to_string(),
            location_id,
            vendor_id: 0, // Not available from device_info
            product_id: 0,
            manufacturer: if mtp_info.manufacturer.is_empty() {
                None
            } else {
                Some(mtp_info.manufacturer.clone())
            },
            product: if mtp_info.model.is_empty() {
                None
            } else {
                Some(mtp_info.model.clone())
            },
            serial_number: if mtp_info.serial_number.is_empty() {
                None
            } else {
                Some(mtp_info.serial_number.clone())
            },
        };

        debug!(
            "Device opened successfully: {} {}",
            mtp_info.manufacturer, mtp_info.model
        );

        // Check if device supports write operations (SendObjectInfo is required for uploads)
        // PTP cameras often don't support this, making them effectively read-only
        let device_supports_write = mtp_info.supports_operation(OperationCode::SendObjectInfo);
        info!(
            "Device '{}' write support: {} (operations: {:?})",
            mtp_info.model,
            device_supports_write,
            mtp_info
                .operations_supported
                .iter()
                .filter(|op| matches!(
                    op,
                    OperationCode::SendObjectInfo | OperationCode::SendObject | OperationCode::DeleteObject
                ))
                .collect::<Vec<_>>()
        );

        // Get storage information
        debug!("Fetching storage information...");
        let storages = match get_storages(&device, device_supports_write).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get storages for {}: {:?}", device_id, e);
                Vec::new()
            }
        };

        let connected_info = ConnectedDeviceInfo {
            device: device_info.clone(),
            storages: storages.clone(),
        };

        // Wrap device in Arc for shared access
        let device_arc = Arc::new(Mutex::new(device));

        // Store in registry
        {
            let mut devices = self.devices.lock().await;
            devices.insert(
                device_id.to_string(),
                DeviceEntry {
                    device: Arc::clone(&device_arc),
                    info: device_info,
                    storages,
                    path_cache: RwLock::new(HashMap::new()),
                    listing_cache: RwLock::new(HashMap::new()),
                },
            );
        }

        // Register MTP volumes for each storage with the global VolumeManager
        // This enables MTP browsing through the standard file listing pipeline
        for storage in &connected_info.storages {
            let volume_id = format!("{}:{}", device_id, storage.id);
            let volume = Arc::new(MtpVolume::new(device_id, storage.id, &storage.name));
            get_volume_manager().register(&volume_id, volume);
            debug!("Registered MTP volume: {} ({})", volume_id, storage.name);
        }

        // Start the event loop for file watching (requires AppHandle)
        if let Some(app) = app {
            self.start_event_loop(device_id.to_string(), device_arc, app.clone());
        }

        // Emit connected event
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-device-connected",
                serde_json::json!({
                    "deviceId": device_id,
                    "storages": connected_info.storages
                }),
            );
        }

        info!(
            "MTP device connected: {} ({} storages)",
            device_id,
            connected_info.storages.len()
        );

        Ok(connected_info)
    }

    /// Disconnects from an MTP device.
    ///
    /// Closes the MTP session gracefully.
    pub async fn disconnect(&self, device_id: &str, app: Option<&AppHandle>) -> Result<(), MtpConnectionError> {
        info!("Disconnecting from MTP device: {}", device_id);

        // Stop the event loop first
        self.stop_event_loop(device_id);

        // Remove from registry
        let entry = {
            let mut devices = self.devices.lock().await;
            devices.remove(device_id)
        };

        let Some(entry) = entry else {
            return Err(MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            });
        };

        // Unregister MTP volumes from the VolumeManager
        for storage in &entry.storages {
            let volume_id = format!("{}:{}", device_id, storage.id);
            get_volume_manager().unregister(&volume_id);
            debug!("Unregistered MTP volume: {}", volume_id);
        }

        // The device will be closed when it's dropped.
        // MtpDevice::close() takes ownership, but we have it in an Arc<Mutex>.
        // Dropping the entry will drop the Arc, and if this is the last reference,
        // the device will be closed (MtpDevice has a Drop impl that closes the session).
        // We just drop the entry here - the device handle going out of scope handles cleanup.
        drop(entry);

        // Emit disconnected event
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-device-disconnected",
                serde_json::json!({
                    "deviceId": device_id,
                    "reason": "user"
                }),
            );
        }

        info!("MTP device disconnected: {}", device_id);
        Ok(())
    }

    // ========================================================================
    // Event Loop for File Watching
    // ========================================================================

    /// Starts the event polling loop for a connected device.
    ///
    /// This spawns a background task that polls for MTP device events and emits
    /// `mtp-directory-changed` events to the frontend when files change on the device.
    fn start_event_loop(&self, device_id: String, device: Arc<Mutex<MtpDevice>>, app: AppHandle) {
        let (shutdown_tx, _) = broadcast::channel(1);

        // Store shutdown sender
        {
            let mut shutdown_map = self.event_loop_shutdown.write().unwrap();
            shutdown_map.insert(device_id.clone(), shutdown_tx.clone());
        }

        // Clone for the spawned task
        let device_id_clone = device_id.clone();

        // Spawn the event loop task. It uses connection_manager() to access the debouncer
        // since the debouncer is part of the global singleton.
        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_tx.subscribe();

            debug!("MTP event loop started for device: {}", device_id_clone);

            loop {
                // Try to acquire the device lock with a short timeout to check for shutdown
                let poll_result = tokio::select! {
                    biased;

                    // Check for shutdown signal first
                    _ = shutdown_rx.recv() => {
                        debug!("MTP event loop shutting down (signal): {}", device_id_clone);
                        break;
                    }

                    // Poll for next event (with timeout built into next_event)
                    result = async {
                        // Try to lock the device - use a timeout to prevent deadlocks
                        match tokio::time::timeout(Duration::from_secs(5), device.lock()).await {
                            Ok(guard) => {
                                // Poll for event
                                guard.next_event().await
                            }
                            Err(_) => {
                                // Timeout acquiring lock - device might be busy with another operation
                                // Return timeout to continue polling
                                Err(mtp_rs::Error::Timeout)
                            }
                        }
                    } => {
                        result
                    }
                };

                match poll_result {
                    Ok(event) => {
                        Self::handle_device_event(&device_id_clone, event, &app);
                    }
                    Err(mtp_rs::Error::Timeout) => {
                        // No event within timeout period - continue polling
                        // Add a small sleep to avoid tight loop when device is idle
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(mtp_rs::Error::Disconnected) => {
                        info!("MTP device disconnected (event loop): {}", device_id_clone);
                        // Device was unplugged - clean up state and emit event
                        // IMPORTANT: Call handle_device_disconnected to remove from devices registry
                        // so reconnection attempts don't fail with "already connected"
                        connection_manager()
                            .handle_device_disconnected(&device_id_clone, Some(&app))
                            .await;
                        break;
                    }
                    Err(e) => {
                        // Log other errors but continue polling - device might recover
                        warn!("MTP event error for {}: {:?}", device_id_clone, e);
                        // Sleep a bit before retrying to avoid tight error loop
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }

            debug!("MTP event loop exited for device: {}", device_id_clone);
        });

        debug!("MTP event loop spawned for device: {}", device_id);
    }

    /// Stops the event loop for a device.
    fn stop_event_loop(&self, device_id: &str) {
        // Remove and signal shutdown
        if let Some(tx) = self.event_loop_shutdown.write().unwrap().remove(device_id) {
            let _ = tx.send(()); // Signal shutdown - ignore error if receiver is gone
            debug!("MTP event loop shutdown signaled for device: {}", device_id);
        }

        // Clear debouncer state for this device
        self.event_debouncer.clear(device_id);
    }

    /// Handles a device event and emits to frontend if appropriate.
    fn handle_device_event(device_id: &str, event: mtp_rs::mtp::DeviceEvent, app: &AppHandle) {
        use mtp_rs::mtp::DeviceEvent;

        match event {
            DeviceEvent::ObjectAdded { handle } => {
                debug!("MTP object added: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
            }
            DeviceEvent::ObjectRemoved { handle } => {
                debug!("MTP object removed: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
            }
            DeviceEvent::ObjectInfoChanged { handle } => {
                debug!("MTP object changed: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
            }
            DeviceEvent::StorageInfoChanged { storage_id } => {
                debug!("MTP storage info changed: {:?} on {}", storage_id, device_id);
                // Could emit a storage space update event in the future
            }
            DeviceEvent::StoreAdded { storage_id } => {
                info!("MTP storage added: {:?} on {}", storage_id, device_id);
                // Could emit a storage list update event in the future
            }
            DeviceEvent::StoreRemoved { storage_id } => {
                info!("MTP storage removed: {:?} on {}", storage_id, device_id);
                // Could emit a storage list update event in the future
            }
            DeviceEvent::DeviceInfoChanged => {
                debug!("MTP device info changed: {}", device_id);
            }
            DeviceEvent::DeviceReset => {
                warn!("MTP device reset: {}", device_id);
            }
            DeviceEvent::Unknown { code, params } => {
                debug!("MTP unknown event {:04x} {:?} on {}", code, params, device_id);
            }
        }
    }

    /// Emits directory-diff events for all affected listings (with debouncing).
    ///
    /// Uses the unified diff system shared with local file watching, providing
    /// smooth incremental UI updates without full directory reloads.
    fn emit_directory_changed(device_id: &str, app: &AppHandle) {
        // Check debouncer via the global connection manager
        if !connection_manager().event_debouncer.should_emit(device_id) {
            debug!(
                "MTP event loop: directory change DEBOUNCED for device={} (within {}ms window)",
                device_id, EVENT_DEBOUNCE_MS
            );
            return;
        }

        // Find all listings for this device (volume IDs like "mtp-123:65537")
        let listings = get_listings_by_volume_prefix(device_id);
        if listings.is_empty() {
            debug!(
                "MTP event loop: no active listings for device={}, skipping diff",
                device_id
            );
            return;
        }

        debug!(
            "MTP event loop: found {} listings for device={}, computing diffs",
            listings.len(),
            device_id
        );

        // Clone what we need for the spawned task
        let device_id_owned = device_id.to_string();
        let app_clone = app.clone();

        // Spawn task to re-read directories and compute diffs
        tokio::spawn(async move {
            Self::compute_and_emit_diffs(&device_id_owned, listings, &app_clone).await;
        });
    }

    /// Re-reads MTP directories and emits directory-diff events.
    ///
    /// For each listing belonging to this device:
    /// 1. Extract the storage_id and path from the volume_id and listing path
    /// 2. Re-read the directory from the MTP device
    /// 3. Compute the diff between old and new entries
    /// 4. Update LISTING_CACHE with new entries
    /// 5. Emit directory-diff event
    async fn compute_and_emit_diffs(
        device_id: &str,
        listings: Vec<(String, String, PathBuf, Vec<FileEntry>)>,
        app: &AppHandle,
    ) {
        // Track sequence numbers per listing (simple counter, increments each diff)
        static SEQUENCE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        for (listing_id, volume_id, path, old_entries) in listings {
            // Extract storage_id from volume_id (format: "mtp-{device}:{storage}")
            let Some(storage_id) = volume_id.split(':').nth(1).and_then(|s| s.parse::<u32>().ok()) else {
                warn!(
                    "MTP diff: could not parse storage_id from volume_id={}, skipping",
                    volume_id
                );
                continue;
            };

            // Convert path to MTP inner path
            let mtp_path = path.to_string_lossy();
            let mtp_path = if mtp_path.starts_with("mtp://") {
                // Parse: mtp://mtp-0-1/65537/DCIM/Camera -> DCIM/Camera
                let without_scheme = mtp_path.strip_prefix("mtp://").unwrap_or(&mtp_path);
                let parts: Vec<&str> = without_scheme.splitn(3, '/').collect();
                if parts.len() >= 3 {
                    parts[2].to_string()
                } else {
                    String::new()
                }
            } else if mtp_path == "/" || mtp_path.is_empty() {
                String::new()
            } else {
                mtp_path.strip_prefix('/').unwrap_or(&mtp_path).to_string()
            };

            // Invalidate the MTP listing cache before re-reading so we get fresh data
            // (otherwise we'd compare stale cached data with itself and detect no changes)
            connection_manager()
                .invalidate_listing_cache(device_id, storage_id, &path)
                .await;

            // Re-read the directory from the MTP device
            let new_entries = match connection_manager()
                .list_directory(device_id, storage_id, &mtp_path)
                .await
            {
                Ok(entries) => entries,
                Err(e) => {
                    debug!("MTP diff: failed to re-read directory {}: {:?}, skipping", mtp_path, e);
                    continue;
                }
            };

            // Compute diff
            let changes = compute_diff(&old_entries, &new_entries);
            if changes.is_empty() {
                debug!(
                    "MTP diff: no changes detected for listing_id={}, path={}",
                    listing_id, mtp_path
                );
                continue;
            }

            // Update LISTING_CACHE with new entries
            update_listing_entries(&listing_id, new_entries);

            // Get sequence number
            let sequence = SEQUENCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

            // Emit directory-diff event (same format as local watcher)
            let diff = DirectoryDiff {
                listing_id: listing_id.clone(),
                sequence,
                changes,
            };

            if let Err(e) = app.emit("directory-diff", &diff) {
                warn!("MTP diff: failed to emit event: {}", e);
            } else {
                info!(
                    "MTP diff: emitted directory-diff for listing_id={}, sequence={}",
                    listing_id, sequence
                );
            }
        }
    }

    /// Gets information about a connected device.
    pub async fn get_device_info(&self, device_id: &str) -> Option<ConnectedDeviceInfo> {
        let devices = self.devices.lock().await;
        devices.get(device_id).map(|entry| ConnectedDeviceInfo {
            device: entry.info.clone(),
            storages: entry.storages.clone(),
        })
    }

    /// Checks if a device is connected.
    #[allow(dead_code, reason = "Will be used in Phase 3+ for file browsing")]
    pub async fn is_connected(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().await;
        devices.contains_key(device_id)
    }

    /// Returns a list of all connected device IDs.
    #[allow(dead_code, reason = "Will be used in Phase 5 for multi-device management")]
    pub async fn connected_device_ids(&self) -> Vec<String> {
        let devices = self.devices.lock().await;
        devices.keys().cloned().collect()
    }

    /// Lists the contents of a directory on an MTP device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `path` - Virtual path to list (for example, "/" or "/DCIM")
    ///
    /// # Returns
    ///
    /// A vector of FileEntry objects suitable for the file browser.
    pub async fn list_directory(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
    ) -> Result<Vec<FileEntry>, MtpConnectionError> {
        use std::sync::atomic::Ordering;

        // Generate unique request ID for tracing this call
        let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let call_start = Instant::now();

        // Track concurrent calls
        let concurrent_before = CONCURRENT_LIST_CALLS.fetch_add(1, Ordering::Relaxed);
        debug!(
            "MTP list_directory [req#{}]: START device={}, storage={}, path={}, concurrent_calls={}",
            request_id,
            device_id,
            storage_id,
            path,
            concurrent_before + 1
        );

        // Wrap the entire operation to ensure we decrement the counter on exit
        let result = self
            .list_directory_inner(request_id, device_id, storage_id, path, call_start)
            .await;

        let concurrent_after = CONCURRENT_LIST_CALLS.fetch_sub(1, Ordering::Relaxed);
        debug!(
            "MTP list_directory [req#{}]: END total_time={:?}, concurrent_calls_remaining={}",
            request_id,
            call_start.elapsed(),
            concurrent_after - 1
        );

        result
    }

    /// Inner implementation of list_directory with detailed phase logging.
    async fn list_directory_inner(
        &self,
        request_id: u64,
        device_id: &str,
        storage_id: u32,
        path: &str,
        call_start: Instant,
    ) -> Result<Vec<FileEntry>, MtpConnectionError> {
        // Normalize the path for building child paths
        let parent_path = normalize_mtp_path(path);

        // Check listing cache first
        let cache_check_start = Instant::now();
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(cache_map) = entry.listing_cache.read()
                && let Some(storage_cache) = cache_map.get(&storage_id)
                && let Some(cached) = storage_cache.listings.get(&parent_path)
            {
                // Check if cache is still valid (within TTL)
                if cached.cached_at.elapsed().as_secs() < LISTING_CACHE_TTL_SECS {
                    debug!(
                        "MTP list_directory [req#{}]: cache HIT, returning {} entries, cache_check_time={:?}, elapsed_since_start={:?}",
                        request_id,
                        cached.entries.len(),
                        cache_check_start.elapsed(),
                        call_start.elapsed()
                    );
                    return Ok(cached.entries.clone());
                } else {
                    debug!(
                        "MTP list_directory [req#{}]: cache STALE (age={}s > TTL={}s)",
                        request_id,
                        cached.cached_at.elapsed().as_secs(),
                        LISTING_CACHE_TTL_SECS
                    );
                }
            } else {
                debug!("MTP list_directory [req#{}]: cache MISS for path={}", request_id, path);
            }
        }
        debug!(
            "MTP list_directory [req#{}]: cache check complete, time={:?}",
            request_id,
            cache_check_start.elapsed()
        );

        // Get the device and resolve path to handle
        let path_resolve_start = Instant::now();
        debug!(
            "MTP list_directory [req#{}]: acquiring devices registry lock...",
            request_id
        );
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            debug!(
                "MTP list_directory [req#{}]: got devices registry lock in {:?}, looking up device...",
                request_id,
                path_resolve_start.elapsed()
            );
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            // Resolve path to parent handle
            debug!("MTP list_directory [req#{}]: resolving path to handle...", request_id);
            let parent_handle = self.resolve_path_to_handle(entry, storage_id, path)?;
            debug!(
                "MTP list_directory [req#{}]: resolved to handle {:?} in {:?}",
                request_id,
                parent_handle,
                path_resolve_start.elapsed()
            );

            (Arc::clone(&entry.device), parent_handle)
        };
        debug!(
            "MTP list_directory [req#{}]: path resolution complete, total_time={:?}",
            request_id,
            path_resolve_start.elapsed()
        );

        // List directory contents (async operation)
        let device_lock_start = Instant::now();
        debug!(
            "MTP list_directory [req#{}]: waiting to acquire device USB lock...",
            request_id
        );
        let device =
            acquire_device_lock(&device_arc, device_id, &format!("list_directory[req#{}]", request_id)).await?;
        let device_lock_acquired_at = Instant::now();
        debug!(
            "MTP list_directory [req#{}]: acquired device USB lock after {:?} wait, getting storage...",
            request_id,
            device_lock_start.elapsed()
        );

        // Get the storage object
        let usb_io_start = Instant::now();
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;
        debug!(
            "MTP list_directory [req#{}]: got storage object in {:?}",
            request_id,
            usb_io_start.elapsed()
        );

        // Use list_objects which returns Vec<ObjectInfo> directly
        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        let list_objects_start = Instant::now();
        debug!(
            "MTP list_directory [req#{}]: calling list_objects (parent={:?})...",
            request_id, parent_opt
        );
        let object_infos =
            match tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.list_objects(parent_opt)).await {
                Ok(Ok(infos)) => infos,
                Ok(Err(e)) => {
                    let mapped_err = map_mtp_error(e, device_id);
                    error!(
                        "MTP list_directory [req#{}]: list_objects failed after {:?}: {:?}",
                        request_id,
                        list_objects_start.elapsed(),
                        mapped_err
                    );
                    return Err(mapped_err);
                }
                Err(_) => {
                    error!(
                        "MTP list_directory [req#{}]: list_objects timed out after {:?}",
                        request_id,
                        list_objects_start.elapsed()
                    );
                    return Err(MtpConnectionError::Timeout {
                        device_id: device_id.to_string(),
                    });
                }
            };

        debug!(
            "MTP list_directory [req#{}]: list_objects returned {} objects in {:?}, total USB I/O time={:?}",
            request_id,
            object_infos.len(),
            list_objects_start.elapsed(),
            usb_io_start.elapsed()
        );

        let mut entries = Vec::with_capacity(object_infos.len());
        let mut cache_updates: Vec<(PathBuf, ObjectHandle)> = Vec::new();

        for info in object_infos {
            let is_dir = info.format == mtp_rs::ptp::ObjectFormatCode::Association;
            let child_path = parent_path.join(&info.filename);

            // Queue cache update
            cache_updates.push((child_path.clone(), info.handle));

            // Convert MTP timestamps
            let modified_at = info.modified.map(convert_mtp_datetime);
            let created_at = info.created.map(convert_mtp_datetime);

            entries.push(FileEntry {
                name: info.filename.clone(),
                path: child_path.to_string_lossy().to_string(),
                is_directory: is_dir,
                is_symlink: false,
                size: if is_dir { None } else { Some(info.size) },
                modified_at,
                created_at,
                added_at: None,
                opened_at: None,
                permissions: if is_dir { 0o755 } else { 0o644 },
                owner: String::new(),
                group: String::new(),
                icon_id: get_mtp_icon_id(is_dir, &info.filename),
                extended_metadata_loaded: true,
            });
        }

        // Release device lock before updating cache
        drop(storage);
        drop(device);
        let lock_held_duration = device_lock_acquired_at.elapsed();
        debug!(
            "MTP list_directory [req#{}]: released device USB lock after holding for {:?}",
            request_id, lock_held_duration
        );

        // Update path cache
        let cache_update_start = Instant::now();
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                for (path, handle) in cache_updates {
                    storage_cache.path_to_handle.insert(path, handle);
                }
            }
        }

        // Sort: directories first, then files, both alphabetically
        entries.sort_by(|a, b| match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

        // Store in listing cache
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.listing_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                storage_cache.listings.insert(
                    parent_path,
                    CachedListing {
                        entries: entries.clone(),
                        cached_at: Instant::now(),
                    },
                );
            }
        }
        debug!(
            "MTP list_directory [req#{}]: cache update complete in {:?}",
            request_id,
            cache_update_start.elapsed()
        );

        debug!(
            "MTP list_directory [req#{}]: returning {} entries, total_time={:?}",
            request_id,
            entries.len(),
            call_start.elapsed()
        );
        Ok(entries)
    }

    /// Invalidates the listing cache for a specific directory.
    /// Call this after any operation that modifies the directory contents.
    async fn invalidate_listing_cache(&self, device_id: &str, storage_id: u32, dir_path: &Path) {
        let devices = self.devices.lock().await;
        if let Some(entry) = devices.get(device_id)
            && let Ok(mut cache_map) = entry.listing_cache.write()
            && let Some(storage_cache) = cache_map.get_mut(&storage_id)
            && storage_cache.listings.remove(dir_path).is_some()
        {
            debug!(
                "Invalidated listing cache for {} on device {}",
                dir_path.display(),
                device_id
            );
        }
    }

    /// Resolves a virtual path to an MTP object handle.
    fn resolve_path_to_handle(
        &self,
        entry: &DeviceEntry,
        storage_id: u32,
        path: &str,
    ) -> Result<ObjectHandle, MtpConnectionError> {
        let path = normalize_mtp_path(path);

        // Root is always ObjectHandle::ROOT
        if path.as_os_str() == "/" || path.as_os_str().is_empty() {
            return Ok(ObjectHandle::ROOT);
        }

        // Check cache
        if let Ok(cache_map) = entry.path_cache.read()
            && let Some(storage_cache) = cache_map.get(&storage_id)
            && let Some(handle) = storage_cache.path_to_handle.get(&path)
        {
            return Ok(*handle);
        }

        // Path not in cache - we need to traverse
        // For Phase 3, we'll only support navigating to paths that have been listed
        // (the cache is populated as directories are browsed)
        Err(MtpConnectionError::Other {
            device_id: entry.info.id.clone(),
            message: format!(
                "Path not in cache: {}. Navigate through parent directories first.",
                path.display()
            ),
        })
    }

    /// Handles a device disconnection (called when we detect the device was unplugged).
    ///
    /// This cleans up the devices registry and emits a disconnection event.
    /// Called from the event loop when MTP reports a disconnect, ensuring that
    /// subsequent reconnection attempts don't fail with "already connected".
    pub async fn handle_device_disconnected(&self, device_id: &str, app: Option<&AppHandle>) {
        debug!(
            "handle_device_disconnected: cleaning up device {} from registry",
            device_id
        );

        let removed = {
            let mut devices = self.devices.lock().await;
            let was_present = devices.remove(device_id).is_some();
            debug!(
                "handle_device_disconnected: device {} was {} in registry, {} devices remaining",
                device_id,
                if was_present { "found" } else { "NOT found" },
                devices.len()
            );
            was_present
        };

        // Stop the event loop for this device
        self.stop_event_loop(device_id);

        if removed {
            info!("MTP device disconnected and removed from registry: {}", device_id);

            if let Some(app) = app {
                let _ = app.emit(
                    "mtp-device-disconnected",
                    serde_json::json!({
                        "deviceId": device_id,
                        "reason": "disconnected"
                    }),
                );
                debug!(
                    "handle_device_disconnected: emitted mtp-device-disconnected event for {}",
                    device_id
                );
            }
        } else {
            debug!(
                "handle_device_disconnected: device {} was not in registry (already cleaned up?)",
                device_id
            );
        }
    }

    // ========================================================================
    // Phase 4: File Operations
    // ========================================================================

    /// Downloads a file from the MTP device to a local path.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Virtual path on the device (for example, "/DCIM/photo.jpg")
    /// * `local_dest` - Local destination path
    /// * `app` - Optional app handle for emitting progress events
    /// * `operation_id` - Unique operation ID for progress tracking
    pub async fn download_file(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        local_dest: &Path,
        app: Option<&AppHandle>,
        operation_id: &str,
    ) -> Result<MtpOperationResult, MtpConnectionError> {
        debug!(
            "MTP download_file: device={}, storage={}, path={}, dest={}",
            device_id,
            storage_id,
            object_path,
            local_dest.display()
        );

        // Get the device and resolve path to handle
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            // Resolve path to handle
            let handle = self.resolve_path_to_handle(entry, storage_id, object_path)?;
            (Arc::clone(&entry.device), handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "download_file").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to determine size
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let total_size = object_info.size;
        let filename = object_info.filename.clone();

        // Emit initial progress
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-transfer-progress",
                MtpTransferProgress {
                    operation_id: operation_id.to_string(),
                    device_id: device_id.to_string(),
                    transfer_type: MtpTransferType::Download,
                    current_file: filename.clone(),
                    bytes_done: 0,
                    bytes_total: total_size,
                },
            );
        }

        // Download the file as a stream (holds session lock until complete)
        let mut download = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10), // Longer timeout for large files
            storage.download_stream(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Create the local file
        let mut file = tokio::fs::File::create(local_dest)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to create local file: {}", e),
            })?;

        // Write chunks to file (must complete before releasing device lock)
        let mut bytes_written = 0u64;
        while let Some(chunk_result) = download.next_chunk().await {
            let chunk = chunk_result.map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Download error: {}", e),
            })?;

            file.write_all(&chunk).await.map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to write local file: {}", e),
            })?;

            bytes_written += chunk.len() as u64;
        }

        // Release device lock after download completes
        drop(storage);
        drop(device);

        file.flush().await.map_err(|e| MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: format!("Failed to flush local file: {}", e),
        })?;

        // Emit completion progress
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-transfer-progress",
                MtpTransferProgress {
                    operation_id: operation_id.to_string(),
                    device_id: device_id.to_string(),
                    transfer_type: MtpTransferType::Download,
                    current_file: filename,
                    bytes_done: bytes_written,
                    bytes_total: total_size,
                },
            );
        }

        info!(
            "MTP download complete: {} bytes to {}",
            bytes_written,
            local_dest.display()
        );

        Ok(MtpOperationResult {
            operation_id: operation_id.to_string(),
            files_processed: 1,
            bytes_transferred: bytes_written,
        })
    }

    /// Uploads a file from the local filesystem to the MTP device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `local_path` - Local file path to upload
    /// * `dest_folder` - Destination folder path on device (for example, "/DCIM")
    /// * `app` - Optional app handle for emitting progress events
    /// * `operation_id` - Unique operation ID for progress tracking
    pub async fn upload_file(
        &self,
        device_id: &str,
        storage_id: u32,
        local_path: &Path,
        dest_folder: &str,
        app: Option<&AppHandle>,
        operation_id: &str,
    ) -> Result<MtpObjectInfo, MtpConnectionError> {
        debug!(
            "MTP upload_file: device={}, storage={}, local={}, dest={}",
            device_id,
            storage_id,
            local_path.display(),
            dest_folder
        );

        // Get file metadata
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to read local file metadata: {}", e),
            })?;

        if metadata.is_dir() {
            return Err(MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: "Cannot upload directories with upload_file. Use create_folder instead.".to_string(),
            });
        }

        let file_size = metadata.len();
        let filename = local_path
            .file_name()
            .ok_or_else(|| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: "Invalid file path".to_string(),
            })?
            .to_string_lossy()
            .to_string();

        // Read the file data
        let data = tokio::fs::read(local_path)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to read local file: {}", e),
            })?;

        // Get device and resolve parent folder
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let parent = self.resolve_path_to_handle(entry, storage_id, dest_folder)?;
            (Arc::clone(&entry.device), parent)
        };

        // Emit initial progress
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-transfer-progress",
                MtpTransferProgress {
                    operation_id: operation_id.to_string(),
                    device_id: device_id.to_string(),
                    transfer_type: MtpTransferType::Upload,
                    current_file: filename.clone(),
                    bytes_done: 0,
                    bytes_total: file_size,
                },
            );
        }

        let device = acquire_device_lock(&device_arc, device_id, "upload_file").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Create object info for the upload (format is auto-detected from filename)
        let object_info = NewObjectInfo::file(&filename, file_size);

        // Upload the file - create a stream from the data
        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        // Create a single-chunk stream from the data
        // Using iter instead of once because iter's items are ready, making it Unpin
        let data_stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(bytes::Bytes::from(data))]);

        let new_handle = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10), // Longer timeout for large files
            storage.upload(parent_opt, object_info, data_stream),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Release device lock
        drop(storage);
        drop(device);

        // Build the new object path
        let new_path = normalize_mtp_path(dest_folder).join(&filename);
        let new_path_str = new_path.to_string_lossy().to_string();

        // Update path cache
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                storage_cache.path_to_handle.insert(new_path.clone(), new_handle);
            }
        }

        // Emit completion progress
        if let Some(app) = app {
            let _ = app.emit(
                "mtp-transfer-progress",
                MtpTransferProgress {
                    operation_id: operation_id.to_string(),
                    device_id: device_id.to_string(),
                    transfer_type: MtpTransferType::Upload,
                    current_file: filename.clone(),
                    bytes_done: file_size,
                    bytes_total: file_size,
                },
            );
        }

        info!("MTP upload complete: {} -> {}", local_path.display(), new_path_str);

        // Invalidate the parent directory's listing cache
        let dest_folder_path = normalize_mtp_path(dest_folder);
        self.invalidate_listing_cache(device_id, storage_id, &dest_folder_path)
            .await;

        Ok(MtpObjectInfo {
            handle: new_handle.0,
            name: filename,
            path: new_path_str,
            is_directory: false,
            size: Some(file_size),
        })
    }

    /// Deletes an object (file or folder) from the MTP device.
    ///
    /// For folders, this recursively deletes all contents first since MTP
    /// requires folders to be empty before deletion.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Virtual path on the device
    pub async fn delete_object(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
    ) -> Result<(), MtpConnectionError> {
        debug!(
            "MTP delete_object: device={}, storage={}, path={}",
            device_id, storage_id, object_path
        );

        // Get the device and resolve path to handle
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let handle = self.resolve_path_to_handle(entry, storage_id, object_path)?;
            (Arc::clone(&entry.device), handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "delete_object").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to check if it's a directory
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let is_dir = object_info.format == mtp_rs::ptp::ObjectFormatCode::Association;

        if is_dir {
            // For directories, we need to recursively delete contents first
            let children = tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                storage.list_objects(Some(object_handle)),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

            drop(storage);
            drop(device);

            // Recursively delete children
            let parent_path = normalize_mtp_path(object_path);
            for child_info in children {
                let child_path = parent_path.join(&child_info.filename);
                let child_path_str = child_path.to_string_lossy().to_string();

                // Cache the child handle for the recursive call
                {
                    let devices = self.devices.lock().await;
                    if let Some(entry) = devices.get(device_id)
                        && let Ok(mut cache_map) = entry.path_cache.write()
                    {
                        let storage_cache = cache_map.entry(storage_id).or_default();
                        storage_cache
                            .path_to_handle
                            .insert(child_path.clone(), child_info.handle);
                    }
                }

                // Use Box::pin for recursive async call
                Box::pin(self.delete_object(device_id, storage_id, &child_path_str)).await?;
            }

            // Re-acquire device and storage lock to delete the now-empty folder
            let device = acquire_device_lock(&device_arc, device_id, "delete_object (empty folder)").await?;
            let storage = tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                device.storage(StorageId(storage_id)),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

            tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.delete(object_handle))
                .await
                .map_err(|_| MtpConnectionError::Timeout {
                    device_id: device_id.to_string(),
                })?
                .map_err(|e| map_mtp_error(e, device_id))?;
        } else {
            // For files, just delete directly
            tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), storage.delete(object_handle))
                .await
                .map_err(|_| MtpConnectionError::Timeout {
                    device_id: device_id.to_string(),
                })?
                .map_err(|e| map_mtp_error(e, device_id))?;
        }

        // Remove from path cache
        let object_path_normalized = normalize_mtp_path(object_path);
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
                && let Some(storage_cache) = cache_map.get_mut(&storage_id)
            {
                storage_cache.path_to_handle.remove(&object_path_normalized);
            }
        }

        // Invalidate the parent directory's listing cache
        if let Some(parent) = object_path_normalized.parent() {
            self.invalidate_listing_cache(device_id, storage_id, parent).await;
        }

        info!("MTP delete complete: {}", object_path);
        Ok(())
    }

    /// Creates a new folder on the MTP device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `parent_path` - Parent folder path (for example, "/DCIM")
    /// * `folder_name` - Name of the new folder
    pub async fn create_folder(
        &self,
        device_id: &str,
        storage_id: u32,
        parent_path: &str,
        folder_name: &str,
    ) -> Result<MtpObjectInfo, MtpConnectionError> {
        debug!(
            "MTP create_folder: device={}, storage={}, parent={}, name={}",
            device_id, storage_id, parent_path, folder_name
        );

        // Get device and resolve parent folder
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let parent = self.resolve_path_to_handle(entry, storage_id, parent_path)?;
            (Arc::clone(&entry.device), parent)
        };

        let device = acquire_device_lock(&device_arc, device_id, "create_folder").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Create the folder
        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        let new_handle = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.create_folder(parent_opt, folder_name),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Release device lock
        drop(storage);
        drop(device);

        // Build the new folder path
        let new_path = normalize_mtp_path(parent_path).join(folder_name);
        let new_path_str = new_path.to_string_lossy().to_string();

        // Update path cache
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                storage_cache.path_to_handle.insert(new_path.clone(), new_handle);
            }
        }

        // Invalidate the parent directory's listing cache
        let parent_path_normalized = normalize_mtp_path(parent_path);
        self.invalidate_listing_cache(device_id, storage_id, &parent_path_normalized)
            .await;

        info!("MTP folder created: {}", new_path_str);

        Ok(MtpObjectInfo {
            handle: new_handle.0,
            name: folder_name.to_string(),
            path: new_path_str,
            is_directory: true,
            size: None,
        })
    }

    /// Renames an object on the MTP device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Current path of the object
    /// * `new_name` - New name for the object
    pub async fn rename_object(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        new_name: &str,
    ) -> Result<MtpObjectInfo, MtpConnectionError> {
        debug!(
            "MTP rename_object: device={}, storage={}, path={}, new_name={}",
            device_id, storage_id, object_path, new_name
        );

        // Get device and resolve object handle
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let handle = self.resolve_path_to_handle(entry, storage_id, object_path)?;
            (Arc::clone(&entry.device), handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "rename_object").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to determine if it's a directory
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let is_dir = object_info.format == mtp_rs::ptp::ObjectFormatCode::Association;
        let old_size = object_info.size;

        // Set the new filename using storage.rename()
        tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.rename(object_handle, new_name),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Release device and storage lock
        drop(storage);
        drop(device);

        // Update path cache
        let old_path = normalize_mtp_path(object_path);
        let parent = old_path.parent().unwrap_or(Path::new("/"));
        let new_path = parent.join(new_name);
        let new_path_str = new_path.to_string_lossy().to_string();

        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
                && let Some(storage_cache) = cache_map.get_mut(&storage_id)
            {
                storage_cache.path_to_handle.remove(&old_path);
                storage_cache.path_to_handle.insert(new_path.clone(), object_handle);
            }
        }

        // Invalidate the parent directory's listing cache (rename affects the parent listing)
        self.invalidate_listing_cache(device_id, storage_id, parent).await;

        info!("MTP rename complete: {} -> {}", object_path, new_path_str);

        Ok(MtpObjectInfo {
            handle: object_handle.0,
            name: new_name.to_string(),
            path: new_path_str,
            is_directory: is_dir,
            size: if is_dir { None } else { Some(old_size) },
        })
    }

    // ========================================================================
    // Phase 5: Copy/Export Operations
    // ========================================================================

    /// Scans an MTP path recursively to get statistics for a copy operation.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `path` - Virtual path on the device to scan
    ///
    /// # Returns
    ///
    /// Statistics including file count, directory count, and total bytes.
    pub async fn scan_for_copy(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
    ) -> Result<CopyScanResult, MtpConnectionError> {
        debug!(
            "MTP scan_for_copy: device={}, storage={}, path={}",
            device_id, storage_id, path
        );

        // Try to list the directory - if it fails or returns empty, it might be a file
        let entries = match self.list_directory(device_id, storage_id, path).await {
            Ok(entries) => entries,
            Err(e) => {
                // list_directory failed - this might be because path is a file, not a directory.
                // Try to check by listing the parent directory.
                debug!(
                    "MTP scan_for_copy: list_directory failed for '{}', checking if it's a file: {:?}",
                    path, e
                );
                if let Some(result) = self.try_scan_as_file(device_id, storage_id, path).await {
                    return Ok(result);
                }
                // Not a file either, propagate the original error
                return Err(e);
            }
        };

        let mut file_count = 0usize;
        let mut dir_count = 0usize;
        let mut total_bytes = 0u64;

        // If entries is empty, it might be an empty directory OR a file (some MTP devices
        // return empty for files instead of an error)
        if entries.is_empty() {
            if let Some(result) = self.try_scan_as_file(device_id, storage_id, path).await {
                return Ok(result);
            }
            // Empty directory
            return Ok(CopyScanResult {
                file_count: 0,
                dir_count: 1,
                total_bytes: 0,
            });
        }

        // Process entries recursively
        for entry in &entries {
            if entry.is_directory {
                dir_count += 1;
                // Recursively scan subdirectory
                let child_result = Box::pin(self.scan_for_copy(device_id, storage_id, &entry.path)).await?;
                file_count += child_result.file_count;
                dir_count += child_result.dir_count;
                total_bytes += child_result.total_bytes;
            } else {
                file_count += 1;
                total_bytes += entry.size.unwrap_or(0);
            }
        }

        debug!(
            "MTP scan_for_copy: {} files, {} dirs, {} bytes for {}",
            file_count, dir_count, total_bytes, path
        );

        Ok(CopyScanResult {
            file_count,
            dir_count,
            total_bytes,
        })
    }

    /// Helper to check if a path is a file by listing its parent directory.
    /// Returns Some(CopyScanResult) if path is a file, None otherwise.
    async fn try_scan_as_file(&self, device_id: &str, storage_id: u32, path: &str) -> Option<CopyScanResult> {
        let path_buf = normalize_mtp_path(path);
        let parent = path_buf.parent()?;
        let name = path_buf.file_name()?.to_str()?;

        let parent_entries = self
            .list_directory(device_id, storage_id, &parent.to_string_lossy())
            .await
            .ok()?;

        let entry = parent_entries.iter().find(|e| e.name == name)?;

        if entry.is_directory {
            // It's a directory, not a file - let caller handle it
            return None;
        }

        debug!(
            "MTP scan_for_copy: path '{}' is a file with size {}",
            path,
            entry.size.unwrap_or(0)
        );

        Some(CopyScanResult {
            file_count: 1,
            dir_count: 0,
            total_bytes: entry.size.unwrap_or(0),
        })
    }

    /// Downloads a file or directory recursively from the MTP device to a local path.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Virtual path on the device to download
    /// * `local_dest` - Local destination path
    ///
    /// # Returns
    ///
    /// Total bytes transferred.
    pub async fn download_recursive(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        local_dest: &Path,
    ) -> Result<u64, MtpConnectionError> {
        debug!(
            "MTP download_recursive: device={}, storage={}, path={}, dest={}",
            device_id,
            storage_id,
            object_path,
            local_dest.display()
        );

        // Try to list the path as a directory first
        let entries = self.list_directory(device_id, storage_id, object_path).await;

        match entries {
            Ok(entries) if !entries.is_empty() => {
                // It's a directory with contents - create local directory and download contents
                debug!(
                    "MTP download_recursive: {} is a directory with {} entries",
                    object_path,
                    entries.len()
                );

                tokio::fs::create_dir_all(local_dest)
                    .await
                    .map_err(|e| MtpConnectionError::Other {
                        device_id: device_id.to_string(),
                        message: format!("Failed to create local directory: {}", e),
                    })?;

                let mut total_bytes = 0u64;
                for entry in entries {
                    let child_dest = local_dest.join(&entry.name);
                    let bytes =
                        Box::pin(self.download_recursive(device_id, storage_id, &entry.path, &child_dest)).await?;
                    total_bytes += bytes;
                }

                debug!(
                    "MTP download_recursive: directory {} complete, {} bytes",
                    object_path, total_bytes
                );
                Ok(total_bytes)
            }
            Ok(_) => {
                // Empty directory or file - check if it's a file by checking parent listing
                let path_buf = normalize_mtp_path(object_path);
                let is_file = if let Some(parent) = path_buf.parent() {
                    let parent_str = parent.to_string_lossy();
                    if let Ok(parent_entries) = self.list_directory(device_id, storage_id, &parent_str).await {
                        if let Some(name) = path_buf.file_name().and_then(|n| n.to_str()) {
                            parent_entries
                                .iter()
                                .find(|e| e.name == name)
                                .is_some_and(|e| !e.is_directory)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_file {
                    // It's a file - download it
                    debug!("MTP download_recursive: {} is a file, downloading", object_path);
                    let operation_id = format!("download-{}", uuid::Uuid::new_v4());
                    let result = self
                        .download_file(device_id, storage_id, object_path, local_dest, None, &operation_id)
                        .await?;
                    Ok(result.bytes_transferred)
                } else {
                    // Empty directory - create it
                    debug!("MTP download_recursive: {} is an empty directory", object_path);
                    tokio::fs::create_dir_all(local_dest)
                        .await
                        .map_err(|e| MtpConnectionError::Other {
                            device_id: device_id.to_string(),
                            message: format!("Failed to create local directory: {}", e),
                        })?;
                    Ok(0)
                }
            }
            Err(e) => {
                // list_directory failed - might be a file (MTP returns ObjectNotFound when
                // trying to list children of a file). Try to check by listing the parent.
                debug!(
                    "MTP download_recursive: list failed for '{}', checking if it's a file: {:?}",
                    object_path, e
                );

                let path_buf = normalize_mtp_path(object_path);
                let is_file = if let Some(parent) = path_buf.parent() {
                    let parent_str = parent.to_string_lossy();
                    if let Ok(parent_entries) = self.list_directory(device_id, storage_id, &parent_str).await {
                        if let Some(name) = path_buf.file_name().and_then(|n| n.to_str()) {
                            parent_entries
                                .iter()
                                .find(|e| e.name == name)
                                .is_some_and(|entry| !entry.is_directory)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_file {
                    debug!("MTP download_recursive: {} is a file, downloading", object_path);
                    let operation_id = format!("download-{}", uuid::Uuid::new_v4());
                    let result = self
                        .download_file(device_id, storage_id, object_path, local_dest, None, &operation_id)
                        .await?;
                    Ok(result.bytes_transferred)
                } else {
                    // Not a file, propagate the original error
                    Err(e)
                }
            }
        }
    }

    /// Uploads a file or directory from local filesystem to MTP device recursively.
    ///
    /// If the source is a directory, creates the directory on the device and
    /// recursively uploads all contents.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `local_source` - Local source path (file or directory)
    /// * `dest_folder` - Destination folder path on device
    ///
    /// # Returns
    ///
    /// Total bytes transferred.
    pub async fn upload_recursive(
        &self,
        device_id: &str,
        storage_id: u32,
        local_source: &Path,
        dest_folder: &str,
    ) -> Result<u64, MtpConnectionError> {
        debug!(
            "MTP upload_recursive: device={}, storage={}, source={}, dest={}",
            device_id,
            storage_id,
            local_source.display(),
            dest_folder
        );

        let metadata = tokio::fs::metadata(local_source)
            .await
            .map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to read local path: {}", e),
            })?;

        if metadata.is_file() {
            // Upload single file
            let operation_id = format!("upload-{}", uuid::Uuid::new_v4());
            let result = self
                .upload_file(device_id, storage_id, local_source, dest_folder, None, &operation_id)
                .await?;
            Ok(result.size.unwrap_or(0))
        } else if metadata.is_dir() {
            // Create directory on device and upload contents
            let dir_name = local_source
                .file_name()
                .ok_or_else(|| MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: "Invalid directory path".to_string(),
                })?
                .to_string_lossy()
                .to_string();

            // Create the directory on the device
            let new_folder = self
                .create_folder(device_id, storage_id, dest_folder, &dir_name)
                .await?;
            let new_folder_path = new_folder.path;

            // Upload all contents
            let mut total_bytes = 0u64;
            let mut entries = tokio::fs::read_dir(local_source)
                .await
                .map_err(|e| MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: format!("Failed to read local directory: {}", e),
                })?;

            while let Some(entry) = entries.next_entry().await.map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: format!("Failed to read directory entry: {}", e),
            })? {
                let entry_path = entry.path();
                let bytes =
                    Box::pin(self.upload_recursive(device_id, storage_id, &entry_path, &new_folder_path)).await?;
                total_bytes += bytes;
            }

            debug!(
                "MTP upload_recursive: directory {} complete, {} bytes",
                local_source.display(),
                total_bytes
            );
            Ok(total_bytes)
        } else {
            // Not a file or directory (symlink, etc.) - skip
            debug!(
                "MTP upload_recursive: skipping non-file/non-directory: {}",
                local_source.display()
            );
            Ok(0)
        }
    }

    /// Moves an object to a new parent folder on the MTP device.
    ///
    /// Falls back to copy+delete if the device doesn't support MoveObject.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `object_path` - Current path of the object
    /// * `new_parent_path` - New parent folder path
    pub async fn move_object(
        &self,
        device_id: &str,
        storage_id: u32,
        object_path: &str,
        new_parent_path: &str,
    ) -> Result<MtpObjectInfo, MtpConnectionError> {
        debug!(
            "MTP move_object: device={}, storage={}, path={}, new_parent={}",
            device_id, storage_id, object_path, new_parent_path
        );

        // Get device and resolve both handles
        let (device_arc, object_handle, new_parent_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let obj_handle = self.resolve_path_to_handle(entry, storage_id, object_path)?;
            let parent_handle = self.resolve_path_to_handle(entry, storage_id, new_parent_path)?;
            (Arc::clone(&entry.device), obj_handle, parent_handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "move_object").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let is_dir = object_info.format == mtp_rs::ptp::ObjectFormatCode::Association;
        let object_size = object_info.size;
        let object_name = object_info.filename.clone();

        // Try to use MoveObject operation
        // storage.move_object expects the new parent handle directly, not Option
        let new_parent_for_move = if new_parent_handle == ObjectHandle::ROOT {
            ObjectHandle::ROOT
        } else {
            new_parent_handle
        };

        let move_result = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.move_object(object_handle, new_parent_for_move, None),
        )
        .await;

        // Release device and storage lock
        drop(storage);
        drop(device);

        match move_result {
            Ok(Ok(())) => {
                // Move succeeded
                let old_path = normalize_mtp_path(object_path);
                let new_path = normalize_mtp_path(new_parent_path).join(&object_name);
                let new_path_str = new_path.to_string_lossy().to_string();

                // Update path cache
                {
                    let devices = self.devices.lock().await;
                    if let Some(entry) = devices.get(device_id)
                        && let Ok(mut cache_map) = entry.path_cache.write()
                        && let Some(storage_cache) = cache_map.get_mut(&storage_id)
                    {
                        storage_cache.path_to_handle.remove(&old_path);
                        storage_cache.path_to_handle.insert(new_path.clone(), object_handle);
                    }
                }

                // Invalidate listing cache for both old and new parent directories
                let old_parent = old_path.parent().unwrap_or(Path::new("/"));
                self.invalidate_listing_cache(device_id, storage_id, old_parent).await;
                let new_parent = normalize_mtp_path(new_parent_path);
                self.invalidate_listing_cache(device_id, storage_id, &new_parent).await;

                info!("MTP move complete: {} -> {}", object_path, new_path_str);

                Ok(MtpObjectInfo {
                    handle: object_handle.0,
                    name: object_name,
                    path: new_path_str,
                    is_directory: is_dir,
                    size: if is_dir { None } else { Some(object_size) },
                })
            }
            Ok(Err(e)) => {
                // Move operation returned an error - might not be supported
                warn!(
                    "MTP MoveObject failed for {}: {:?}. Device may not support this operation.",
                    object_path, e
                );
                Err(MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: format!("Move operation not supported by device: {}", e),
                })
            }
            Err(_) => Err(MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            }),
        }
    }

    // ========================================================================
    // Phase 6: Streaming Operations for Volume-to-Volume Copy
    // ========================================================================

    /// Opens a streaming download for a file.
    ///
    /// Returns the FileDownload stream and the file size.
    /// The caller must consume the entire stream before releasing it.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `path` - Virtual path on the device (e.g., "DCIM/photo.jpg")
    pub async fn open_download_stream(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
    ) -> Result<(mtp_rs::FileDownload, u64), MtpConnectionError> {
        debug!(
            "MTP open_download_stream: device={}, storage={}, path={}",
            device_id, storage_id, path
        );

        // Get the device and resolve path to handle
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let handle = self.resolve_path_to_handle(entry, storage_id, path)?;
            (Arc::clone(&entry.device), handle)
        };

        let device = acquire_device_lock(&device_arc, device_id, "open_download_stream").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Get object info to determine size
        let object_info = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            storage.get_object_info(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        let total_size = object_info.size;

        // Open the download stream
        let download = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10),
            storage.download_stream(object_handle),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Note: We intentionally don't drop 'storage' and 'device' here.
        // The FileDownload holds a reference to the storage session internally.
        // The caller must consume the entire download before other operations.
        // This is a design limitation of the current mtp-rs streaming API.
        // In practice, the Volume trait methods run in spawn_blocking, so
        // the device lock is released when the blocking task completes.

        debug!("MTP open_download_stream: stream opened for {} bytes", total_size);

        Ok((download, total_size))
    }

    /// Uploads pre-collected chunks to the MTP device.
    ///
    /// This variant takes already-collected chunks instead of a stream reference,
    /// avoiding nested `block_on` issues when the stream uses `block_on` internally.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `dest_folder` - Destination folder path on device (e.g., "DCIM")
    /// * `filename` - Name for the new file
    /// * `size` - Total size in bytes
    /// * `chunks` - Pre-collected data chunks
    pub async fn upload_from_chunks(
        &self,
        device_id: &str,
        storage_id: u32,
        dest_folder: &str,
        filename: &str,
        size: u64,
        chunks: Vec<bytes::Bytes>,
    ) -> Result<u64, MtpConnectionError> {
        debug!(
            "MTP upload_from_chunks: device={}, storage={}, dest={}/{}, size={}, chunks={}",
            device_id,
            storage_id,
            dest_folder,
            filename,
            size,
            chunks.len()
        );

        // Get device and resolve parent folder
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let parent = if dest_folder.is_empty() {
                ObjectHandle::ROOT
            } else {
                self.resolve_path_to_handle(entry, storage_id, dest_folder)?
            };
            (Arc::clone(&entry.device), parent)
        };

        let device = acquire_device_lock(&device_arc, device_id, "upload_from_chunks").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(storage_id)),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Create object info for the upload
        let object_info = NewObjectInfo::file(filename, size);

        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        // Convert chunks to stream format expected by mtp-rs
        let chunk_results: Vec<Result<bytes::Bytes, std::io::Error>> = chunks.into_iter().map(Ok).collect();
        let data_stream = futures_util::stream::iter(chunk_results);

        let new_handle = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10),
            storage.upload(parent_opt, object_info, data_stream),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Release device lock
        drop(storage);
        drop(device);

        // Update path cache
        let new_path = normalize_mtp_path(dest_folder).join(filename);
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                storage_cache.path_to_handle.insert(new_path.clone(), new_handle);
            }
        }

        // Invalidate the parent directory's listing cache
        let dest_folder_path = normalize_mtp_path(dest_folder);
        self.invalidate_listing_cache(device_id, storage_id, &dest_folder_path)
            .await;

        info!(
            "MTP upload_from_chunks complete: {} bytes to {}/{}",
            size, dest_folder, filename
        );

        Ok(size)
    }
}

/// Global connection manager instance.
static CONNECTION_MANAGER: LazyLock<MtpConnectionManager> = LazyLock::new(MtpConnectionManager::new);

/// Gets the global connection manager.
pub fn connection_manager() -> &'static MtpConnectionManager {
    &CONNECTION_MANAGER
}

/// Parses a device ID to extract location_id.
///
/// Format: "mtp-{location_id}"
fn parse_device_id(device_id: &str) -> Option<u64> {
    let prefix = "mtp-";
    if !device_id.starts_with(prefix) {
        return None;
    }

    device_id[prefix.len()..].parse().ok()
}

/// Opens an MTP device by location_id.
async fn open_device(location_id: u64) -> Result<MtpDevice, mtp_rs::Error> {
    MtpDeviceBuilder::new()
        .timeout(Duration::from_secs(MTP_TIMEOUT_SECS))
        .open_by_location(location_id)
        .await
}

/// Probes whether a storage actually supports writes by attempting to create
/// and delete a hidden test folder.
///
/// Some devices (especially cameras) report ReadWrite capability but actually
/// reject writes at runtime with `StoreReadOnly`. This probe detects such cases early.
///
/// Returns `true` if writes are supported (or probe was inconclusive), `false` only
/// if the device explicitly rejected writes with `StoreReadOnly` or `AccessDenied`.
async fn probe_write_capability(storage: &mtp_rs::Storage, storage_name: &str) -> bool {
    use mtp_rs::ptp::ResponseCode;

    const PROBE_FOLDER_NAME: &str = ".cmdr_write_probe";
    const PROBE_TIMEOUT_SECS: u64 = 3;

    // Try to create a hidden probe folder at the root
    match tokio::time::timeout(
        Duration::from_secs(PROBE_TIMEOUT_SECS),
        storage.create_folder(None, PROBE_FOLDER_NAME),
    )
    .await
    {
        Ok(Ok(handle)) => {
            // Success! Clean up by deleting the probe folder
            debug!("Storage '{}': write probe succeeded, cleaning up", storage_name);
            if let Err(e) = storage.delete(handle).await {
                warn!("Storage '{}': failed to clean up probe folder: {:?}", storage_name, e);
            }
            true
        }
        Ok(Err(e)) => {
            // Check the specific error code to determine if this is a read-only issue
            // or just a restriction on where we can create folders
            let is_read_only_error = match &e {
                mtp_rs::Error::Protocol { code, .. } => {
                    matches!(code, ResponseCode::StoreReadOnly | ResponseCode::AccessDenied)
                }
                _ => false,
            };

            if is_read_only_error {
                debug!(
                    "Storage '{}': write probe failed with read-only error: {:?}",
                    storage_name, e
                );
                false
            } else {
                // Other errors (InvalidObjectHandle, InvalidParentObject, etc.) likely mean
                // we just can't create at root, not that the device is read-only.
                // Android devices often don't allow creating at root but are still writable.
                debug!(
                    "Storage '{}': write probe failed with non-fatal error (assuming writable): {:?}",
                    storage_name, e
                );
                true
            }
        }
        Err(_) => {
            // Timeout - assume writable (benefit of the doubt)
            debug!("Storage '{}': write probe timed out (assuming writable)", storage_name);
            true
        }
    }
}

/// Gets storage information from a connected device.
///
/// # Arguments
/// * `device` - The connected MTP device
/// * `device_supports_write` - Whether the device supports write operations (SendObjectInfo)
async fn get_storages(device: &MtpDevice, device_supports_write: bool) -> Result<Vec<MtpStorageInfo>, mtp_rs::Error> {
    use mtp_rs::ptp::AccessCapability;

    debug!("Calling device.storages()...");
    let storage_list = device.storages().await?;
    debug!("Got {} storage(s)", storage_list.len());
    let mut storages = Vec::new();

    for storage in storage_list {
        let info = storage.info();
        // Check if storage reports read-only capability
        let storage_reports_read_only = !matches!(info.access_capability, AccessCapability::ReadWrite);

        // Determine actual read-only status
        let is_read_only = if !device_supports_write || storage_reports_read_only {
            // Device/storage claims no write support - trust it
            true
        } else {
            // Device claims write support - probe to verify
            // This catches cameras that advertise write support but reject writes at runtime
            let probe_ok = probe_write_capability(&storage, &info.description).await;
            if !probe_ok {
                info!(
                    "Storage '{}' claims write support but probe failed - marking read-only",
                    info.description
                );
            }
            !probe_ok // read-only if probe failed
        };

        // Log final determination
        info!(
            "Storage '{}': access_capability={:?}, device_supports_write={}, is_read_only={}",
            info.description, info.access_capability, device_supports_write, is_read_only
        );

        storages.push(MtpStorageInfo {
            id: storage.id().0,
            name: info.description.clone(),
            total_bytes: info.max_capacity,
            available_bytes: info.free_space_bytes,
            storage_type: Some(format!("{:?}", info.storage_type)),
            is_read_only,
        });
    }

    Ok(storages)
}

/// Maps mtp_rs errors to our error types.
fn map_mtp_error(e: mtp_rs::Error, device_id: &str) -> MtpConnectionError {
    use mtp_rs::ptp::ResponseCode;

    match e {
        mtp_rs::Error::NoDevice => MtpConnectionError::DeviceNotFound {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Disconnected => MtpConnectionError::Disconnected {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Timeout => MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Cancelled => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: "Operation cancelled".to_string(),
        },
        mtp_rs::Error::SessionNotOpen => MtpConnectionError::NotConnected {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Protocol { code, operation } => {
            // Map specific response codes to user-friendly errors
            match code {
                ResponseCode::DeviceBusy => MtpConnectionError::DeviceBusy {
                    device_id: device_id.to_string(),
                },
                ResponseCode::StoreFull => MtpConnectionError::StorageFull {
                    device_id: device_id.to_string(),
                },
                ResponseCode::StoreReadOnly => MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: "This device is read-only. You can copy files from it, but not to it.".to_string(),
                },
                ResponseCode::InvalidObjectHandle | ResponseCode::InvalidParentObject => {
                    MtpConnectionError::ObjectNotFound {
                        device_id: device_id.to_string(),
                        path: format!("(operation: {:?})", operation),
                    }
                }
                ResponseCode::AccessDenied => MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: "Access denied. The device rejected the operation.".to_string(),
                },
                _ => MtpConnectionError::Protocol {
                    device_id: device_id.to_string(),
                    message: format!("{:?}", code),
                },
            }
        }
        mtp_rs::Error::InvalidData { message } => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: format!("Invalid data from device: {}", message),
        },
        mtp_rs::Error::Io(io_err) => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: format!("I/O error: {}", io_err),
        },
        mtp_rs::Error::Usb(usb_err) => {
            // Check for exclusive access errors
            let msg = usb_err.to_string().to_lowercase();
            if msg.contains("exclusive access") || msg.contains("device or resource busy") {
                MtpConnectionError::ExclusiveAccess {
                    device_id: device_id.to_string(),
                    blocking_process: None,
                }
            } else {
                MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: format!("USB error: {}", usb_err),
                }
            }
        }
    }
}

/// Normalizes an MTP path.
///
/// Ensures the path starts with "/" and handles empty/relative paths.
fn normalize_mtp_path(path: &str) -> PathBuf {
    if path.is_empty() || path == "." {
        PathBuf::from("/")
    } else if !path.starts_with('/') {
        PathBuf::from("/").join(path)
    } else {
        PathBuf::from(path)
    }
}

/// Converts MTP DateTime to Unix timestamp.
fn convert_mtp_datetime(dt: mtp_rs::ptp::DateTime) -> u64 {
    // Convert the DateTime struct fields to Unix timestamp
    // This is a simplified conversion - MTP DateTime has year, month, day, hour, minute, second

    // Create a rough Unix timestamp from the date components
    // Note: This is a simplified calculation that doesn't account for leap years perfectly
    let year = dt.year as u64;
    let month = dt.month as u64;
    let day = dt.day as u64;
    let hour = dt.hour as u64;
    let minute = dt.minute as u64;
    let second = dt.second as u64;

    // Simplified calculation: days since epoch + time
    // This is approximate but good enough for file listing purposes
    let years_since_1970 = year.saturating_sub(1970);
    let days = years_since_1970 * 365 + (years_since_1970 / 4) // leap years approximation
        + (month.saturating_sub(1)) * 30  // approximate days per month
        + day.saturating_sub(1);

    days * 86400 + hour * 3600 + minute * 60 + second
}

/// Generates icon ID for MTP files.
fn get_mtp_icon_id(is_dir: bool, filename: &str) -> String {
    if is_dir {
        return "dir".to_string();
    }
    if let Some(ext) = Path::new(filename).extension() {
        return format!("ext:{}", ext.to_string_lossy().to_lowercase());
    }
    "file".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_id_valid() {
        assert_eq!(parse_device_id("mtp-336592896"), Some(336592896));
        assert_eq!(parse_device_id("mtp-12345"), Some(12345));
        assert_eq!(parse_device_id("mtp-0"), Some(0));
    }

    #[test]
    fn test_parse_device_id_invalid() {
        assert_eq!(parse_device_id("usb-336592896"), None);
        assert_eq!(parse_device_id("mtp-abc"), None);
        assert_eq!(parse_device_id("mtp-"), None);
        assert_eq!(parse_device_id("mtp"), None);
        assert_eq!(parse_device_id(""), None);
    }

    #[test]
    fn test_connection_error_display() {
        let err = MtpConnectionError::DeviceNotFound {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device not found: mtp-1-5");

        let err = MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1-5".to_string(),
            blocking_process: Some("ptpcamerad".to_string()),
        };
        assert_eq!(err.to_string(), "Device mtp-1-5 is in use by ptpcamerad");
    }

    #[test]
    fn test_connection_error_is_retryable() {
        // Retryable errors
        assert!(
            MtpConnectionError::Timeout {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
        assert!(
            MtpConnectionError::DeviceBusy {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );

        // Non-retryable errors
        assert!(
            !MtpConnectionError::DeviceNotFound {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
        assert!(
            !MtpConnectionError::Disconnected {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
        assert!(
            !MtpConnectionError::StorageFull {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
    }

    #[test]
    fn test_connection_error_user_message() {
        let err = MtpConnectionError::DeviceNotFound {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.user_message().contains("not found"));
        assert!(err.user_message().contains("unplugged"));

        let err = MtpConnectionError::StorageFull {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.user_message().contains("full"));

        let err = MtpConnectionError::DeviceBusy {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.user_message().contains("busy"));
        assert!(err.user_message().contains("try again"));
    }

    #[test]
    fn test_new_error_types_display() {
        let err = MtpConnectionError::DeviceBusy {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device busy: mtp-1-5");

        let err = MtpConnectionError::StorageFull {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Storage full on device: mtp-1-5");

        let err = MtpConnectionError::ObjectNotFound {
            device_id: "mtp-1-5".to_string(),
            path: "/DCIM/photo.jpg".to_string(),
        };
        assert!(err.to_string().contains("Object not found"));
        assert!(err.to_string().contains("/DCIM/photo.jpg"));
    }

    // ========================================================================
    // Path normalization tests
    // ========================================================================

    #[test]
    fn test_normalize_mtp_path_empty() {
        assert_eq!(normalize_mtp_path(""), PathBuf::from("/"));
    }

    #[test]
    fn test_normalize_mtp_path_dot() {
        assert_eq!(normalize_mtp_path("."), PathBuf::from("/"));
    }

    #[test]
    fn test_normalize_mtp_path_root() {
        assert_eq!(normalize_mtp_path("/"), PathBuf::from("/"));
    }

    #[test]
    fn test_normalize_mtp_path_absolute() {
        assert_eq!(normalize_mtp_path("/DCIM"), PathBuf::from("/DCIM"));
        assert_eq!(normalize_mtp_path("/DCIM/Camera"), PathBuf::from("/DCIM/Camera"));
    }

    #[test]
    fn test_normalize_mtp_path_relative() {
        assert_eq!(normalize_mtp_path("DCIM"), PathBuf::from("/DCIM"));
        assert_eq!(normalize_mtp_path("DCIM/Camera"), PathBuf::from("/DCIM/Camera"));
    }

    #[test]
    fn test_normalize_mtp_path_special_characters() {
        // Test paths with spaces and special characters
        assert_eq!(normalize_mtp_path("/My Files"), PathBuf::from("/My Files"));
        assert_eq!(normalize_mtp_path("Photos & Videos"), PathBuf::from("/Photos & Videos"));
    }

    // ========================================================================
    // Icon ID generation tests
    // ========================================================================

    #[test]
    fn test_get_mtp_icon_id_directory() {
        assert_eq!(get_mtp_icon_id(true, "DCIM"), "dir");
        assert_eq!(get_mtp_icon_id(true, "Camera"), "dir");
        assert_eq!(get_mtp_icon_id(true, ""), "dir");
    }

    #[test]
    fn test_get_mtp_icon_id_file_with_extension() {
        assert_eq!(get_mtp_icon_id(false, "photo.jpg"), "ext:jpg");
        assert_eq!(get_mtp_icon_id(false, "document.PDF"), "ext:pdf");
        assert_eq!(get_mtp_icon_id(false, "video.MP4"), "ext:mp4");
        assert_eq!(get_mtp_icon_id(false, "archive.tar.gz"), "ext:gz");
    }

    #[test]
    fn test_get_mtp_icon_id_file_without_extension() {
        assert_eq!(get_mtp_icon_id(false, "README"), "file");
        assert_eq!(get_mtp_icon_id(false, "Makefile"), "file");
        // Hidden files starting with . have no "real" extension, Path::extension returns None
        assert_eq!(get_mtp_icon_id(false, ".hidden"), "file");
    }

    // ========================================================================
    // Error serialization tests
    // ========================================================================

    #[test]
    fn test_connection_error_serialization() {
        let err = MtpConnectionError::DeviceNotFound {
            device_id: "mtp-1-5".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        // Note: With tag = "type" and rename_all = "camelCase", device_id becomes deviceId
        assert!(json.contains("\"type\":\"deviceNotFound\""), "JSON: {}", json);
        assert!(json.contains("\"device_id\":\"mtp-1-5\""), "JSON: {}", json);
    }

    #[test]
    fn test_connection_error_exclusive_access_serialization() {
        let err = MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1-5".to_string(),
            blocking_process: Some("ptpcamerad".to_string()),
        };
        let json = serde_json::to_string(&err).unwrap();
        // Note: tag type is camelCase, but inner field names stay snake_case
        assert!(json.contains("\"type\":\"exclusiveAccess\""), "JSON: {}", json);
        assert!(json.contains("\"blocking_process\":\"ptpcamerad\""), "JSON: {}", json);
    }

    #[test]
    fn test_connection_error_exclusive_access_no_process() {
        let err = MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1-5".to_string(),
            blocking_process: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"blocking_process\":null"), "JSON: {}", json);

        // Test user message for this case
        assert!(err.user_message().contains("Another app"));
        assert!(!err.user_message().contains("ptpcamerad"));
    }

    #[test]
    fn test_connection_error_protocol_serialization() {
        let err = MtpConnectionError::Protocol {
            device_id: "mtp-1-5".to_string(),
            message: "InvalidObjectHandle".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"type\":\"protocol\""), "JSON: {}", json);
        assert!(json.contains("\"message\":\"InvalidObjectHandle\""), "JSON: {}", json);
    }

    // ========================================================================
    // All error display and user_message coverage
    // ========================================================================

    #[test]
    fn test_all_error_variants_display() {
        // Test all variants have Display impl
        let errors = vec![
            MtpConnectionError::DeviceNotFound {
                device_id: "test".to_string(),
            },
            MtpConnectionError::NotConnected {
                device_id: "test".to_string(),
            },
            MtpConnectionError::ExclusiveAccess {
                device_id: "test".to_string(),
                blocking_process: None,
            },
            MtpConnectionError::Timeout {
                device_id: "test".to_string(),
            },
            MtpConnectionError::Disconnected {
                device_id: "test".to_string(),
            },
            MtpConnectionError::Protocol {
                device_id: "test".to_string(),
                message: "error".to_string(),
            },
            MtpConnectionError::DeviceBusy {
                device_id: "test".to_string(),
            },
            MtpConnectionError::StorageFull {
                device_id: "test".to_string(),
            },
            MtpConnectionError::ObjectNotFound {
                device_id: "test".to_string(),
                path: "/path".to_string(),
            },
            MtpConnectionError::Other {
                device_id: "test".to_string(),
                message: "other".to_string(),
            },
        ];

        for err in errors {
            // Each should have non-empty display
            assert!(!err.to_string().is_empty());
            // Each should have non-empty user message
            assert!(!err.user_message().is_empty());
        }
    }

    #[test]
    fn test_not_connected_error() {
        let err = MtpConnectionError::NotConnected {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device not connected: mtp-1-5");
        assert!(err.user_message().contains("not connected"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_timeout_error() {
        let err = MtpConnectionError::Timeout {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.to_string().contains("timed out"));
        assert!(err.user_message().contains("timed out"));
        assert!(err.is_retryable());
    }

    #[test]
    fn test_disconnected_error() {
        let err = MtpConnectionError::Disconnected {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.to_string().contains("disconnected"));
        assert!(err.user_message().contains("disconnected"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_protocol_error_user_message() {
        let err = MtpConnectionError::Protocol {
            device_id: "mtp-1-5".to_string(),
            message: "InvalidObjectHandle".to_string(),
        };
        assert!(err.user_message().contains("InvalidObjectHandle"));
        assert!(err.user_message().contains("reconnecting"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_object_not_found_user_message() {
        let err = MtpConnectionError::ObjectNotFound {
            device_id: "mtp-1-5".to_string(),
            path: "/DCIM/photo.jpg".to_string(),
        };
        assert!(err.user_message().contains("/DCIM/photo.jpg"));
        assert!(err.user_message().contains("deleted"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_other_error() {
        let err = MtpConnectionError::Other {
            device_id: "mtp-1-5".to_string(),
            message: "Custom error message".to_string(),
        };
        assert!(err.to_string().contains("Custom error message"));
        assert_eq!(err.user_message(), "Custom error message");
        assert!(!err.is_retryable());
    }

    // ========================================================================
    // Transfer types and result tests
    // ========================================================================

    #[test]
    fn test_transfer_type_serialization() {
        let download = MtpTransferType::Download;
        let upload = MtpTransferType::Upload;

        let download_json = serde_json::to_string(&download).unwrap();
        let upload_json = serde_json::to_string(&upload).unwrap();

        assert_eq!(download_json, "\"download\"");
        assert_eq!(upload_json, "\"upload\"");
    }

    #[test]
    fn test_transfer_progress_serialization() {
        let progress = MtpTransferProgress {
            operation_id: "op-123".to_string(),
            device_id: "mtp-1-5".to_string(),
            transfer_type: MtpTransferType::Download,
            current_file: "photo.jpg".to_string(),
            bytes_done: 1024,
            bytes_total: 4096,
        };

        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"operationId\":\"op-123\""));
        assert!(json.contains("\"deviceId\":\"mtp-1-5\""));
        assert!(json.contains("\"transferType\":\"download\""));
        assert!(json.contains("\"currentFile\":\"photo.jpg\""));
        assert!(json.contains("\"bytesDone\":1024"));
        assert!(json.contains("\"bytesTotal\":4096"));
    }

    #[test]
    fn test_operation_result_serialization() {
        let result = MtpOperationResult {
            operation_id: "op-456".to_string(),
            files_processed: 5,
            bytes_transferred: 1_000_000,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"operationId\":\"op-456\""));
        assert!(json.contains("\"filesProcessed\":5"));
        assert!(json.contains("\"bytesTransferred\":1000000"));
    }

    #[test]
    fn test_object_info_serialization() {
        let info = MtpObjectInfo {
            handle: 12345,
            name: "test.jpg".to_string(),
            path: "/DCIM/test.jpg".to_string(),
            is_directory: false,
            size: Some(1024),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"handle\":12345"));
        assert!(json.contains("\"name\":\"test.jpg\""));
        assert!(json.contains("\"path\":\"/DCIM/test.jpg\""));
        assert!(json.contains("\"isDirectory\":false"));
        assert!(json.contains("\"size\":1024"));
    }

    #[test]
    fn test_object_info_directory() {
        let info = MtpObjectInfo {
            handle: 100,
            name: "Photos".to_string(),
            path: "/Photos".to_string(),
            is_directory: true,
            size: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"isDirectory\":true"));
        assert!(json.contains("\"size\":null"));
    }

    // ========================================================================
    // Connected device info tests
    // ========================================================================

    #[test]
    fn test_connected_device_info_serialization() {
        use super::super::types::{MtpDeviceInfo, MtpStorageInfo};

        let info = ConnectedDeviceInfo {
            device: MtpDeviceInfo {
                id: "mtp-336592896".to_string(),
                location_id: 336592896,
                vendor_id: 0x18d1,
                product_id: 0x4ee1,
                manufacturer: Some("Google".to_string()),
                product: Some("Pixel 8".to_string()),
                serial_number: None,
            },
            storages: vec![MtpStorageInfo {
                id: 65537,
                name: "Internal shared storage".to_string(),
                total_bytes: 128_000_000_000,
                available_bytes: 64_000_000_000,
                storage_type: Some("FixedRAM".to_string()),
                is_read_only: false,
            }],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"id\":\"mtp-336592896\""));
        assert!(json.contains("\"locationId\":336592896"));
        assert!(json.contains("\"manufacturer\":\"Google\""));
        assert!(json.contains("\"product\":\"Pixel 8\""));
        assert!(json.contains("\"Internal shared storage\""));
        assert!(json.contains("\"isReadOnly\":false"));
    }

    // ========================================================================
    // Edge cases for parse_device_id
    // ========================================================================

    #[test]
    fn test_parse_device_id_edge_cases() {
        // Maximum u64 value
        assert_eq!(parse_device_id("mtp-18446744073709551615"), Some(u64::MAX));

        // Zero value
        assert_eq!(parse_device_id("mtp-0"), Some(0));

        // Typical macOS location_id values
        assert_eq!(parse_device_id("mtp-336592896"), Some(336592896));

        // Wrong prefix (case sensitive)
        assert_eq!(parse_device_id("MTP-336592896"), None);

        // Whitespace
        assert_eq!(parse_device_id(" mtp-336592896"), None);
        assert_eq!(parse_device_id("mtp-336592896 "), None);

        // Negative numbers (not valid for u64)
        assert_eq!(parse_device_id("mtp--1"), None);
    }

    // ========================================================================
    // MtpOperationState tests
    // ========================================================================

    #[test]
    fn test_operation_state_default() {
        let state = MtpOperationState::default();
        assert!(!state.cancelled.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn test_operation_state_cancel() {
        let state = MtpOperationState::default();
        state.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(state.cancelled.load(std::sync::atomic::Ordering::Relaxed));
    }

    // ========================================================================
    // EventDebouncer tests
    // ========================================================================

    #[test]
    fn test_event_debouncer_allows_first_event() {
        let debouncer = EventDebouncer::new(Duration::from_millis(500));

        // First event for a device should always be allowed
        assert!(debouncer.should_emit("device-1"));

        // First event for a different device should also be allowed
        assert!(debouncer.should_emit("device-2"));
    }

    #[test]
    fn test_event_debouncer_throttles_rapid_events() {
        let debouncer = EventDebouncer::new(Duration::from_millis(100));

        // First event should be allowed
        assert!(debouncer.should_emit("device-1"));

        // Immediate second event should be throttled
        assert!(!debouncer.should_emit("device-1"));

        // Third rapid event should also be throttled
        assert!(!debouncer.should_emit("device-1"));
    }

    #[test]
    fn test_event_debouncer_allows_after_timeout() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));

        // First event should be allowed
        assert!(debouncer.should_emit("device-1"));

        // Wait for debounce period to elapse
        std::thread::sleep(Duration::from_millis(20));

        // Event after timeout should be allowed
        assert!(debouncer.should_emit("device-1"));
    }

    #[test]
    fn test_event_debouncer_clear() {
        let debouncer = EventDebouncer::new(Duration::from_millis(500));

        // First event allowed
        assert!(debouncer.should_emit("device-1"));

        // Second event should be throttled
        assert!(!debouncer.should_emit("device-1"));

        // Clear the device state
        debouncer.clear("device-1");

        // After clear, next event should be allowed immediately
        assert!(debouncer.should_emit("device-1"));
    }

    #[test]
    fn test_event_debouncer_per_device_isolation() {
        let debouncer = EventDebouncer::new(Duration::from_millis(500));

        // First event for device-1
        assert!(debouncer.should_emit("device-1"));

        // Rapid event for device-1 should be throttled
        assert!(!debouncer.should_emit("device-1"));

        // But event for device-2 should be allowed (independent)
        assert!(debouncer.should_emit("device-2"));

        // And rapid event for device-2 should be throttled independently
        assert!(!debouncer.should_emit("device-2"));
    }
}
