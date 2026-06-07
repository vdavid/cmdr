//! Live disk-space poller.
//!
//! Polls `get_volume_space()` for volumes the frontend is actively displaying
//! in panes, and emits `volume-space-changed` events when the value changes
//! beyond a configurable threshold.
//!
//! Poll intervals are per-volume-type via `Volume::space_poll_interval()`:
//! local volumes poll every 2 s, network/MTP every 5 s.
//!
//! Also owns the low-disk-space warning: a permanent, backend-owned watcher on
//! the boot volume (so the check works even when neither pane shows it) feeds
//! a hysteresis detector that emits a `low-disk-space` event when free space
//! crosses below the user-configured percent threshold. The poll loop already
//! deduplicates by volume id, so a pane watching the boot volume shares the
//! same single `statfs` per tick with the permanent watcher.

use log::{debug, info, warn};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::file_system::get_volume_manager;
use crate::file_system::volume::DEFAULT_VOLUME_ID;

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

/// Whether the low-disk-space warning is on. Mirrors the
/// `behavior.fileSystemWatching.lowDiskSpaceNotifications` setting
/// (`true` for any mode but "off"; the registry default is "in-app").
static LOW_SPACE_ENABLED: AtomicBool = AtomicBool::new(true);

/// Free-space percent threshold for the low-disk-space warning. Mirrors the
/// `behavior.fileSystemWatching.lowDiskSpaceThresholdPercent` setting.
static LOW_SPACE_THRESHOLD_PERCENT: AtomicU64 = AtomicU64::new(5);

/// Hysteresis state: `true` means the detector may fire on the next crossing
/// below the threshold. Disarmed after firing; re-armed once free space climbs
/// back above threshold + [`LOW_SPACE_REARM_MARGIN_PERCENT`].
static LOW_SPACE_ARMED: AtomicBool = AtomicBool::new(true);

/// Re-arm margin in percentage points. Without it, free space oscillating
/// around the exact threshold (a download writing and deleting temp files)
/// would fire a warning on every dip.
const LOW_SPACE_REARM_MARGIN_PERCENT: f64 = 1.0;

/// Watcher id for the permanent backend-owned boot-volume entry. Lives in the
/// same `WATCHED` map as the pane watchers so the dedup-by-volume-id logic
/// merges it with a pane that happens to show the boot volume.
const LOW_SPACE_BOOT_WATCHER_ID: &str = "low-space:boot";

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

/// Payload for the `low-disk-space` Tauri event.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LowDiskSpacePayload {
    volume_id: String,
    total_bytes: u64,
    available_bytes: u64,
    free_percent: f64,
    threshold_percent: u64,
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

/// Applies the low-disk-space warning config (at startup and live from Settings).
///
/// Registers or removes the permanent boot-volume watcher so the extra
/// `statfs` goes away entirely when the warning is off. Always re-arms the
/// detector: a changed threshold should re-evaluate against the current free
/// space on the next poll.
pub fn configure_low_disk_space(enabled: bool, threshold_percent: u64) {
    LOW_SPACE_ENABLED.store(enabled, Ordering::Relaxed);
    LOW_SPACE_THRESHOLD_PERCENT.store(threshold_percent.clamp(1, 99), Ordering::Relaxed);
    LOW_SPACE_ARMED.store(true, Ordering::Relaxed);
    if enabled {
        watch(
            LOW_SPACE_BOOT_WATCHER_ID.to_string(),
            DEFAULT_VOLUME_ID.to_string(),
            "/".to_string(),
        );
    } else {
        unwatch(LOW_SPACE_BOOT_WATCHER_ID);
    }
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
#[specta::specta]
pub fn watch_volume_space(watcher_id: String, volume_id: String, path: String) {
    watch(watcher_id, volume_id, path);
}

/// Stops monitoring for this watcher.
#[tauri::command]
#[specta::specta]
pub fn unwatch_volume_space(watcher_id: String) {
    unwatch(&watcher_id);
}

/// Updates the change threshold at runtime (from settings).
#[tauri::command]
#[specta::specta]
pub fn set_disk_space_threshold(mb: u64) {
    set_threshold_mb(mb);
}

/// Updates the low-disk-space warning config at runtime (from settings).
#[tauri::command]
#[specta::specta]
pub fn set_low_disk_space_config(enabled: bool, threshold_percent: u64) {
    configure_low_disk_space(enabled, threshold_percent);
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
            let fetch = async move {
                if let Some(vol) = vol_clone
                    && let Ok(info) = vol.get_space_info().await
                {
                    return Some(CachedSpace {
                        total_bytes: info.total_bytes,
                        available_bytes: info.available_bytes,
                    });
                }
                fetch_space_for_path(&path_clone)
            };
            let space = match tokio::time::timeout(FETCH_TIMEOUT, fetch).await {
                Ok(Some(s)) => s,
                _ => continue, // timeout or no data: skip this tick
            };

            // The low-space check sees every fetch, not just the ones that
            // pass the change-threshold gate below: a slow leak smaller than
            // the 1 MB emit threshold must still trip the warning.
            if volume_id == DEFAULT_VOLUME_ID {
                check_low_space(&volume_id, &space);
            }

            if exceeds_threshold(&volume_id, &space, threshold) {
                update_cache(&volume_id, &space);
                emit(&volume_id, &space);
            }
        }
    }
}

/// Runs the hysteresis detector on a fresh boot-volume space fetch and emits
/// `low-disk-space` when the free percent crosses below the threshold.
fn check_low_space(volume_id: &str, space: &CachedSpace) {
    if !LOW_SPACE_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let threshold = LOW_SPACE_THRESHOLD_PERCENT.load(Ordering::Relaxed);
    let free = free_percent(space.total_bytes, space.available_bytes);
    let armed = LOW_SPACE_ARMED.load(Ordering::Relaxed);
    let (new_armed, fire) = low_space_transition(armed, free, threshold as f64);
    LOW_SPACE_ARMED.store(new_armed, Ordering::Relaxed);
    if fire {
        emit_low_disk_space(volume_id, space, free, threshold);
    }
}

/// Free space as a percent of total. Treats an unknown total (0) as not-low
/// so a bogus fetch can't fire a false warning.
fn free_percent(total_bytes: u64, available_bytes: u64) -> f64 {
    if total_bytes == 0 {
        return 100.0;
    }
    available_bytes as f64 / total_bytes as f64 * 100.0
}

/// The pure hysteresis step: `(armed, free, threshold)` → `(new_armed, fire)`.
///
/// Fires exactly once per crossing below the threshold; re-arms only after
/// free space recovers above threshold + [`LOW_SPACE_REARM_MARGIN_PERCENT`],
/// so oscillation around the boundary can't re-fire.
fn low_space_transition(armed: bool, free_percent: f64, threshold_percent: f64) -> (bool, bool) {
    if armed && free_percent < threshold_percent {
        return (false, true);
    }
    if !armed && free_percent >= threshold_percent + LOW_SPACE_REARM_MARGIN_PERCENT {
        return (true, false);
    }
    (armed, false)
}

fn emit_low_disk_space(volume_id: &str, space: &CachedSpace, free_percent: f64, threshold_percent: u64) {
    let Some(app) = APP_HANDLE.get() else { return };
    let payload = LowDiskSpacePayload {
        volume_id: volume_id.to_string(),
        total_bytes: space.total_bytes,
        available_bytes: space.available_bytes,
        free_percent,
        threshold_percent,
    };
    info!(
        "low-disk-space: {} at {:.1}% free ({} of {} bytes), threshold {}%",
        volume_id, free_percent, space.available_bytes, space.total_bytes, threshold_percent
    );
    if let Err(e) = app.emit("low-disk-space", &payload) {
        warn!("Failed to emit low-disk-space: {}", e);
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
        None => true, // First fetch: always emit.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_once_when_crossing_below_threshold() {
        let (armed, fire) = low_space_transition(true, 4.9, 5.0);
        assert!(!armed);
        assert!(fire);
    }

    #[test]
    fn does_not_fire_above_threshold() {
        let (armed, fire) = low_space_transition(true, 5.0, 5.0);
        assert!(armed);
        assert!(!fire);
    }

    #[test]
    fn does_not_refire_while_disarmed() {
        let (armed, fire) = low_space_transition(false, 3.0, 5.0);
        assert!(!armed);
        assert!(!fire);
    }

    #[test]
    fn stays_disarmed_inside_rearm_margin() {
        // Recovered above the threshold but not past the margin: no re-arm,
        // so a dip back under 5% can't fire again.
        let (armed, fire) = low_space_transition(false, 5.5, 5.0);
        assert!(!armed);
        assert!(!fire);
    }

    #[test]
    fn rearms_past_the_margin_then_fires_on_next_crossing() {
        let (armed, fire) = low_space_transition(false, 6.0, 5.0);
        assert!(armed);
        assert!(!fire);
        let (armed, fire) = low_space_transition(armed, 4.0, 5.0);
        assert!(!armed);
        assert!(fire);
    }

    #[test]
    fn oscillation_around_threshold_fires_once() {
        // 5.2 → 4.8 → 5.2 → 4.8: one warning, not two.
        let mut armed = true;
        let mut fires = 0;
        for free in [5.2, 4.8, 5.2, 4.8] {
            let (next, fire) = low_space_transition(armed, free, 5.0);
            armed = next;
            if fire {
                fires += 1;
            }
        }
        assert_eq!(fires, 1);
    }

    #[test]
    fn free_percent_handles_zero_total() {
        // Unknown total must read as not-low (no false warning on a bogus fetch).
        assert_eq!(free_percent(0, 0), 100.0);
    }

    #[test]
    fn free_percent_computes_fraction() {
        assert!((free_percent(1000, 50) - 5.0).abs() < f64::EPSILON);
    }
}
