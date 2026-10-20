//! In-app "Send feedback" backend.
//!
//! Validates and ships a beta tester's feedback text to the api-server's `POST /feedback`,
//! which stores it in D1; the cron files its card and mails it in the digest. The payload is
//! the text plus app/OS identifiers and an optional reply-to email the user explicitly ticked
//! a box to attach.
//!
//! When the user also ticks "Include the past five hours of logs", the logs travel the error
//! reporter's own path ([`send_with_optional_logs`]): the same Flow A bundle, redaction, and
//! upload, under an `ERR-XXXXX` the feedback payload names so the server can show the two as
//! one card. The feedback goes FIRST and the logs only once it landed, so a failed send
//! uploads nothing and a retry can't leave a second bundle behind.
//!
//! Returns a typed [`SendFeedbackResult`] the UI branches on (`kind` discriminant), never a
//! message string, per the no-error-string-matching rule. Mirrors `commands/beta_signup.rs`
//! for the network shape and `error_reporter::upload` for the CI / E2E skip gates.

use crate::error_reporter::{self, BundleKind, BundleRequest, BundleScope, FLOW_A_BUNDLE_CAP_MB};
use crate::managed_policy::Egress;
use crate::server_request::{self, ServerRequestError};
use serde::Serialize;
use std::future::Future;

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

/// The note on a bundle sent with feedback. Ours, not the user's (their words are the
/// feedback itself), so triage can tell these logs from a hand-written report at a glance.
pub const LOGS_NOTE: &str = "Logs sent with in-app feedback";

/// The send outcome, returned across IPC so the frontend reacts on a typed `kind`
/// discriminant rather than parsing a message. Serializes as `{"kind":"sent"}` /
/// `{"kind":"sentWithoutLogs"}` / `{"kind":"invalid"}` / `{"kind":"softFailure"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SendFeedbackResult {
    /// The feedback landed on the server, with its logs when the user asked for them.
    Sent,
    /// The feedback landed, but the logs the user asked to include didn't: the bundle
    /// couldn't be built or uploaded, or the organization's policy turns error reports off.
    /// The UI thanks them and says so; nothing is retried, since the feedback is already in.
    SentWithoutLogs,
    /// The text didn't pass validation (empty after trimming, or over the length cap).
    /// The dialog already blocks both, so this is a backstop for bypassed input.
    Invalid,
    /// Something went wrong reaching or talking to the server. The UI shows a gentle
    /// try-again; the user's text stays in the dialog.
    /// E2E builds compile out the network path in `send` (the only constructor of this
    /// variant), so `deny(unused)` needs the cfg-gated allow. The variant must stay even
    /// then: the frontend's generated union type covers all three kinds.
    #[cfg_attr(
        feature = "playwright-e2e",
        allow(
            dead_code,
            reason = "E2E builds compile out `send`'s network path, the only constructor"
        )
    )]
    SoftFailure,
}

/// The request body for `POST /feedback`. `email`/`error_report_id` serialize as `null` when
/// absent (no `skip_serializing_if`; the server tolerates both `null` and missing keys).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackPayload {
    pub feedback: String,
    pub email: Option<String>,
    pub app_version: String,
    pub os_version: String,
    pub build_mode: String,
    /// The `ERR-XXXXX` the logs will be uploaded under, when the user asked to include them.
    pub error_report_id: Option<String>,
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

/// Assemble the full payload from a validated text, the optional reply-to email, and the id
/// the logs will travel under. App version comes from the crate version, OS version from
/// [`crate::platform::os_version`], and `build_mode` from `cfg!(debug_assertions)` (the server
/// keeps debug builds off the board and out of the digest).
pub fn build_payload(feedback: String, email: Option<String>, error_report_id: Option<String>) -> FeedbackPayload {
    FeedbackPayload {
        feedback,
        email,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: crate::platform::os_version(),
        build_mode: if cfg!(debug_assertions) { "debug" } else { "release" }.to_string(),
        error_report_id,
    }
}

/// Send the feedback, and the logs with it when `include_logs` is set and the organization's
/// policy allows error reports.
///
/// The logs are a Flow A bundle ([`BundleScope::flow_a_default`], the past five hours) under a
/// freshly minted `ERR-XXXXX`, with [`LOGS_NOTE`] as its note and no reply-to: the address, if
/// any, rides on the feedback, and `AttachedEmail` belongs to the error-report dialog.
pub async fn send_with_optional_logs<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    text: String,
    email: Option<String>,
    include_logs: bool,
    server_url: &str,
) -> SendFeedbackResult {
    let logs_allowed = include_logs && server_request::check_policy(Egress::ErrorReport).await.is_ok();
    let report_id = logs_allowed.then(error_reporter::generate_short_id);
    let payload = build_payload(text, email, report_id);
    let result = send_then_attach_logs(&payload, server_url, |id| upload_logs(app, id)).await;
    // Asked for logs the policy won't send: the feedback is in, the logs aren't, and the dialog
    // says so rather than claim they went. (The dialog hides the box under the policy, so this
    // is a backstop.)
    if include_logs && !logs_allowed && result == SendFeedbackResult::Sent {
        return SendFeedbackResult::SentWithoutLogs;
    }
    result
}

/// [`send`] the payload, then, only once it landed and only when it names a report,
/// `attach_logs(id)`. `attach_logs` answers whether the logs landed too.
///
/// Feedback first because the feedback is what the person wrote: it must never wait on, or be
/// lost to, a bundle (error-report intake can be paused or over its daily budget). And a failed
/// feedback send leaves nothing behind, so the dialog's retry is clean.
pub async fn send_then_attach_logs<F, Fut>(
    payload: &FeedbackPayload,
    server_url: &str,
    attach_logs: F,
) -> SendFeedbackResult
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = bool>,
{
    let result = send(payload, server_url).await;
    match (&result, &payload.error_report_id) {
        (SendFeedbackResult::Sent, Some(id)) if !attach_logs(id.clone()).await => SendFeedbackResult::SentWithoutLogs,
        _ => result,
    }
}

/// Build and upload the Flow A bundle under `id`. Answers whether it landed; the why goes to
/// the log.
async fn upload_logs<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: String) -> bool {
    let request = BundleRequest {
        kind: BundleKind::User,
        scope: BundleScope::flow_a_default(),
        id: Some(id),
        user_note: Some(LOGS_NOTE.to_string()),
        email: None,
    };
    let bundle = match error_reporter::build_bundle(app, request).await {
        Ok(bundle) => bundle,
        Err(detail) => {
            log::warn!(target: "cmdr_lib::feedback", "Couldn't build the logs for feedback: {detail}");
            return false;
        }
    };
    let capped = error_reporter::cap_bundle_to_mb(bundle.zip_bytes, FLOW_A_BUNDLE_CAP_MB);
    match error_reporter::upload(capped, &bundle.manifest, &error_reporter::error_report_url()).await {
        Ok(result) => {
            log::info!(target: "cmdr_lib::feedback", "Sent the logs with the feedback as {}", result.id);
            true
        }
        Err(ServerRequestError::BlockedByPolicy) => false,
        Err(failure) => {
            log::warn!(target: "cmdr_lib::feedback", "Couldn't send the logs with the feedback: {failure}");
            false
        }
    }
}

/// Network timeout for the send request. Mirrors the beta-signup and crash/error reporters.
/// E2E builds compile out the network path in `send` (its only user), hence the cfg-gated allow.
#[cfg_attr(
    feature = "playwright-e2e",
    allow(dead_code, reason = "E2E builds compile out `send`'s network path, its only user")
)]
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// POST the payload to the api-server. Skips the network in CI (env var) and in E2E builds
/// (compile-time), mirroring `error_reporter::upload`: test runs shouldn't pollute the live
/// feedback channel, and failures there are already visible in test output.
pub async fn send(payload: &FeedbackPayload, server_url: &str) -> SendFeedbackResult {
    #[cfg(feature = "playwright-e2e")]
    {
        let _ = (payload, server_url); // the network path is compiled out below
        log::info!(target: "cmdr_lib::feedback", "Skipping feedback send (E2E build)");
        // Tail expression, not `return`: under this feature the block below is compiled
        // out, so this block IS the function body and a `return` would be redundant.
        SendFeedbackResult::Sent
    }
    #[cfg(not(feature = "playwright-e2e"))]
    {
        // Not under `cfg(test)`: the unit tests send to a local fake server, never the live one.
        if !cfg!(test) && std::env::var("CI").is_ok() {
            log::info!(target: "cmdr_lib::feedback", "Skipping feedback send (CI)");
            return SendFeedbackResult::Sent;
        }

        let client = match cmdr_http::client_builder().timeout(SEND_TIMEOUT).build() {
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
        let payload = build_payload("great app".to_string(), None, None);
        let value = serde_json::to_value(&payload).unwrap();
        assert_eq!(value["feedback"], "great app");
        assert!(value["email"].is_null());
        assert!(value["errorReportId"].is_null());
        assert_eq!(value["appVersion"], env!("CARGO_PKG_VERSION"));
        assert!(value["osVersion"].as_str().is_some_and(|v| !v.is_empty()));
        assert_eq!(
            value["buildMode"],
            if cfg!(debug_assertions) { "debug" } else { "release" }
        );
    }

    #[test]
    fn payload_carries_the_reply_to_email_when_provided() {
        let payload = build_payload("hi".to_string(), Some("tester@example.com".to_string()), None);
        let value = serde_json::to_value(&payload).unwrap();
        assert_eq!(value["email"], "tester@example.com");
    }

    #[test]
    fn payload_names_the_error_report_carrying_the_logs() {
        let payload = build_payload("it hung".to_string(), None, Some("ERR-AB23X".to_string()));
        let value = serde_json::to_value(&payload).unwrap();
        assert_eq!(value["errorReportId"], "ERR-AB23X");
    }

    /// The feedback-then-logs order, against a fake `/feedback`. The network path is compiled
    /// out of E2E builds, so these only exist without that feature.
    #[cfg(not(feature = "playwright-e2e"))]
    mod with_logs {
        use super::*;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use wiremock::matchers::{body_partial_json, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        async fn feedback_server(status: u16) -> MockServer {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            server
        }

        fn payload_with_logs() -> FeedbackPayload {
            build_payload("it hung".to_string(), None, Some("ERR-AB23X".to_string()))
        }

        #[tokio::test]
        async fn uploads_the_logs_only_after_the_feedback_landed_and_under_its_id() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(body_partial_json(serde_json::json!({ "errorReportId": "ERR-AB23X" })))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let result = send_then_attach_logs(&payload_with_logs(), &server.uri(), |id| async move {
                assert_eq!(id, "ERR-AB23X");
                true
            })
            .await;

            assert_eq!(result, SendFeedbackResult::Sent);
        }

        #[tokio::test]
        async fn sends_no_logs_when_the_feedback_did_not_land() {
            let server = feedback_server(503).await;
            let attached = Arc::new(AtomicBool::new(false));
            let flag = attached.clone();

            let result = send_then_attach_logs(&payload_with_logs(), &server.uri(), |_| async move {
                flag.store(true, Ordering::SeqCst);
                true
            })
            .await;

            assert_eq!(result, SendFeedbackResult::SoftFailure);
            assert!(!attached.load(Ordering::SeqCst), "a retry would upload a second bundle");
        }

        #[tokio::test]
        async fn says_the_feedback_went_without_its_logs_when_the_upload_did_not_land() {
            let server = feedback_server(204).await;

            let result = send_then_attach_logs(&payload_with_logs(), &server.uri(), |_| async { false }).await;

            assert_eq!(result, SendFeedbackResult::SentWithoutLogs);
        }

        #[tokio::test]
        async fn never_attaches_logs_to_feedback_that_names_no_report() {
            let server = feedback_server(204).await;
            let payload = build_payload("great app".to_string(), None, None);

            let result = send_then_attach_logs(&payload, &server.uri(), |_| async {
                panic!("no report id, so no logs to send");
            })
            .await;

            assert_eq!(result, SendFeedbackResult::Sent);
        }
    }
}
