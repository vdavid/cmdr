//! The one thing a wake says out loud: it staged something for the user to look at.
//!
//! ⚠️ **A separate event from both the turn stream and the indicator's phase**, because it is
//! neither. `AskCmdrTurn` carries a turn's progress to whoever is showing that thread;
//! `AgentWakeStatus` carries the corner's state. This is a one-shot "something is waiting for
//! you", and folding it into a state event would have every reconnecting window re-raise the
//! same toast.
//!
//! ⚠️ **The setting is checked on the FRONTEND** (`askCmdr.wakeToast`). The event says what
//! happened; whether the window makes a noise about it is that window's business, and the
//! settings store already updates live there. Gating the emit instead would mean a user who
//! turns the toast back on mid-wake silently misses the one it was about to raise.

use serde::Serialize;
use tauri_specta::Event;

const LOG_TARGET: &str = "agent::wake";

/// A wake left proposals behind. The user has something to look at, and never had to ask.
///
/// ⚠️ `Serialize` only beside the derive, like [`AgentWakeStatus`](super::AgentWakeStatus):
/// `tauri_specta::Event` wants `DeserializeOwned` solely for its Rust-side `listen`, which
/// nothing here does.
#[derive(Debug, Clone, Copy, Serialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "agent-wake-staged")]
pub struct AgentWakeStaged {
    /// The thread the wake reasoned in, so a reader can go and see WHY it proposed this.
    pub conversation_id: i64,
    /// How many proposals went past on the way. Always at least one: a wake that staged
    /// nothing says nothing.
    pub proposals: u32,
}

/// The app handle the emitter uses, wired once at startup like the turn emitter's. `None`
/// before wiring, which is every unit test, so emitting is a silent no-op there.
static STAGED_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// Point the staged-proposal emitter at the app. Startup only.
pub fn init_wake_staged_emitter(app: &tauri::AppHandle) {
    let _ = STAGED_APP.set(app.clone());
}

/// Say a wake staged `proposals` proposals in `conversation_id`.
///
/// A no-op at zero, so the one caller does not have to remember: nothing staged is exactly the
/// case the user must not be interrupted for.
pub(super) fn announce_staged(conversation_id: i64, proposals: usize) {
    if proposals == 0 {
        return;
    }
    let Some(app) = STAGED_APP.get() else {
        return;
    };
    let staged = AgentWakeStaged {
        conversation_id,
        proposals: proposals.min(u32::MAX as usize) as u32,
    };
    if let Err(e) = staged.emit(app) {
        log::warn!(target: LOG_TARGET, "a staged wake didn't reach the windows: {e}");
    }
}
