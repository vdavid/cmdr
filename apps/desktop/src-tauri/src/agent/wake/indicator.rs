//! What the status corner says about the proactive agent, and the one event that moves it.
//!
//! Two facts, one shape, because they answer the same question ("is the corner showing
//! anything, and what?"): whether a wake is thinking right now, and which of the three gates is
//! in the way if it can't. They move on different triggers — a wake starting and finishing, a
//! consent or key or disk-access change — but a subscriber that had to reconcile two events
//! would render a state neither of them meant.
//!
//! ⚠️ **A separate event from the turn stream** (`agent/chat/stream.rs`). That one carries a
//! turn's PROGRESS to whoever is showing that thread; this one carries the indicator's phase to
//! a corner that is showing no thread at all. Folding them would make the corner subscribe to
//! every text delta of every rail send.
//!
//! ⚠️ **The corner's stop button is not a new cancel.** A wake registers its token in
//! `agent::chat::cancel` like any other turn, so the frontend calls `askCmdrCancel` with the
//! conversation id this event carries.

use std::sync::atomic::{AtomicI64, Ordering};

use serde::Serialize;
use tauri_specta::Event;

use super::readiness::WakeReadiness;
use super::readiness_snapshot;

const LOG_TARGET: &str = "agent::wake";

/// The thread the running wake is writing into, or [`NO_WAKE`] when none is.
///
/// An atomic rather than a lock: it is read by the IPC command on an arbitrary thread and
/// written by the wake thread, and neither may wait on the other.
static THINKING_IN: AtomicI64 = AtomicI64::new(NO_WAKE);

/// Sentinel for "no wake is running". Conversation ids are `INTEGER PRIMARY KEY` rowids, so
/// they are always positive and 0 can never collide with one.
const NO_WAKE: i64 = 0;

// ── The wire shapes ────────────────────────────────────────────────────────────

/// What the status corner's wake indicator shows right now.
///
/// ⚠️ `Serialize` only, like [`AskCmdrTurn`](crate::agent::chat::stream::AskCmdrTurn):
/// `tauri_specta::Event` wants `DeserializeOwned` solely for its Rust-side `listen`, which
/// nothing here does.
#[derive(Debug, Clone, Copy, Serialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "agent-wake-status")]
pub struct AgentWakeStatus {
    pub phase: WakePhase,
    pub readiness: WakeReadinessView,
}

/// Whether a wake is thinking, and in which thread.
///
/// A tagged enum rather than a nullable id beside a boolean, so "thinking with no thread" and
/// "idle with a thread" are unrepresentable: the corner's click target is exactly the id in
/// the `Thinking` arm.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(tag = "phase", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WakePhase {
    /// Nothing is running. The corner shows a readiness gap or nothing at all.
    Idle,
    /// A wake is on a provider right now, in this thread. The corner offers a way in and a way
    /// to stop it.
    Thinking { conversation_id: i64 },
}

/// The wire form of [`WakeReadiness`]. A separate type because the pure core's enum carries no
/// serde derives on purpose: `readiness.rs` is values-in-values-out and knows nothing about a
/// wire.
#[derive(Debug, Clone, Copy, Serialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WakeReadinessView {
    Ready,
    NeedsConsent,
    NeedsFullDiskAccess,
    NeedsApiKey,
}

impl From<WakeReadiness> for WakeReadinessView {
    fn from(readiness: WakeReadiness) -> Self {
        match readiness {
            WakeReadiness::Ready => Self::Ready,
            WakeReadiness::NeedsConsent => Self::NeedsConsent,
            WakeReadiness::NeedsFullDiskAccess => Self::NeedsFullDiskAccess,
            WakeReadiness::NeedsApiKey => Self::NeedsApiKey,
        }
    }
}

// ── Reading and moving it ──────────────────────────────────────────────────────

/// What the corner should show right now. The frontend seeds itself with this once at startup,
/// the way the suggestions badge seeds from `list_suggested_ops`: a wake already running when
/// the window opens announced itself before anyone was listening.
pub fn wake_status() -> AgentWakeStatus {
    let id = THINKING_IN.load(Ordering::Relaxed);
    AgentWakeStatus {
        phase: if id == NO_WAKE {
            WakePhase::Idle
        } else {
            WakePhase::Thinking { conversation_id: id }
        },
        readiness: readiness_snapshot().into(),
    }
}

/// Say a wake has started thinking in `conversation_id`.
pub(super) fn note_wake_started(conversation_id: i64) {
    THINKING_IN.store(conversation_id, Ordering::Relaxed);
    emit_status();
}

/// Say the wake is over, whatever it ended as. Called from the wake thread's exit path, so a
/// turn that failed, was cancelled, or took its own thread away still clears the corner.
pub(super) fn note_wake_finished() {
    THINKING_IN.store(NO_WAKE, Ordering::Relaxed);
    emit_status();
}

/// Announce the current status to every window. Called on both moves and by
/// [`refresh_readiness`](super::refresh_readiness) when a gate changes.
pub(crate) fn emit_status() {
    let Some(app) = STATUS_APP.get() else {
        return;
    };
    if let Err(e) = wake_status().emit(app) {
        log::warn!(target: LOG_TARGET, "the wake status didn't reach the windows: {e}");
    }
}

/// The app handle the emitter uses, wired once at startup like the turn emitter's. `None`
/// before wiring, which is every unit test, so emitting is a silent no-op there.
static STATUS_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// Point the wake-status emitter at the app. Startup only.
pub fn init_wake_status_emitter(app: &tauri::AppHandle) {
    let _ = STATUS_APP.set(app.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corner's click target is the id in the `Thinking` arm, so a finished wake has to
    /// take it away: clicking a stale id would open a thread a quiet wake already deleted.
    #[test]
    fn a_finished_wake_leaves_no_thread_for_the_corner_to_open() {
        note_wake_started(4_242);
        assert!(matches!(
            wake_status().phase,
            WakePhase::Thinking { conversation_id: 4_242 }
        ));

        note_wake_finished();
        assert!(matches!(wake_status().phase, WakePhase::Idle));
    }

    /// Every gate has to survive the mapping onto the wire: a collapsed arm would have the
    /// corner naming the wrong gap, which sends the user to the wrong screen.
    #[test]
    fn every_readiness_state_has_its_own_wire_token() {
        assert_eq!(WakeReadinessView::from(WakeReadiness::Ready), WakeReadinessView::Ready);
        assert_eq!(
            WakeReadinessView::from(WakeReadiness::NeedsConsent),
            WakeReadinessView::NeedsConsent
        );
        assert_eq!(
            WakeReadinessView::from(WakeReadiness::NeedsFullDiskAccess),
            WakeReadinessView::NeedsFullDiskAccess
        );
        assert_eq!(
            WakeReadinessView::from(WakeReadiness::NeedsApiKey),
            WakeReadinessView::NeedsApiKey
        );
    }
}
