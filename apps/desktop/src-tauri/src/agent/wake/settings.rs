//! The two settings the wake loop runs on, and the seam they arrive through.
//!
//! ⚠️ **`askCmdr.proactive` and `askCmdr.wakeDelay` do not exist in the registry yet.** They are
//! M1 item 7. This is the seam they land in: [`load`] is the ONE place the loop reads them, so
//! wiring the real registry entries means changing the two bodies here and nothing else.
//!
//! ⚠️ **`proactive` ships FALSE.** M1 alone would create threads carrying an English digest
//! frozen in `main.db`, with no indicator, no toast, and no readiness surface, so a release
//! landing before M2 would hand beta users invisible threads. The end state is David's call; the
//! default here only stages it.
//!
//! ⚠️ When the registry entries land, `settings.json` is SPARSE, so the Rust loader needs an
//! explicit `.unwrap_or(...)` matching the registry default. `unwrap_or_default()` would ship
//! `false` forever for the boolean and a zero-second cadence for the delay.

use std::time::Duration;

use tauri::{AppHandle, Runtime};

use super::DEFAULT_HOT_DELAY;

/// Whether the agent may start conversations on its own, before the setting exists to say so.
const PROACTIVE_UNTIL_THE_SETTING_LANDS: bool = false;

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
            proactive: PROACTIVE_UNTIL_THE_SETTING_LANDS,
            hot_delay: DEFAULT_HOT_DELAY,
        }
    }
}

/// Read the wake loop's settings fresh. Called at launch and on every `SettingsChanged`
/// control message, so a change applies without a restart.
pub fn load<R: Runtime>(app: &AppHandle<R>) -> WakeSettings {
    // ⚠️ The registry entries are item 7's. Until they exist there is nothing to read, and the
    // honest answer is the staged default rather than a guess dressed as a user choice.
    let _ = app;
    WakeSettings::default()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
