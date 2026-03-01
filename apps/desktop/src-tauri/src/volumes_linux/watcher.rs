//! Volume mount/unmount watcher for Linux.
//!
//! Watches `/proc/mounts` for changes using `notify` (inotify). When mounts
//! change, diffs against the previous state and emits `volume-mounted` /
//! `volume-unmounted` Tauri events. Also registers/unregisters volumes with
//! the global `VolumeManager`.

use crate::file_system::linux_mounts;
use log::{debug, error, info};
use notify::{Event, EventKind, RecommendedWatcher, Watcher};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

/// Global app handle for emitting events from the watcher.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// The watcher instance (kept alive for the app's lifetime).
static WATCHER: OnceLock<Mutex<Option<RecommendedWatcher>>> = OnceLock::new();

/// Known mount points mapped to their filesystem type, for diffing.
static KNOWN_MOUNTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// Payload for volume mount/unmount events.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeEventPayload {
    pub volume_path: String,
}

/// Start watching for volume mount/unmount events.
/// Call this once during app setup.
pub fn start_volume_watcher(app: &AppHandle) {
    if APP_HANDLE.set(app.clone()).is_err() {
        debug!("Linux volume watcher already initialized");
        return;
    }

    let initial = get_real_mounts();
    let known = KNOWN_MOUNTS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = known.lock() {
        *guard = initial;
        debug!("Initial Linux mounts: {} entries", guard.len());
    }

    info!("Starting Linux volume watcher on /proc/mounts");

    let watcher_result = notify::recommended_watcher(move |result: Result<Event, notify::Error>| match result {
        Ok(event) => handle_fs_event(event),
        Err(e) => error!("Linux volume watcher error: {}", e),
    });

    match watcher_result {
        Ok(mut watcher) => {
            // Watch /proc/mounts — inotify fires when mounts change
            let proc_mounts = Path::new("/proc/mounts");
            if let Err(e) = watcher.watch(proc_mounts, notify::RecursiveMode::NonRecursive) {
                error!("Failed to watch /proc/mounts: {}", e);
                return;
            }

            let storage = WATCHER.get_or_init(|| Mutex::new(None));
            if let Ok(mut guard) = storage.lock() {
                *guard = Some(watcher);
            }

            info!("Linux volume watcher started successfully");
        }
        Err(e) => {
            error!("Failed to create Linux volume watcher: {}", e);
        }
    }
}

/// Stop the volume watcher.
#[allow(dead_code, reason = "Symmetry with macOS, will be used for explicit cleanup")]
pub fn stop_volume_watcher() {
    if let Some(storage) = WATCHER.get()
        && let Ok(mut guard) = storage.lock()
    {
        *guard = None;
    }
    debug!("Linux volume watcher stopped");
}

/// Handle filesystem events on /proc/mounts.
fn handle_fs_event(event: Event) {
    match event.kind {
        EventKind::Modify(_) | EventKind::Access(_) => {
            check_for_mount_changes();
        }
        _ => {}
    }
}

/// Diff current mounts against known state and emit events.
fn check_for_mount_changes() {
    let current = get_real_mounts();

    let known = match KNOWN_MOUNTS.get() {
        Some(k) => k,
        None => return,
    };

    let mut known_guard = match known.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    // Newly mounted
    for (path, _fstype) in &current {
        if !known_guard.contains_key(path) {
            debug!("Volume mounted: {}", path);
            emit_volume_mounted(path);
        }
    }

    // Unmounted
    for (path, _fstype) in known_guard.iter() {
        if !current.contains_key(path) {
            debug!("Volume unmounted: {}", path);
            emit_volume_unmounted(path);
        }
    }

    *known_guard = current;
}

/// Build a map of real (non-virtual) mount points from /proc/mounts.
fn get_real_mounts() -> HashMap<String, String> {
    let entries = linux_mounts::parse_proc_mounts();
    let virtual_types: &[&str] = &[
        "proc",
        "sysfs",
        "devpts",
        "tmpfs",
        "cgroup",
        "cgroup2",
        "devtmpfs",
        "hugetlbfs",
        "mqueue",
        "debugfs",
        "tracefs",
        "securityfs",
        "pstore",
        "configfs",
        "fusectl",
        "binfmt_misc",
        "autofs",
        "efivarfs",
        "ramfs",
        "rpc_pipefs",
        "nfsd",
        "nsfs",
        "bpf",
    ];

    entries
        .into_iter()
        .filter(|e| !virtual_types.contains(&e.fstype.as_str()))
        .map(|e| (e.mountpoint, e.fstype))
        .collect()
}

/// Emit a volume-mounted event and register with VolumeManager.
fn emit_volume_mounted(volume_path: &str) {
    register_volume_with_manager(volume_path);

    if let Some(app) = APP_HANDLE.get() {
        let payload = VolumeEventPayload {
            volume_path: volume_path.to_string(),
        };
        if let Err(e) = app.emit("volume-mounted", payload) {
            error!("Failed to emit volume-mounted event: {}", e);
        } else {
            debug!("Emitted volume-mounted for {}", volume_path);
        }
    }
}

/// Emit a volume-unmounted event and unregister from VolumeManager.
fn emit_volume_unmounted(volume_path: &str) {
    unregister_volume_from_manager(volume_path);

    if let Some(app) = APP_HANDLE.get() {
        let payload = VolumeEventPayload {
            volume_path: volume_path.to_string(),
        };
        if let Err(e) = app.emit("volume-unmounted", payload) {
            error!("Failed to emit volume-unmounted event: {}", e);
        } else {
            debug!("Emitted volume-unmounted for {}", volume_path);
        }
    }
}

/// Register a volume with the global VolumeManager.
fn register_volume_with_manager(volume_path: &str) {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::LocalPosixVolume;
    use std::sync::Arc;

    let volume_id: String = volume_path
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase();

    let name = Path::new(volume_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let volume = Arc::new(LocalPosixVolume::new(&name, volume_path));
    get_volume_manager().register(&volume_id, volume);
    debug!("Registered mounted volume: {} -> {}", volume_id, volume_path);
}

/// Unregister a volume from the global VolumeManager.
fn unregister_volume_from_manager(volume_path: &str) {
    use crate::file_system::get_volume_manager;

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
            volume_path: "/mnt/usb".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("volumePath"));
        assert!(json.contains("/mnt/usb"));
    }

    #[test]
    fn test_get_real_mounts_filters_virtual() {
        let mounts = get_real_mounts();
        // Should not contain virtual fs mount points
        for (path, fstype) in &mounts {
            assert_ne!(fstype, "proc", "Should filter proc at {}", path);
            assert_ne!(fstype, "sysfs", "Should filter sysfs at {}", path);
            assert_ne!(fstype, "tmpfs", "Should filter tmpfs at {}", path);
        }
    }
}
