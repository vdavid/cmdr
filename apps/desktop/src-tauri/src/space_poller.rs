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
//! a hysteresis detector that emits a `low-disk-space` event on each edge:
//! `is_low: true` when free space crosses below the user-configured percent
//! threshold, `is_low: false` when it recovers above the re-arm margin (so the
//! frontend auto-dismisses the toast). The live free-space numbers shown while
//! the toast is up ride the separate `volume-space-changed` stream, which the
//! boot-volume watcher already emits every tick. The poll loop deduplicates by
//! volume id, so a pane watching the boot volume shares the same single
//! `statfs` per tick with the permanent watcher.

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::file_system::volume::DEFAULT_VOLUME_ID;
use crate::file_system::volume::SpaceInfo;
use crate::file_system::volume::manager::get_volume_manager;
use crate::ignore_poison::IgnorePoison;

/// Global app handle for emitting events.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Watchers registered by the frontend: watcher_id → (volume_id, path).
///
/// The key is the watcher_id (typically a pane ID like "left" or "right") so
/// each pane has its own independent entry. The poller deduplicates by
/// volume_id to avoid polling the same volume twice per tick.
static WATCHED: OnceLock<Mutex<HashMap<String, WatchEntry>>> = OnceLock::new();

/// Last emitted space per volume, for change detection.
static LAST_SPACE: OnceLock<Mutex<HashMap<String, SpaceInfo>>> = OnceLock::new();

/// Rate limit for the per-emission debug line (the events themselves are never
/// throttled; only the logging is). Per volume, so a churning boot disk can't
/// swallow a NAS's first line.
static SPACE_EMIT_LOG: cmdr_fs::log_rollup::LogRollup = cmdr_fs::log_rollup::LogRollup::new(Duration::from_secs(60));

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

/// The last polled space for a volume, if the poller has any.
///
/// A READ of the poll cache, never a fresh `statfs`. That's the point: the MCP
/// resources and the agent's read tools answer "how full is this drive" without
/// a syscall that a hung NAS could park them on for two minutes
/// (`agent/tools/CLAUDE.md`: handlers read stores, never the filesystem).
///
/// A volume has an entry while something is watching it: the boot volume
/// permanently (the low-disk-space watcher, on by default), and any volume a pane
/// is showing. Nothing watching ⇒ `None`, which callers render as absent rather
/// than as a guessed zero. Widening this beyond the watched set means widening
/// what the poller watches, not reaching for `statfs` here.
pub(crate) fn cached_space(volume_id: &str) -> Option<SpaceInfo> {
    let cache = LAST_SPACE.get()?;
    let map = cache.lock_ignore_poison();
    map.get(volume_id).copied()
}

/// Typed `volume-space-changed` Tauri event. The struct name kebab-cases to the
/// wire event name (`volume-space-changed`) via `tauri_specta::Event`. Both the
/// TS payload type and a typed `events.volumeSpaceChanged.listen(...)` helper are
/// generated into `apps/desktop/src/lib/ipc/bindings.ts`.
///
/// The whole [`SpaceInfo`] rides along rather than two loose numbers, so a volume
/// with no ceiling stays recognizable all the way to the pane's indicator.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpaceChanged {
    pub volume_id: String,
    pub space: SpaceInfo,
}

/// Typed `low-disk-space` Tauri event. The struct keeps its `Payload` suffix
/// (used internally), so the wire name is pinned with `event_name` rather than
/// letting the kebab-case of the ident drift to `low-disk-space-payload`.
///
/// One event carries both hysteresis transitions, distinguished by `is_low`:
/// `true` when free space crosses below the threshold (show the warning),
/// `false` when it recovers above threshold + [`LOW_SPACE_REARM_MARGIN_PERCENT`]
/// (dismiss it). The in-app toast acts on both; the macOS native notification
/// only on `is_low: true` (a delivered notification can't be recalled).
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "low-disk-space")]
#[serde(rename_all = "camelCase")]
pub struct LowDiskSpacePayload {
    pub volume_id: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub free_percent: f64,
    pub threshold_percent: u64,
    pub is_low: bool,
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
    if let Some(w) = WATCHED.get() {
        w.lock_ignore_poison()
            .insert(watcher_id, WatchEntry { volume_id, path });
    }
}

/// Stops watching. Only removes this watcher's entry; other watchers on the
/// same volume are unaffected.
pub fn unwatch(watcher_id: &str) {
    if let Some(w) = WATCHED.get() {
        w.lock_ignore_poison().remove(watcher_id);
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
        let unique_volumes: HashMap<String, String> = match WATCHED.get().map(|w| w.lock_ignore_poison()) {
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
                    return Some(info);
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

/// Which edge, if any, a hysteresis step crossed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LowSpaceTransition {
    /// No edge crossed; emit nothing.
    None,
    /// Free space fell below the threshold: show the warning.
    BecameLow,
    /// Free space recovered above threshold + margin: dismiss the warning.
    Recovered,
}

/// Runs the hysteresis detector on a fresh boot-volume space fetch and emits
/// `low-disk-space` on each edge: `is_low: true` when free percent crosses
/// below the threshold, `is_low: false` when it recovers above the re-arm
/// margin (so the frontend can auto-dismiss the toast).
///
/// ❗ A volume with no ceiling is never low and never recovers: there is no
/// percentage to cross a threshold with, and you can't run out of storage that
/// has no limit. It leaves the detector's armed state untouched, so a boot
/// volume that somehow reported one can't disarm the warning for the real one.
fn check_low_space(volume_id: &str, space: &SpaceInfo) {
    if !LOW_SPACE_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let SpaceInfo::Bounded {
        total_bytes,
        available_bytes,
        ..
    } = *space
    else {
        return;
    };
    let threshold = LOW_SPACE_THRESHOLD_PERCENT.load(Ordering::Relaxed);
    let free = free_percent(total_bytes, available_bytes);
    let armed = LOW_SPACE_ARMED.load(Ordering::Relaxed);
    let (new_armed, transition) = low_space_transition(armed, free, threshold as f64);
    LOW_SPACE_ARMED.store(new_armed, Ordering::Relaxed);
    match transition {
        LowSpaceTransition::BecameLow => {
            emit_low_disk_space(volume_id, total_bytes, available_bytes, free, threshold, true)
        }
        LowSpaceTransition::Recovered => {
            emit_low_disk_space(volume_id, total_bytes, available_bytes, free, threshold, false)
        }
        LowSpaceTransition::None => {}
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

/// The pure hysteresis step: `(armed, free, threshold)` → `(new_armed, transition)`.
///
/// Emits [`LowSpaceTransition::BecameLow`] exactly once per crossing below the
/// threshold, then [`LowSpaceTransition::Recovered`] once free space climbs back
/// above threshold + [`LOW_SPACE_REARM_MARGIN_PERCENT`] (which also re-arms it).
/// The margin means oscillation around the boundary can't re-fire either edge.
fn low_space_transition(armed: bool, free_percent: f64, threshold_percent: f64) -> (bool, LowSpaceTransition) {
    if armed && free_percent < threshold_percent {
        return (false, LowSpaceTransition::BecameLow);
    }
    if !armed && free_percent >= threshold_percent + LOW_SPACE_REARM_MARGIN_PERCENT {
        return (true, LowSpaceTransition::Recovered);
    }
    (armed, LowSpaceTransition::None)
}

fn emit_low_disk_space(
    volume_id: &str,
    total_bytes: u64,
    available_bytes: u64,
    free_percent: f64,
    threshold_percent: u64,
    is_low: bool,
) {
    let Some(app) = APP_HANDLE.get() else { return };
    let payload = LowDiskSpacePayload {
        volume_id: volume_id.to_string(),
        total_bytes,
        available_bytes,
        free_percent,
        threshold_percent,
        is_low,
    };
    info!(
        "low-disk-space ({}): {} at {:.1}% free ({} of {} bytes), threshold {}%",
        if is_low { "low" } else { "recovered" },
        volume_id,
        free_percent,
        available_bytes,
        total_bytes,
        threshold_percent
    );
    if let Err(e) = payload.emit(app) {
        warn!("Failed to emit low-disk-space: {}", e);
    }
}

/// Fetches space info for a filesystem path using the platform API.
/// Used as a fallback when the volume is not registered in VolumeManager.
///
/// Always [`SpaceInfo::Bounded`]: a mounted filesystem has a size.
fn fetch_space_for_path(path: &str) -> Option<SpaceInfo> {
    #[cfg(target_os = "macos")]
    {
        crate::volumes::get_volume_space(path)
    }

    #[cfg(target_os = "linux")]
    {
        crate::volumes_linux::get_volume_space(path)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        None
    }
}

/// The one number a reader watches move on this volume, and the number the emit
/// threshold is measured on.
///
/// A bounded volume's story is how much room is LEFT; an unbounded one has no
/// such number, so its story is how much is stored. Both move by the same amount
/// on a write, so one threshold serves both.
fn moving_figure(space: &SpaceInfo) -> u64 {
    space.available_bytes().unwrap_or_else(|| space.used_bytes())
}

/// [`moving_figure`] with the word that says which figure it is, for the log.
fn log_figure(space: &SpaceInfo) -> String {
    match space.available_bytes() {
        Some(available) => format!("{available} avail"),
        None => format!("{} used, no ceiling", space.used_bytes()),
    }
}

/// Returns `true` if the new space exceeds the threshold relative to the last
/// emission.
///
/// A volume that changed SHAPE (a quota added or lifted between polls) always
/// emits: the two figures aren't comparable, and the pane has a different thing
/// to draw.
fn exceeds_threshold(volume_id: &str, new: &SpaceInfo, threshold: u64) -> bool {
    let cache = match LAST_SPACE.get() {
        Some(c) => c,
        None => return true,
    };
    let map = cache.lock_ignore_poison();
    match map.get(volume_id) {
        Some(old) if old.available_bytes().is_some() == new.available_bytes().is_some() => {
            let diff = (moving_figure(old) as i64 - moving_figure(new) as i64).unsigned_abs();
            diff >= threshold
        }
        // A changed shape, or a first fetch: always emit.
        _ => true,
    }
}

fn update_cache(volume_id: &str, space: &SpaceInfo) {
    if let Some(cache) = LAST_SPACE.get() {
        cache.lock_ignore_poison().insert(volume_id.to_string(), *space);
    }
}

fn emit(volume_id: &str, space: &SpaceInfo) {
    let Some(app) = APP_HANDLE.get() else { return };
    let payload = VolumeSpaceChanged {
        volume_id: volume_id.to_string(),
        space: *space,
    };
    // A machine that's building writes past the 1 MB threshold on most 2 s ticks,
    // so this was ~1,000 lines an hour of "the number moved again". Rolled up per
    // volume: the first emission logs at once, then one line a minute carrying how
    // many emissions it stands for and the latest value. What a reader needs from
    // this line is that the stream is flowing and roughly how fast, and the rolled
    // up form says both.
    let moved = log_figure(space);
    if let Some(batch) = SPACE_EMIT_LOG.record(volume_id) {
        if batch.is_rolled_up() {
            debug!(
                "volume-space-changed: {} ×{} in {}s (now {})",
                volume_id,
                batch.count,
                batch.elapsed.as_secs(),
                moved
            );
        } else {
            debug!("volume-space-changed: {} ({})", volume_id, moved);
        }
    }
    if let Err(e) = payload.emit(app) {
        warn!("Failed to emit volume-space-changed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_once_when_crossing_below_threshold() {
        let (armed, transition) = low_space_transition(true, 4.9, 5.0);
        assert!(!armed);
        assert_eq!(transition, LowSpaceTransition::BecameLow);
    }

    #[test]
    fn does_not_fire_above_threshold() {
        let (armed, transition) = low_space_transition(true, 5.0, 5.0);
        assert!(armed);
        assert_eq!(transition, LowSpaceTransition::None);
    }

    #[test]
    fn does_not_refire_while_disarmed() {
        let (armed, transition) = low_space_transition(false, 3.0, 5.0);
        assert!(!armed);
        assert_eq!(transition, LowSpaceTransition::None);
    }

    #[test]
    fn stays_disarmed_inside_rearm_margin() {
        // Recovered above the threshold but not past the margin: no re-arm,
        // so neither a dip back under 5% nor a recovery signal fires yet.
        let (armed, transition) = low_space_transition(false, 5.5, 5.0);
        assert!(!armed);
        assert_eq!(transition, LowSpaceTransition::None);
    }

    #[test]
    fn recovers_past_the_margin() {
        // Climbing above threshold + margin re-arms AND emits the recovery
        // edge, so the frontend can auto-dismiss the toast.
        let (armed, transition) = low_space_transition(false, 6.0, 5.0);
        assert!(armed);
        assert_eq!(transition, LowSpaceTransition::Recovered);
    }

    #[test]
    fn recovers_then_fires_on_next_crossing() {
        let (armed, transition) = low_space_transition(false, 6.0, 5.0);
        assert!(armed);
        assert_eq!(transition, LowSpaceTransition::Recovered);
        let (armed, transition) = low_space_transition(armed, 4.0, 5.0);
        assert!(!armed);
        assert_eq!(transition, LowSpaceTransition::BecameLow);
    }

    #[test]
    fn oscillation_around_threshold_fires_once() {
        // 5.2 → 4.8 → 5.2 → 4.8: one BecameLow, no spurious recovery
        // (never climbs past the 6% re-arm margin).
        let mut armed = true;
        let mut lows = 0;
        let mut recoveries = 0;
        for free in [5.2, 4.8, 5.2, 4.8] {
            let (next, transition) = low_space_transition(armed, free, 5.0);
            armed = next;
            match transition {
                LowSpaceTransition::BecameLow => lows += 1,
                LowSpaceTransition::Recovered => recoveries += 1,
                LowSpaceTransition::None => {}
            }
        }
        assert_eq!(lows, 1);
        assert_eq!(recoveries, 0);
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
