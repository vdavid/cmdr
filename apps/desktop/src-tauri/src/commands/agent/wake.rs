//! The wake loop's IPC surface: the seed read the status corner's indicator starts from.
//!
//! No push command lives here: a change to the Ask Cmdr switch or the proactive cadence reaches
//! the loop from the settings store itself (`settings::store_watch` → `agent::follow_settings_move`),
//! once `settings.json` holds it, whichever window made it.

use crate::agent::wake::{AgentWakeStatus, wake_status};

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
