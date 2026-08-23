//! The two settings the wake loop runs on, and the seam they arrive through.
//!
//! [`load`] is the ONE place the loop reads them: `askCmdr.proactive` and `askCmdr.wakeDelay`,
//! both re-read on every `SettingsChanged` control message so a change applies with no restart.
//!
//! ⚠️ **`proactive` ships FALSE.** M1 alone would create threads carrying an English digest
//! frozen in `main.db`, with no indicator, no toast, and no readiness surface, so a release
//! landing before those surfaces exist would hand beta users invisible threads. The end state is
//! David's call; the default here only stages it.
//!
//! ⚠️ **`settings.json` is SPARSE**, so both reads are `Option` and both defaults are spelled
//! out. `unwrap_or_default()` would ship `false` forever for the boolean and a zero-second
//! cadence for the delay, and a zero-second cadence is a wake per batch.

use std::time::Duration;

use tauri::{AppHandle, Runtime};

use super::{DEFAULT_HOT_DELAY, MAX_HOT_DELAY, MIN_HOT_DELAY};

/// The registry default for `askCmdr.proactive`, mirrored here because the store is sparse and
/// an untouched row reaches Rust as an absent key rather than as a value.
const PROACTIVE_DEFAULT: bool = false;

/// What the wake loop needs out of the user's settings. Values, so the loop re-reads on a
/// change rather than holding a live reference to anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakeSettings {
    /// The fourth gate the scheduler checks, beside the three in `readiness.rs`. The middle
    /// tier between "no AI" and "AI that starts conversations".
    pub proactive: bool,
    /// The hot tier's delay, which is the one cadence number the user moves. Warm derives from
    /// it inside `wake_delay`; cold gets none.
    pub hot_delay: Duration,
}

impl Default for WakeSettings {
    fn default() -> Self {
        WakeSettings {
            proactive: PROACTIVE_DEFAULT,
            hot_delay: DEFAULT_HOT_DELAY,
        }
    }
}

/// Read the wake loop's settings fresh. Called at launch and on every `SettingsChanged`
/// control message, so a change applies without a restart.
pub fn load<R: Runtime>(app: &AppHandle<R>) -> WakeSettings {
    from_parts(
        crate::settings::load_ask_cmdr_proactive(app),
        crate::settings::load_ask_cmdr_wake_delay_secs(app),
    )
}

/// Turn what the store had (or didn't) into the settings the loop runs on. Pure, so the two
/// traps below are testable without an app handle: the sparse-store default, and a cadence off
/// the slider's track.
fn from_parts(proactive: Option<bool>, hot_delay_secs: Option<u64>) -> WakeSettings {
    WakeSettings {
        proactive: proactive.unwrap_or(PROACTIVE_DEFAULT),
        hot_delay: hot_delay_secs
            .map(|secs| Duration::from_secs(secs).clamp(MIN_HOT_DELAY, MAX_HOT_DELAY))
            .unwrap_or(DEFAULT_HOT_DELAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::wake::WAKE_DELAY_STOPS;

    /// ⚠️ M1 ships the proactive gate CLOSED on purpose. A release landing between M1 and M2
    /// would otherwise create threads the user has no indicator, toast, or readiness surface to
    /// find. Flipping this is M2's deliberate act, not a side effect of wiring the setting.
    #[test]
    fn the_proactive_gate_ships_closed() {
        assert!(!WakeSettings::default().proactive);
    }

    /// The cadence starts at the slider's attentive end, which is what `DEFAULT_HOT_DELAY`
    /// means; the loop must not invent a different one.
    #[test]
    fn the_cadence_starts_at_the_pipelines_own_default() {
        assert_eq!(WakeSettings::default().hot_delay, DEFAULT_HOT_DELAY);
    }

    /// ⚠️ **The sparse-store trap.** `settings.json` holds only what an actor explicitly set,
    /// so both keys are absent for every user who has never opened the row. `unwrap_or_default`
    /// on the two `Option`s would ship `false` and a zero-second cadence forever; the defaults
    /// have to be the registry's own.
    #[test]
    fn an_untouched_settings_file_reads_as_the_registry_defaults() {
        assert_eq!(from_parts(None, None), WakeSettings::default());
    }

    /// What the user actually chose wins, including turning the gate ON: the staged default is
    /// a default, not a lock.
    #[test]
    fn an_explicit_choice_beats_the_default_in_both_directions() {
        assert!(from_parts(Some(true), None).proactive);
        assert!(!from_parts(Some(false), None).proactive);
        assert_eq!(from_parts(None, Some(300)).hot_delay, Duration::from_secs(300));
    }

    /// A hand-edited `settings.json` is the only way a value off the slider's track gets here,
    /// and both ends are load-bearing: below the shortest stop the agent would wake on its own
    /// noise, and above the longest the warm tier's arithmetic leaves the cap behind.
    #[test]
    fn a_cadence_off_the_sliders_track_is_pulled_back_onto_it() {
        assert_eq!(from_parts(None, Some(1)).hot_delay, MIN_HOT_DELAY);
        assert_eq!(from_parts(None, Some(u64::MAX)).hot_delay, MAX_HOT_DELAY);
        assert_eq!(
            from_parts(None, Some(WAKE_DELAY_STOPS[0])).hot_delay,
            MIN_HOT_DELAY,
            "and the shortest stop itself survives untouched"
        );
    }
}
