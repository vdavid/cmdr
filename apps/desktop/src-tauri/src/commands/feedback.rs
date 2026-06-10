//! "Send feedback" Tauri command. Thin IPC wrapper; the validation, payload assembly, and
//! network logic live in [`crate::feedback`].

use crate::feedback::{self, SendFeedbackResult};

/// Server URL for feedback ingestion. Debug builds hit the local Worker; release hits production.
#[cfg(debug_assertions)]
const FEEDBACK_URL: &str = "http://localhost:8787/feedback";
#[cfg(not(debug_assertions))]
const FEEDBACK_URL: &str = "https://api.getcmdr.com/feedback";

/// Sends a beta tester's feedback text (plus an optional reply-to email they chose to attach)
/// to the api-server. Returns a typed result the UI branches on.
///
/// Network-touching but NOT filesystem-touching, so no `blocking_with_timeout` is needed (that's
/// for syscalls that hang on dead mounts). The `reqwest` client carries its own 10 s timeout.
#[tauri::command]
#[specta::specta]
pub async fn send_feedback(feedback_text: String, email: Option<String>) -> SendFeedbackResult {
    let text = match feedback::prepared_feedback_text(&feedback_text) {
        Ok(t) => t,
        Err(e) => {
            log::warn!(target: "cmdr_lib::feedback", "Feedback text rejected locally: {e:?}");
            return SendFeedbackResult::Invalid;
        }
    };
    let payload = feedback::build_payload(text, email);
    feedback::send(&payload, FEEDBACK_URL).await
}
