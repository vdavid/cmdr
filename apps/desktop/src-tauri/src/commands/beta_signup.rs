//! Beta-tester signup Tauri command.
//!
//! Subscribes a beta contact email to the mailing list via the api-server's `POST /beta-signup`.
//! The whole point is the privacy invariant: this sends ONLY the email, NEVER an install id of any
//! kind (no `anal_`, no `diag_`), so the email and the analytics ids never co-occur on our servers.
//!
//! Returns a typed [`BetaSignupResult`] the UI branches on (`kind` discriminant), never a message
//! string, per the no-error-string-matching rule. Backend-does-network keeps the email off any path
//! that also carries an analytics id.

use serde::Serialize;

/// Server URL for beta signup. Debug builds hit the local Worker; release hits production.
#[cfg(debug_assertions)]
const BETA_SIGNUP_URL: &str = "http://localhost:8787/beta-signup";
#[cfg(not(debug_assertions))]
const BETA_SIGNUP_URL: &str = "https://api.getcmdr.com/beta-signup";

/// Network timeout for the signup request. Mirrors the crash/error reporters.
const BETA_SIGNUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The signup outcome, returned across IPC so the frontend reacts on a typed `kind` discriminant
/// rather than parsing a message (see the `no-string-matching` rule). Serializes as
/// `{"kind":"subscribed"}` / `{"kind":"invalidEmail"}` / `{"kind":"softFailure"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BetaSignupResult {
    /// The address was accepted. Listmonk sends its own double-opt-in confirmation email; the UI
    /// tells the user to check their inbox. Identical for new and already-subscribed addresses (the
    /// server never reveals which, to avoid enumeration).
    Subscribed,
    /// The address didn't pass the server's email-shape check.
    InvalidEmail,
    /// Something went wrong reaching or talking to the signup service. The UI shows a gentle
    /// try-again. Covers a network failure, a non-2xx server response, or a missing-config 500.
    SoftFailure,
}

/// The signup request body: ONLY the email. No install id field exists on this struct, so an
/// analytics or diagnostics id can never be attached, by construction.
#[derive(Debug, Serialize)]
struct BetaSignupRequest<'a> {
    email: &'a str,
}

/// Subscribes a beta contact email to the mailing list. Sends ONLY the email to the api-server,
/// which forwards it to Listmonk for double opt-in. Returns a typed result the UI branches on.
///
/// Network-touching but NOT filesystem-touching, so no `blocking_with_timeout` is needed (that's for
/// syscalls that hang on dead mounts). The `reqwest` client carries its own 10 s timeout.
#[tauri::command]
#[specta::specta]
pub async fn beta_signup(email: String) -> BetaSignupResult {
    let client = match reqwest::Client::builder().timeout(BETA_SIGNUP_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            log::warn!(target: "beta_signup", "Couldn't build HTTP client: {e}");
            return BetaSignupResult::SoftFailure;
        }
    };

    let response = match client
        .post(BETA_SIGNUP_URL)
        .json(&BetaSignupRequest { email: &email })
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!(target: "beta_signup", "Signup request failed: {e}");
            return BetaSignupResult::SoftFailure;
        }
    };

    let status = response.status();
    if status.is_success() {
        log::debug!(target: "beta_signup", "Beta signup accepted ({status})");
        BetaSignupResult::Subscribed
    } else if status == reqwest::StatusCode::BAD_REQUEST {
        BetaSignupResult::InvalidEmail
    } else {
        log::warn!(target: "beta_signup", "Beta signup server returned {status}");
        BetaSignupResult::SoftFailure
    }
}
