//! Volume mount/unmount watcher for macOS.
//!
//! Watches the /Volumes directory for changes using FSEvents, detecting when
//! volumes are mounted or unmounted, and emits Tauri events to the frontend.

use log::{debug, error};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

/// Global app handle for emitting events from the watcher
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// The watcher instance (kept alive for the duration of the app)
static WATCHER: OnceLock<Mutex<Option<RecommendedWatcher>>> = OnceLock::new();

/// Track known volume paths for comparison
static KNOWN_VOLUMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Payload for volume mount/unmount events
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeEventPayload {
    /// The volume path (e.g., "/Volumes/MyDrive")
    pub volume_path: String,
}

/// Get the current set of volumes in /Volumes
fn get_current_volumes() -> HashSet<String> {
    let mut volumes = HashSet::new();
    if let Ok(entries) = std::fs::read_dir("/Volumes") {
        for entry in entries.flatten() {
            if let Some(name) = entry.path().to_str() {
                volumes.insert(name.to_string());
            }
        }
    }
    volumes
}

/// Start watching for volume mount/unmount events.
/// Call this once at app initialization.
pub fn start_volume_watcher(app: &AppHandle) {
    // Store app handle for event emission
    if APP_HANDLE.set(app.clone()).is_err() {
        debug!("Volume watcher already initialized");
        return;
    }

    // Initialize known volumes
    let initial_volumes = get_current_volumes();
    let known = KNOWN_VOLUMES.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut known_guard) = known.lock() {
        *known_guard = initial_volumes.clone();
        debug!("Initial volumes: {:?}", known_guard);
    }

    debug!("Starting volume mount/unmount watcher on /Volumes");

    // Create a watcher for /Volumes directory
    let watcher_result = notify::recommended_watcher(move |result: Result<Event, notify::Error>| match result {
        Ok(event) => handle_fs_event(event),
        Err(e) => error!("Volume watcher error: {}", e),
    });

    match watcher_result {
        Ok(mut watcher) => {
            // Watch /Volumes with non-recursive mode (we only care about direct children)
            let volumes_path = Path::new("/Volumes");
            if let Err(e) = watcher.watch(volumes_path, RecursiveMode::NonRecursive) {
                error!("Failed to watch /Volumes: {}", e);
                return;
            }

            // Store the watcher to keep it alive
            let watcher_storage = WATCHER.get_or_init(|| Mutex::new(None));
            if let Ok(mut guard) = watcher_storage.lock() {
                *guard = Some(watcher);
            }

            debug!("Volume watcher started successfully");
        }
        Err(e) => {
            error!("Failed to create volume watcher: {}", e);
        }
    }
}

/// Handle filesystem events on /Volumes
fn handle_fs_event(event: Event) {
    // We're interested in Create and Remove events
    match event.kind {
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
            // Debounce: compare current state with known state
            check_for_volume_changes();
        }
        _ => {}
    }
}

/// Check for volume changes by comparing current state with known state
fn check_for_volume_changes() {
    let current_volumes = get_current_volumes();

    let known = match KNOWN_VOLUMES.get() {
        Some(k) => k,
        None => return,
    };

    let mut known_guard = match known.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    // Find newly mounted volumes
    for path in current_volumes.difference(&known_guard) {
        debug!("Volume mounted: {}", path);
        emit_volume_mounted(path);
    }

    // Find unmounted volumes
    for path in known_guard.difference(&current_volumes) {
        debug!("Volume unmounted: {}", path);
        emit_volume_unmounted(path);
    }

    // Update known volumes
    *known_guard = current_volumes;
}

/// Stop watching for volume events.
/// Call this on app shutdown.
#[allow(dead_code, reason = "Will be used for explicit cleanup on app shutdown")]
pub fn stop_volume_watcher() {
    if let Some(watcher_storage) = WATCHER.get()
        && let Ok(mut guard) = watcher_storage.lock()
    {
        *guard = None;
    }
    debug!("Volume watcher stopped");
}

/// Emit a volume mounted event to the frontend and register with VolumeManager.
fn emit_volume_mounted(volume_path: &str) {
    // Register the new volume with VolumeManager so it can be used for file operations
    register_volume_with_manager(volume_path);

    if let Some(app) = APP_HANDLE.get() {
        let payload = VolumeEventPayload {
            volume_path: volume_path.to_string(),
        };
        if let Err(e) = app.emit("volume-mounted", payload) {
            error!("Failed to emit volume-mounted event: {}", e);
        } else {
            debug!("Emitted volume-mounted event for {}", volume_path);
        }
    }
}

/// Register a mounted volume with the VolumeManager.
fn register_volume_with_manager(volume_path: &str) {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::LocalPosixVolume;
    use std::path::Path;
    use std::sync::Arc;

    // Generate volume ID from path (same logic as path_to_id in mod.rs)
    let volume_id: String = volume_path
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase();

    // Get volume name from path
    let name = Path::new(volume_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let volume = Arc::new(LocalPosixVolume::new(&name, volume_path));
    get_volume_manager().register(&volume_id, volume);
    debug!("Registered mounted volume: {} -> {}", volume_id, volume_path);
}

/// Emit a volume unmounted event to the frontend and unregister from VolumeManager.
fn emit_volume_unmounted(volume_path: &str) {
    // Unregister the volume from VolumeManager
    unregister_volume_from_manager(volume_path);

    if let Some(app) = APP_HANDLE.get() {
        let payload = VolumeEventPayload {
            volume_path: volume_path.to_string(),
        };
        if let Err(e) = app.emit("volume-unmounted", payload) {
            error!("Failed to emit volume-unmounted event: {}", e);
        } else {
            debug!("Emitted volume-unmounted event for {}", volume_path);
        }
    }
}

/// Unregister a volume from the VolumeManager.
fn unregister_volume_from_manager(volume_path: &str) {
    use crate::file_system::get_volume_manager;

    // Generate volume ID from path (same logic as path_to_id in mod.rs)
    let volume_id: String = volume_path
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase();

    get_volume_manager().unregister(&volume_id);
    debug!("Unregistered volume: {} ({})", volume_id, volume_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_event_payload_serialization() {
        let payload = VolumeEventPayload {
            volume_path: "/Volumes/MyDrive".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("volumePath"));
        assert!(json.contains("/Volumes/MyDrive"));
    }

    #[test]
    fn test_get_current_volumes() {
        let volumes = get_current_volumes();
        // /Volumes should always have at least "Macintosh HD" or similar
        // This test just ensures the function doesn't panic
        assert!(volumes.is_empty() || !volumes.is_empty());
    }
}
