//! The wake loop's IPC surface: the live-apply pushes behind its settings and the Ask Cmdr
//! switch, and the seed read the status corner's indicator starts from.
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

/// Tell the gates that `askCmdr.enabled` moved. No value crosses: the send gate reads the
/// setting fresh per send, but the wake loop's readiness is a cached answer
/// (`agent::wake::snapshot`), so without this push a switched-off Ask Cmdr would keep storing
/// (and a switched-on one keep refusing) until something else refreshed it. Switched off, it also
/// stops every running turn: the rail's gate replaces the composer, Stop button and all. Called
/// from `settings-applier.ts`.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_enabled_changed(app: tauri::AppHandle) {
    crate::agent::chat::cancel::stop_all_unless_on(crate::settings::load_ask_cmdr_switch(&app));
    crate::agent::wake::refresh_readiness(&app);
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
