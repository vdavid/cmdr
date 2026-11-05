//! What the boot volume's free space means: the low-disk-space warning's hysteresis
//! detector and its `low-disk-space` event, plus the drive index's "is the disk you
//! live on low?" answer, which reads the same figure against the same threshold
//! (cmdr-reports#100). The poll loop in `mod.rs` feeds both on every boot-volume
//! fetch.

use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri_specta::Event;

use super::APP_HANDLE;
use crate::file_system::volume::SpaceInfo;

/// Whether the low-disk-space warning is on. Mirrors the
/// `behavior.fileSystemWatching.lowDiskSpaceNotifications` setting
/// (`true` for any mode but "off"; the registry default is "in-app").
static LOW_SPACE_ENABLED: AtomicBool = AtomicBool::new(true);

/// Free-space percent threshold for the low-disk-space warning and the index's
/// pause. Mirrors the `behavior.fileSystemWatching.lowDiskSpaceThresholdPercent`
/// setting.
static LOW_SPACE_THRESHOLD_PERCENT: AtomicU64 = AtomicU64::new(5);

/// Hysteresis state: `true` means the detector may fire on the next crossing
/// below the threshold. Disarmed after firing; re-armed once free space climbs
/// back above threshold + [`LOW_SPACE_REARM_MARGIN_PERCENT`].
static LOW_SPACE_ARMED: AtomicBool = AtomicBool::new(true);

/// Whether the boot volume is below the low-disk-space threshold as far as the drive
/// index is concerned. Its own hysteresis state, separate from [`LOW_SPACE_ARMED`]:
/// that one follows the warning's on/off switch, this one follows every reading.
/// Starts `false`, so an index never pauses on a reading nobody took.
static INDEX_STORAGE_LOW: AtomicBool = AtomicBool::new(false);

/// Re-arm margin in percentage points. Without it, free space oscillating
/// around the exact threshold (a download writing and deleting temp files)
/// would fire a warning on every dip.
const LOW_SPACE_REARM_MARGIN_PERCENT: f64 = 1.0;

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

/// Store the warning's switch and threshold, and re-arm its detector: a changed
/// threshold should re-evaluate against the current free space on the next poll.
pub(super) fn configure(enabled: bool, threshold_percent: u64) {
    LOW_SPACE_ENABLED.store(enabled, Ordering::Relaxed);
    LOW_SPACE_THRESHOLD_PERCENT.store(threshold_percent.clamp(1, 99), Ordering::Relaxed);
    LOW_SPACE_ARMED.store(true, Ordering::Relaxed);
}

/// Whether the low-disk-space toast is showing right now (the warning is on and
/// has fired without recovering).
pub(super) fn toast_up() -> bool {
    LOW_SPACE_ENABLED.load(Ordering::Relaxed) && !LOW_SPACE_ARMED.load(Ordering::Relaxed)
}

/// Whether the boot volume, where the drive index's databases live, is below the
/// user's low-disk-space threshold. The index pauses while it is
/// (`AppHostPolicy::index_storage_low`). A plain atomic read: the index asks about
/// once a second per writer.
pub(crate) fn index_storage_low() -> bool {
    INDEX_STORAGE_LOW.load(Ordering::Relaxed)
}

/// Feed a fresh boot-volume reading to [`index_storage_low`], logging each edge.
/// A volume with no ceiling can't run out, so it leaves the state alone.
pub(super) fn update_index_storage_low(space: &SpaceInfo) {
    let SpaceInfo::Bounded {
        total_bytes,
        available_bytes,
        ..
    } = *space
    else {
        return;
    };
    let threshold = LOW_SPACE_THRESHOLD_PERCENT.load(Ordering::Relaxed) as f64;
    let free = free_percent(total_bytes, available_bytes);
    let was_low = INDEX_STORAGE_LOW.load(Ordering::Relaxed);
    let low = index_storage_low_after(was_low, free, threshold);
    if low != was_low {
        INDEX_STORAGE_LOW.store(low, Ordering::Relaxed);
        info!(
            "index storage {}: boot volume at {:.1}% free, threshold {}%",
            if low {
                "low, pausing indexing"
            } else {
                "recovered, indexing may resume"
            },
            free,
            threshold
        );
    }
}

/// The pure hysteresis step for [`index_storage_low`]: low below the threshold,
/// back past threshold + [`LOW_SPACE_REARM_MARGIN_PERCENT`], unchanged in between,
/// so a disk hovering at the line can't flap every index on and off.
fn index_storage_low_after(was_low: bool, free_percent: f64, threshold_percent: f64) -> bool {
    if was_low {
        free_percent < threshold_percent + LOW_SPACE_REARM_MARGIN_PERCENT
    } else {
        free_percent < threshold_percent
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
pub(super) fn check_low_space(volume_id: &str, space: &SpaceInfo) {
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

    /// The drive index pauses below the warning's threshold and resumes past its
    /// re-arm margin, so a disk hovering at the line doesn't flap every volume's
    /// index on and off (cmdr-reports#100).
    #[test]
    fn index_storage_low_has_the_warnings_threshold_and_margin() {
        assert!(!index_storage_low_after(false, 5.0, 5.0), "at the line is not below it");
        assert!(index_storage_low_after(false, 0.5, 5.0), "the incident's 0.5% free");
        assert!(
            index_storage_low_after(true, 5.5, 5.0),
            "inside the margin it stays low"
        );
        assert!(!index_storage_low_after(true, 6.0, 5.0), "past the margin it recovers");
        assert!(
            !index_storage_low_after(false, 5.5, 5.0),
            "and inside it, a recovered disk stays clear"
        );
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
