//! Volume mount/unmount watcher for Linux.
//!
//! Two watchers run concurrently:
//! - `/proc/mounts` (inotify) — detects standard mount/unmount operations
//! - `/run/user/<uid>/gvfs/` (inotify) — detects GVFS SMB share mount/unmount
//!   (these are subdirectories of a single gvfsd-fuse mount, so they don't
//!   appear in `/proc/mounts`)
//!
//! Both diff against known state and emit `volume-mounted` / `volume-unmounted`
//! Tauri events. Also registers/unregisters volumes with the global `VolumeManager`.

use crate::file_system::linux_mounts;
use log::{debug, error, info, warn};
use notify::{Event, EventKind, RecommendedWatcher, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

/// Global app handle for emitting events from the watcher.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// The watcher instance for /proc/mounts (kept alive for the app's lifetime).
static WATCHER: OnceLock<Mutex<Option<RecommendedWatcher>>> = OnceLock::new();

/// The watcher instance for GVFS directory.
static GVFS_WATCHER: OnceLock<Mutex<Option<RecommendedWatcher>>> = OnceLock::new();

/// Known mount points mapped to their filesystem type, for diffing.
static KNOWN_MOUNTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// Known GVFS SMB mount paths, for diffing.
static KNOWN_GVFS_MOUNTS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

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

    start_gvfs_watcher();
}

/// Stop both volume watchers (proc/mounts and GVFS).
#[allow(dead_code, reason = "Symmetry with macOS, will be used for explicit cleanup")]
pub fn stop_volume_watcher() {
    if let Some(storage) = WATCHER.get()
        && let Ok(mut guard) = storage.lock()
    {
        *guard = None;
    }
    if let Some(storage) = GVFS_WATCHER.get()
        && let Ok(mut guard) = storage.lock()
    {
        *guard = None;
    }
    debug!("Linux volume watchers stopped");
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

/// Start watching the GVFS directory for SMB share mount/unmount.
/// Skips silently if `/run/user/<uid>/gvfs/` doesn't exist (non-GNOME systems).
fn start_gvfs_watcher() {
    let uid = unsafe { libc::getuid() };
    let gvfs_dir = format!("/run/user/{}/gvfs", uid);
    let gvfs_path = Path::new(&gvfs_dir);

    if !gvfs_path.is_dir() {
        debug!("GVFS directory {} not found, skipping GVFS watcher", gvfs_dir);
        return;
    }

    // Snapshot current GVFS SMB mounts
    let initial = get_current_gvfs_smb_paths(&gvfs_dir);
    let known = KNOWN_GVFS_MOUNTS.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut guard) = known.lock() {
        debug!("Initial GVFS SMB mounts: {} entries", initial.len());
        *guard = initial;
    }

    let gvfs_dir_owned = gvfs_dir.clone();
    let watcher_result = notify::recommended_watcher(move |result: Result<Event, notify::Error>| match result {
        Ok(event) => handle_gvfs_event(event, &gvfs_dir_owned),
        Err(e) => error!("GVFS watcher error: {}", e),
    });

    match watcher_result {
        Ok(mut watcher) => {
            if let Err(e) = watcher.watch(gvfs_path, notify::RecursiveMode::NonRecursive) {
                warn!("Failed to watch GVFS directory {}: {}", gvfs_dir, e);
                return;
            }

            let storage = GVFS_WATCHER.get_or_init(|| Mutex::new(None));
            if let Ok(mut guard) = storage.lock() {
                *guard = Some(watcher);
            }

            info!("GVFS watcher started on {}", gvfs_dir);
        }
        Err(e) => {
            warn!("Failed to create GVFS watcher: {}", e);
        }
    }
}

/// Handle inotify events on the GVFS directory.
fn handle_gvfs_event(event: Event, gvfs_dir: &str) {
    match event.kind {
        EventKind::Create(_) | EventKind::Remove(_) => {
            check_for_gvfs_changes(gvfs_dir);
        }
        _ => {}
    }
}

/// Diff current GVFS SMB directories against known state and emit events.
fn check_for_gvfs_changes(gvfs_dir: &str) {
    let current = get_current_gvfs_smb_paths(gvfs_dir);

    let known = match KNOWN_GVFS_MOUNTS.get() {
        Some(k) => k,
        None => return,
    };

    let mut known_guard = match known.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    // Newly mounted shares
    for path in &current {
        if !known_guard.contains(path) {
            debug!("GVFS SMB share mounted: {}", path);
            emit_volume_mounted(path);
        }
    }

    // Unmounted shares
    for path in known_guard.iter() {
        if !current.contains(path) {
            debug!("GVFS SMB share unmounted: {}", path);
            emit_volume_unmounted(path);
        }
    }

    *known_guard = current;
}

/// Scan the GVFS directory for current SMB share mount paths.
fn get_current_gvfs_smb_paths(gvfs_dir: &str) -> HashSet<String> {
    let mut paths = HashSet::new();
    let Ok(entries) = std::fs::read_dir(gvfs_dir) else {
        return paths;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let dirname = name.to_string_lossy();
        if super::parse_gvfs_smb_dirname(&dirname).is_some() {
            paths.insert(entry.path().to_string_lossy().to_string());
        }
    }
    paths
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

    let volume_id = super::path_to_id(volume_path);

    // For GVFS SMB shares, extract the share name instead of the raw dirname
    let name = if let Some(dirname) = Path::new(volume_path).file_name().and_then(|n| n.to_str()) {
        if let Some((_server, share)) = super::parse_gvfs_smb_dirname(dirname) {
            share
        } else {
            dirname.to_string()
        }
    } else {
        "Unknown".to_string()
    };

    let volume = Arc::new(LocalPosixVolume::new(&name, volume_path));
    get_volume_manager().register(&volume_id, volume);
    debug!("Registered mounted volume: {} -> {}", volume_id, volume_path);
}

/// Unregister a volume from the global VolumeManager.
fn unregister_volume_from_manager(volume_path: &str) {
    use crate::file_system::get_volume_manager;

    let volume_id = super::path_to_id(volume_path);
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
