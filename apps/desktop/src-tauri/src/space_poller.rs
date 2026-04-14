//! Live disk-space poller.
//!
//! Polls `get_volume_space()` for volumes the frontend is actively displaying
//! in panes, and emits `volume-space-changed` events when the value changes
//! beyond a configurable threshold.
//!
//! Poll intervals are per-volume-type via `Volume::space_poll_interval()`:
//! local volumes poll every 2 s, network/MTP every 5 s.

use log::{debug, warn};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::file_system::get_volume_manager;

/// Global app handle for emitting events.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Watchers registered by the frontend: watcher_id → (volume_id, path).
///
/// The key is the watcher_id (typically a pane ID like "left" or "right") so
/// each pane has its own independent entry. The poller deduplicates by
/// volume_id to avoid polling the same volume twice per tick.
static WATCHED: OnceLock<Mutex<HashMap<String, WatchEntry>>> = OnceLock::new();

/// Last emitted space per volume, for change detection.
static LAST_SPACE: OnceLock<Mutex<HashMap<String, CachedSpace>>> = OnceLock::new();

/// Change threshold in bytes. Updated at runtime from settings.
static THRESHOLD_BYTES: AtomicU64 = AtomicU64::new(1_048_576); // 1 MB default

/// Default poll interval for volumes not registered in VolumeManager.
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Timeout for a single space-info fetch. Prevents a hung mount from stalling
/// all volume polls in the same tick.
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone)]
struct WatchEntry {
    volume_id: String,
    path: String,
}

#[derive(Clone)]
struct CachedSpace {
    total_bytes: u64,
    available_bytes: u64,
}

/// Payload for the `volume-space-changed` Tauri event.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VolumeSpaceChangedPayload {
    volume_id: String,
    total_bytes: u64,
    available_bytes: u64,
}

/// Stores the app handle. Call once during setup.
pub fn init(app: &AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
    let _ = WATCHED.set(Mutex::new(HashMap::new()));
    let _ = LAST_SPACE.set(Mutex::new(HashMap::new()));
}

/// Updates the threshold from the Settings UI (value in megabytes).
pub fn set_threshold_mb(mb: u64) {
    THRESHOLD_BYTES.store(mb.saturating_mul(1_048_576), Ordering::Relaxed);
}

/// Registers (or updates) a watcher for live space updates.
///
/// `watcher_id` is typically a pane ID ("left"/"right"). Multiple watchers
/// can watch the same volume without interfering with each other.
pub fn watch(watcher_id: String, volume_id: String, path: String) {
    if let Some(w) = WATCHED.get()
        && let Ok(mut map) = w.lock()
    {
        map.insert(watcher_id, WatchEntry { volume_id, path });
    }
}

/// Stops watching. Only removes this watcher's entry; other watchers on the
/// same volume are unaffected.
pub fn unwatch(watcher_id: &str) {
    if let Some(w) = WATCHED.get()
        && let Ok(mut map) = w.lock()
    {
        map.remove(watcher_id);
    }
    // Note: we don't clear LAST_SPACE here. Another watcher may still be on
    // the same volume, and clearing the cache would force a spurious re-emit.
}

/// Starts the background poll loop. Call once from setup.
pub fn start() {
    tauri::async_runtime::spawn(async { poll_loop().await });
}

// ── Tauri commands ──────────────────────────────────────────────────────

/// Registers a watcher for live space monitoring.
#[tauri::command]
pub fn watch_volume_space(watcher_id: String, volume_id: String, path: String) {
    watch(watcher_id, volume_id, path);
}

/// Stops monitoring for this watcher.
#[tauri::command]
pub fn unwatch_volume_space(watcher_id: String) {
    unwatch(&watcher_id);
}

/// Updates the change threshold at runtime (from settings).
#[tauri::command]
pub fn set_disk_space_threshold(mb: u64) {
    set_threshold_mb(mb);
}

/// The core loop. Ticks every second; each volume is polled at its own cadence.
async fn poll_loop() {
    let mut tick: u64 = 0;
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        tick += 1;

        // Snapshot the watch list and deduplicate by volume_id.
        // Multiple panes on the same volume produce one poll.
        let unique_volumes: HashMap<String, String> = match WATCHED.get().and_then(|w| w.lock().ok()) {
            Some(map) => {
                let mut deduped = HashMap::new();
                for entry in map.values() {
                    deduped
                        .entry(entry.volume_id.clone())
                        .or_insert_with(|| entry.path.clone());
                }
                deduped
            }
            None => continue,
        };

        let manager = get_volume_manager();
        let threshold = THRESHOLD_BYTES.load(Ordering::Relaxed);

        for (volume_id, path) in unique_volumes {
            let volume = manager.get(&volume_id);

            // Determine poll interval from the Volume trait (elegant per-type cadence).
            let interval = volume
                .as_ref()
                .and_then(|v| v.space_poll_interval())
                .unwrap_or(DEFAULT_POLL_INTERVAL);

            let interval_secs = interval.as_secs().max(1);
            if !tick.is_multiple_of(interval_secs) {
                continue;
            }

            // Fetch space on a blocking thread with a timeout so a hung mount
            // doesn't stall the entire poll loop.
            let vol_clone = volume.clone();
            let path_clone = path.clone();
            let fetch = tokio::task::spawn_blocking(move || {
                if let Some(vol) = vol_clone
                    && let Ok(info) = vol.get_space_info()
                {
                    return Some(CachedSpace {
                        total_bytes: info.total_bytes,
                        available_bytes: info.available_bytes,
                    });
                }
                fetch_space_for_path(&path_clone)
            });
            let space = match tokio::time::timeout(FETCH_TIMEOUT, fetch).await {
                Ok(Ok(Some(s))) => s,
                _ => continue, // timeout, panic, or no data — skip this tick
            };

            if exceeds_threshold(&volume_id, &space, threshold) {
                update_cache(&volume_id, &space);
                emit(&volume_id, &space);
            }
        }
    }
}

/// Fetches space info for a filesystem path using the platform API.
/// Used as a fallback when the volume is not registered in VolumeManager.
fn fetch_space_for_path(path: &str) -> Option<CachedSpace> {
    #[cfg(target_os = "macos")]
    {
        let info = crate::volumes::get_volume_space(path)?;
        Some(CachedSpace {
            total_bytes: info.total_bytes,
            available_bytes: info.available_bytes,
        })
    }

    #[cfg(target_os = "linux")]
    {
        let info = crate::volumes_linux::get_volume_space(path)?;
        Some(CachedSpace {
            total_bytes: info.total_bytes,
            available_bytes: info.available_bytes,
        })
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        None
    }
}

/// Returns `true` if the new space exceeds the threshold relative to the last emission.
fn exceeds_threshold(volume_id: &str, new: &CachedSpace, threshold: u64) -> bool {
    let cache = match LAST_SPACE.get() {
        Some(c) => c,
        None => return true,
    };
    let map = match cache.lock() {
        Ok(m) => m,
        Err(_) => return true,
    };
    match map.get(volume_id) {
        Some(old) => {
            let diff = (old.available_bytes as i64 - new.available_bytes as i64).unsigned_abs();
            diff >= threshold
        }
        None => true, // First fetch — always emit.
    }
}

fn update_cache(volume_id: &str, space: &CachedSpace) {
    if let Some(cache) = LAST_SPACE.get()
        && let Ok(mut map) = cache.lock()
    {
        map.insert(volume_id.to_string(), space.clone());
    }
}

fn emit(volume_id: &str, space: &CachedSpace) {
    let Some(app) = APP_HANDLE.get() else { return };
    let payload = VolumeSpaceChangedPayload {
        volume_id: volume_id.to_string(),
        total_bytes: space.total_bytes,
        available_bytes: space.available_bytes,
    };
    debug!("volume-space-changed: {} ({} avail)", volume_id, space.available_bytes);
    if let Err(e) = app.emit("volume-space-changed", &payload) {
        warn!("Failed to emit volume-space-changed: {}", e);
    }
}
