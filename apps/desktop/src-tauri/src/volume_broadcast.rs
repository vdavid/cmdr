//! Volume list broadcast, cross-platform.
//!
//! Provides a single `emit_volumes_changed()` function that computes the full
//! volume list (local + MTP) and emits a `volumes-changed` Tauri event.
//! All volume-list consumers (volume selector, DualPaneExplorer) subscribe to
//! this one event instead of juggling multiple separate events.
//!
//! A 150ms debounce coalesces rapid events (e.g. multiple mounts in quick
//! succession, or MTP connect immediately after USB hotplug).

use log::{debug, error, warn};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event;

/// Global app handle for emitting events.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Generation counter for debounce. Each call to `emit_volumes_changed()` bumps
/// the counter; the spawned task only emits if its generation is still current.
/// This ensures late-arriving triggers always produce an emission with fresh data.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Debounce window: events within this window are coalesced into one emission.
const DEBOUNCE_MS: u64 = 150;

/// Timeout for listing local volumes. If `list_locations()` takes longer
/// (for example, a hung NFS mount), we emit whatever we have with `timed_out: true`.
const LIST_TIMEOUT: Duration = Duration::from_secs(2);

/// Stores the app handle for later use. Call once during app setup.
pub fn init(app: &AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
}

/// Schedules a `volumes-changed` event emission with debouncing.
///
/// Can be called from any thread. Multiple rapid calls within the debounce
/// window result in a single emission after the window expires. The last
/// call always wins: a late trigger re-bumps the generation so the pending
/// task emits fresh data.
pub fn emit_volumes_changed() {
    use std::sync::atomic::Ordering;

    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    debug!("volumes-changed requested (generation {})", generation);

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;
        // Only emit if no newer request arrived during the sleep
        if GENERATION.load(Ordering::SeqCst) == generation {
            do_emit().await;
        } else {
            debug!("volumes-changed skipped (generation {} superseded)", generation);
        }
    });
}

/// Tauri command: triggers a fresh `volumes-changed` broadcast.
/// The result arrives via the event, not as a return value.
/// Used by the frontend retry button when the initial listing timed out.
#[tauri::command]
#[specta::specta]
pub fn refresh_volumes() {
    emit_volumes_changed_now();
}

/// Emits immediately, bypassing debounce. Used for the initial startup emission.
pub fn emit_volumes_changed_now() {
    tauri::async_runtime::spawn(async {
        do_emit().await;
    });
}

// ============================================================================
// Platform-specific list_locations() dispatch
// ============================================================================

#[cfg(target_os = "macos")]
use crate::volumes::LocationInfo;

#[cfg(target_os = "linux")]
use crate::volumes_linux::LocationInfo;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use crate::stubs::volumes::VolumeInfo as LocationInfo;

/// Typed `volumes-changed` Tauri event. The struct name kebab-cases to the wire
/// event name (`volumes-changed`) via `tauri_specta::Event`. The TS payload type
/// and a typed `events.volumesChanged.listen(...)` helper are generated into
/// `apps/desktop/src/lib/ipc/bindings.ts`.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumesChanged {
    /// The full volume list (local + MTP).
    pub data: Vec<LocationInfo>,
    /// Whether the local volume listing timed out (some volumes may be missing).
    pub timed_out: bool,
}

/// Typed `volume-mounted` Tauri event (per-volume, carries the mount path).
/// Emitted by both the macOS (`NSWorkspace`) and Linux (`/proc/mounts` + GVFS)
/// watchers. The struct name kebab-cases to `volume-mounted`.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeMounted {
    /// The volume path (like "/Volumes/MyDrive").
    pub volume_path: String,
}

/// Typed `volume-unmounted` Tauri event (per-volume, carries the gone path).
/// `DualPaneExplorer` listens for this to redirect panes off ejected volumes.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeUnmounted {
    /// The volume path (like "/Volumes/MyDrive").
    pub volume_path: String,
}

/// Typed `volume-context-action` Tauri event. Emitted to the `main` window when
/// the user picks an action (currently just "eject") from the native breadcrumb /
/// dropdown-row context menu. Window-scoped, so it's emitted via `Event::emit_to`.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeContextAction {
    /// The action id (for example, "eject").
    pub action: String,
    /// The target volume's ID.
    pub volume_id: String,
    /// The target volume's display name (for confirmation copy).
    pub volume_name: String,
}

#[cfg(target_os = "macos")]
fn list_locations() -> Vec<LocationInfo> {
    crate::volumes::list_locations()
}

#[cfg(target_os = "linux")]
fn list_locations() -> Vec<LocationInfo> {
    crate::volumes_linux::list_locations()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn list_locations() -> Vec<LocationInfo> {
    crate::stubs::volumes::list_volumes()
}

// ============================================================================
// MTP volume category
// ============================================================================

#[cfg(target_os = "macos")]
use crate::volumes::LocationCategory;

#[cfg(target_os = "linux")]
use crate::volumes_linux::LocationCategory;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use crate::stubs::volumes::LocationCategory;

// ============================================================================
// Emission
// ============================================================================

/// Computes the full volume list and emits the event.
async fn do_emit() {
    let app = match APP_HANDLE.get() {
        Some(a) => a,
        None => {
            error!("volumes-changed: no app handle (broadcast not initialized)");
            return;
        }
    };

    // Compute local volumes with a timeout (list_locations may block on hung mounts)
    let (local_volumes, timed_out) =
        match tokio::time::timeout(LIST_TIMEOUT, tokio::task::spawn_blocking(list_locations)).await {
            Ok(Ok(vols)) => (vols, false),
            Ok(Err(e)) => {
                error!("volumes-changed: spawn_blocking panicked: {}", e);
                (vec![], false)
            }
            Err(_) => {
                warn!("volumes-changed: list_locations timed out after {:?}", LIST_TIMEOUT);
                (vec![], true)
            }
        };

    // Append MTP volumes
    let mut volumes = local_volumes;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    append_mtp_volumes(&mut volumes).await;

    // Enrich SMB volumes with connection state from VolumeManager
    #[cfg(target_os = "macos")]
    crate::volumes::enrich_smb_connection_state(&mut volumes);

    debug!(
        "Emitting volumes-changed ({} volumes, timed_out={})",
        volumes.len(),
        timed_out
    );
    let payload = VolumesChanged {
        data: volumes,
        timed_out,
    };
    if let Err(e) = payload.emit(app) {
        error!("Failed to emit volumes-changed: {}", e);
    }
}

/// Appends connected MTP device storages to the volume list.
#[cfg(any(target_os = "macos", target_os = "linux"))]
async fn append_mtp_volumes(volumes: &mut Vec<LocationInfo>) {
    let devices = crate::mtp::connection_manager().get_all_connected_devices().await;
    for device in devices {
        let multi = device.storages.len() > 1;
        let device_name = device
            .device
            .product
            .as_deref()
            .or(device.device.manufacturer.as_deref())
            .unwrap_or("Mobile device");
        for storage in &device.storages {
            let name = if multi {
                format!("{} - {}", device_name, storage.name)
            } else {
                device_name.to_string()
            };
            volumes.push(LocationInfo {
                id: format!("{}:{}", device.device.id, storage.id),
                name,
                path: format!("mtp://{}/{}", device.device.id, storage.id),
                category: LocationCategory::MobileDevice,
                icon: None,
                is_ejectable: true,
                is_read_only: storage.is_read_only,
                fs_type: Some("mtp".to_string()),
                supports_trash: false,
                smb_connection_state: None,
                usb_speed: device.device.usb_speed,
            });
        }
    }
}
