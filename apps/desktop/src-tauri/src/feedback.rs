//! In-app "Send feedback" backend.
//!
//! Validates and ships a beta tester's feedback text to the api-server's `POST /feedback`,
//! which stores it in D1 and pings Discord. No log bundle rides along (that's the error
//! reporter's job); the payload is the text plus app/OS identifiers and an optional
//! reply-to email the user explicitly ticked a box to attach.
//!
//! Returns a typed [`SendFeedbackResult`] the UI branches on (`kind` discriminant), never a
//! message string, per the no-error-string-matching rule. Mirrors `commands/beta_signup.rs`
//! for the network shape and `error_reporter::upload` for the CI / E2E skip gates.

use serde::Serialize;

/// Hard cap on the feedback text, counted in Unicode code points (`.chars().count()`) so it
/// matches the dialog's counter (`Array.from(text).length`) and the server-side validator.
/// Same number as the error reporter's user-note cap.
pub const MAX_FEEDBACK_CHARS: usize = 100_000;

/// Why a feedback text failed local validation. Both map to [`SendFeedbackResult::Invalid`]
/// at the IPC boundary; the split exists so unit tests pin the exact rule that fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackTextError {
    Empty,
    TooLong,
}

/// The send outcome, returned across IPC so the frontend reacts on a typed `kind`
/// discriminant rather than parsing a message. Serializes as `{"kind":"sent"}` /
/// `{"kind":"invalid"}` / `{"kind":"softFailure"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SendFeedbackResult {
    /// The feedback landed on the server.
    Sent,
    /// The text didn't pass validation (empty after trimming, or over the length cap).
    /// The dialog already blocks both, so this is a backstop for bypassed input.
    Invalid,
    /// Something went wrong reaching or talking to the server. The UI shows a gentle
    /// try-again; the user's text stays in the dialog.
    /// E2E builds compile out the network path in `send` (the only constructor of this
    /// variant), so `deny(unused)` needs the cfg-gated allow. The variant must stay even
    /// then: the frontend's generated union type covers all three kinds.
    #[cfg_attr(feature = "playwright-e2e", allow(dead_code))]
    SoftFailure,
}

/// The request body for `POST /feedback`. `email`/`build_mode` serialize as `null` when
/// absent (no `skip_serializing_if`; the server tolerates both `null` and missing keys).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackPayload {
    pub feedback: String,
    pub email: Option<String>,
    pub app_version: String,
    pub os_version: String,
    pub build_mode: String,
}

/// Trim and validate a raw feedback text. Returns the trimmed text ready to ship.
pub fn prepared_feedback_text(raw: &str) -> Result<String, FeedbackTextError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(FeedbackTextError::Empty);
    }
    // Count code points (not bytes, not UTF-16 units) so the cap matches the dialog's
    // counter and the server-side validator.
    if trimmed.chars().count() > MAX_FEEDBACK_CHARS {
        return Err(FeedbackTextError::TooLong);
    }
    Ok(trimmed.to_string())
}

/// Assemble the full payload from a validated text and the optional reply-to email.
/// App version comes from the crate version, OS version from [`crate::platform::os_version`],
/// and `build_mode` from `cfg!(debug_assertions)` (the server tags dev builds `[DEV]` on Discord).
pub fn build_payload(feedback: String, email: Option<String>) -> FeedbackPayload {
    FeedbackPayload {
        feedback,
        email,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: crate::platform::os_version(),
        build_mode: if cfg!(debug_assertions) { "debug" } else { "release" }.to_string(),
    }
}

/// Network timeout for the send request. Mirrors the beta-signup and crash/error reporters.
/// E2E builds compile out the network path in `send` (its only user), hence the cfg-gated allow.
#[cfg_attr(feature = "playwright-e2e", allow(dead_code))]
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// POST the payload to the api-server. Skips the network in CI (env var) and in E2E builds
/// (compile-time), mirroring `error_reporter::upload`: test runs shouldn't pollute the live
/// feedback channel, and failures there are already visible in test output.
pub async fn send(payload: &FeedbackPayload, server_url: &str) -> SendFeedbackResult {
    #[cfg(feature = "playwright-e2e")]
    {
        let _ = (payload, server_url); // the network path is compiled out below
        log::info!(target: "cmdr_lib::feedback", "Skipping feedback send (E2E build)");
        return SendFeedbackResult::Sent;
    }
    #[cfg(not(feature = "playwright-e2e"))]
    {
        if std::env::var("CI").is_ok() {
            log::info!(target: "cmdr_lib::feedback", "Skipping feedback send (CI)");
            return SendFeedbackResult::Sent;
        }

        let client = match reqwest::Client::builder().timeout(SEND_TIMEOUT).build() {
            Ok(c) => c,
            Err(e) => {
                log::warn!(target: "cmdr_lib::feedback", "Couldn't build HTTP client: {e}");
                return SendFeedbackResult::SoftFailure;
            }
        };

        let response = match client.post(server_url).json(payload).send().await {
            Ok(r) => r,
            Err(e) => {
                log::warn!(target: "cmdr_lib::feedback", "Feedback request failed: {e}");
                return SendFeedbackResult::SoftFailure;
            }
        };

        let status = response.status();
        if status.is_success() {
            log::debug!(target: "cmdr_lib::feedback", "Feedback accepted ({status})");
            SendFeedbackResult::Sent
        } else if status == reqwest::StatusCode::BAD_REQUEST {
            SendFeedbackResult::Invalid
        } else {
            log::warn!(target: "cmdr_lib::feedback", "Feedback server returned {status}");
            SendFeedbackResult::SoftFailure
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(prepared_feedback_text("  hello there \n").unwrap(), "hello there");
    }

    #[test]
    fn rejects_empty_and_whitespace_only_text() {
        assert_eq!(prepared_feedback_text(""), Err(FeedbackTextError::Empty));
        assert_eq!(prepared_feedback_text("   \n\t"), Err(FeedbackTextError::Empty));
    }

    #[test]
    fn rejects_text_over_the_cap_and_accepts_text_at_the_cap() {
        let at_cap = "a".repeat(MAX_FEEDBACK_CHARS);
        assert_eq!(prepared_feedback_text(&at_cap).unwrap(), at_cap);
        let over_cap = "a".repeat(MAX_FEEDBACK_CHARS + 1);
        assert_eq!(prepared_feedback_text(&over_cap), Err(FeedbackTextError::TooLong));
    }

    #[test]
    fn counts_code_points_not_utf16_units() {
        // 60k emoji are 120k UTF-16 units but only 60k code points: well under the cap.
        let emoji = "🎉".repeat(60_000);
        assert!(prepared_feedback_text(&emoji).is_ok());
    }

    #[test]
    fn payload_serializes_camel_case_with_null_email_when_absent() {
        let payload = build_payload("great app".to_string(), None);
        let value = serde_json::to_value(&payload).unwrap();
        assert_eq!(value["feedback"], "great app");
        assert!(value["email"].is_null());
        assert_eq!(value["appVersion"], env!("CARGO_PKG_VERSION"));
        assert!(value["osVersion"].as_str().is_some_and(|v| !v.is_empty()));
        assert_eq!(
            value["buildMode"],
            if cfg!(debug_assertions) { "debug" } else { "release" }
        );
    }

    #[test]
    fn payload_carries_the_reply_to_email_when_provided() {
        let payload = build_payload("hi".to_string(), Some("tester@example.com".to_string()));
        let value = serde_json::to_value(&payload).unwrap();
        assert_eq!(value["email"], "tester@example.com");
    }
}
