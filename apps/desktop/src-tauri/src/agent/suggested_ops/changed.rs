//! The one signal that says the pending suggestion set moved.
//!
//! The status-corner indicator is always mounted and shows a live pending count, and the
//! review dialog needs to know when the agent amends a group somebody has open. Both are
//! subscribe-shaped questions, and without an event both would have to poll `main.db` on a
//! timer — against principle 5 and against subscribe-don't-poll, for a store that changes a
//! handful of times a day.
//!
//! ❌ No path, file name, rationale, or selector pattern ever rides on this. It crosses to
//! every window, and `main.db` is a map of the user's life that stays local; the ids and
//! counts here are enough to render an indicator and to decide whether an open group is the
//! one that moved.

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use super::super::store::AgentStoreError;
use super::super::store::proposals::count_pending;
use super::super::types::ProposalStatus;
use rusqlite::Connection;

/// Why the pending set changed.
///
/// The count alone can't tell these apart, and the dialog's recovery differs: an amend under
/// an open group needs the non-destructive "this changed" affordance, while an approval of
/// that same group means the review is over. Same id, different affordance, so the reason
/// travels rather than being inferred from a follow-up status query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionChange {
    /// A sweep landed, so one or more groups are newly pending.
    Proposed,
    /// The agent re-proposed a group that was already pending; its ops may be different.
    Amended,
    /// The user approved a group and its ops went to the queue.
    Approved,
    /// The user rejected a group.
    Rejected,
}

/// The pending suggestion set changed.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "suggestions-changed")]
pub struct SuggestionsChanged {
    /// How many groups are pending now, so the indicator renders without a follow-up query.
    pub pending_group_count: u64,
    /// How many ops those groups hold between them. Free: it is the same `COUNT(*)` shape.
    pub pending_op_count: u64,
    /// The group this change was about, when it was about one. `None` for a sweep that
    /// landed several at once. It is what lets an open review tell "the group I am looking at
    /// moved" from "something else appeared".
    pub group_id: Option<i64>,
    pub reason: SuggestionChange,
}

/// The app handle the emitter uses, wired once at startup like the operation manager's.
/// `None` before wiring, which is every unit test, so emitting is a silent no-op there.
static SUGGESTIONS_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// Point the emitter at the app. Startup only.
pub fn init_suggestions_event_emitter(app: &tauri::AppHandle) {
    let _ = SUGGESTIONS_APP.set(app.clone());
}

/// Announce that the pending set changed, counting it fresh from the store.
///
/// Counts come from `COUNT(*)`, never from loading rows: a group of 60 000 ops is legitimate
/// and an indicator must not cost 60 000 rows to draw.
///
/// A count that can't be read is logged and dropped rather than propagated. This is a UI
/// notification: failing the approval that just succeeded, because the badge could not be
/// refreshed, would be the tail wagging the dog.
pub fn announce(conn: &Connection, reason: SuggestionChange, group_id: Option<i64>) {
    match pending_counts(conn) {
        Ok((pending_group_count, pending_op_count)) => emit(SuggestionsChanged {
            pending_group_count,
            pending_op_count,
            group_id,
            reason,
        }),
        Err(e) => log::warn!(
            target: "agent::suggested_ops",
            "couldn't count pending suggestions after {reason:?}: {e}"
        ),
    }
}

fn emit(event: SuggestionsChanged) {
    let Some(app) = SUGGESTIONS_APP.get() else {
        return;
    };
    let _ = event.emit(app);
}

/// How many groups are pending, and how many ops they hold between them.
fn pending_counts(conn: &Connection) -> Result<(u64, u64), AgentStoreError> {
    count_pending(conn, ProposalStatus::Pending)
}
