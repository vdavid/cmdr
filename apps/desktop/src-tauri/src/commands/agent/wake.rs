//! The wake loop's IPC surface: the live-apply push behind its three settings, and the seed
//! read the status corner's indicator starts from.
//!
//! The loop owns its own thread, its own inbox, and a timer parked against a deadline, so a
//! settings change has to REACH it rather than being noticed the next time something happens.
//! The push is a message on the never-dropped control lane; nothing here touches the inbox.

use crate::agent::wake::{AgentWakeStatus, WakeControl, send_control, wake_status};

/// Tell the wake loop its settings moved, so it re-reads them and re-arms.
///
/// Called from `settings-applier.ts` for each of the three `askCmdr` cadence rows. ⚠️ **Not
/// optional, and not the same shape as the other two `askCmdr.*` settings**, which the backend
/// reads fresh at send time: these drive a SLEEPING timer. Without this push, turning the
/// proactive gate on would leave a parked scheduler that never notices, and a lengthened
/// cadence would never reach the rows already waiting (the inbox merge is min-only on purpose).
///
/// No value crosses: the loop re-reads `settings.json` itself, so whatever is on disk when the
/// message is serviced wins and two changes racing can't leave the loop on the older one.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_wake_settings_changed() {
    send_control(WakeControl::SettingsChanged);
}

/// What the status corner's wake indicator should show right now: whether a wake is thinking,
/// and which gate is in the way if it can't.
///
/// ⚠️ **A seed, not a poll.** `agent-wake-status` announces every move, and the corner
/// subscribes to it; this exists because a wake already running when the window opens, or a
/// gate that closed before it did, announced itself to nobody. Same shape as the suggestions
/// badge's one read at startup.
#[tauri::command]
#[specta::specta]
pub fn agent_wake_status() -> AgentWakeStatus {
    wake_status()
}
