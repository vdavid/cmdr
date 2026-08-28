//! What the last Flow B auto-send actually shipped, plus the call that adds a note to it.
//!
//! Flow B builds a bundle, uploads it, and emits `error-report-auto-sent`. The toast that
//! event raises offers to show the user what went out and to add a note, so something has to
//! outlive the dispatch: this module is that something, plus the amend request itself.
//!
//! ## What's stashed, and what isn't
//!
//! The report id, the amend credential, and the same preview material
//! `prepare_error_report_preview` returns (manifest, first/last sample lines, redacted-line
//! count, size). **Never the zip bytes.** The standing rule against caching bundle bytes on
//! the Rust side across IPC round-trips is about megabytes of compressed logs living in the
//! process for as long as a dialog might open; a manifest plus a couple of dozen sample lines
//! is a few KB, so it isn't the thing that rule guards against. See
//! `DETAILS.md` § "The command surface".
//!
//! A second auto-send overwrites the first: the toast is deduped to one on screen, so there's
//! only ever one report the user can be looking at. The stash dies with the process, which is
//! right too, since the toast doesn't survive a restart either. Amending doesn't consume
//! anything: the credential stays, so a reporter can come back with a second thought.
//!
//! ## Amending
//!
//! Two steps, because the endpoint is per-report: [`amend_target`] resolves the id and the
//! credential in one read of the stash, the caller turns that id into a URL with
//! [`super::error_report_amend_url`], and [`amend`] spends the target against it. The URL is a
//! parameter for the same reason [`super::upload`] takes one: the endpoint belongs to the
//! caller, and a test can point it at a mock.

use super::{AmendKey, AttachedEmail, BundleManifest};
use crate::IgnorePoison;
use std::sync::Mutex;

/// Preview material for a report that already shipped. Mirrors the fields
/// `prepare_error_report_preview` returns so the dialog can render an auto-sent report with
/// the same component it uses for a manual one.
#[derive(Debug, Clone)]
pub struct AutoSentPreview {
    pub size_bytes: usize,
    pub manifest: BundleManifest,
    pub sample_first: Vec<String>,
    pub sample_last: Vec<String>,
    pub total_redacted_lines: usize,
}

/// The stash's contents. Private: the credential must not leave this module except into the
/// amend request body, so readers get an [`AutoSentSnapshot`] instead.
struct StashedAutoSend {
    id: String,
    amend_key: Option<AmendKey>,
    preview: AutoSentPreview,
}

/// Everything a reader may know about the stashed report: the credential is reduced to
/// `can_amend`, so nothing downstream (an IPC payload, a log line) can carry it by accident.
#[derive(Debug, Clone)]
pub struct AutoSentSnapshot {
    pub id: String,
    pub can_amend: bool,
    pub preview: AutoSentPreview,
}

static STASH: Mutex<Option<StashedAutoSend>> = Mutex::new(None);

/// Remember what an auto-send just shipped. Called from the auto-dispatcher's flush on a
/// successful upload, before it emits `error-report-auto-sent`: the frontend can act on that
/// event immediately, and it needs the stash to already be there.
pub fn record(id: String, amend_key: Option<AmendKey>, preview: AutoSentPreview) {
    let mut guard = STASH.lock_ignore_poison();
    *guard = Some(StashedAutoSend { id, amend_key, preview });
}

/// The most recent auto-sent report, or `None` if nothing has been auto-sent this run.
pub fn snapshot() -> Option<AutoSentSnapshot> {
    let guard = STASH.lock_ignore_poison();
    guard.as_ref().map(|stashed| AutoSentSnapshot {
        id: stashed.id.clone(),
        can_amend: stashed.amend_key.is_some(),
        preview: stashed.preview.clone(),
    })
}

/// The report an amend would land on: its id, plus the credential that authorizes the request.
///
/// Resolved in ONE read of the stash by [`amend_target`], and then handed back to [`amend`], so
/// the URL the caller builds and the credential the request carries can't come from two
/// different reports if an auto-send lands in between. The credential is private: only
/// [`amend`] can spend it, and nothing outside this module can read it back out.
#[derive(Debug)]
pub struct AmendTarget {
    pub id: String,
    key: AmendKey,
}

/// Resolve what an amend would target, or say why there's nothing to amend.
///
/// Callers need the id before [`amend`], because the endpoint is per-report: build the URL with
/// [`super::error_report_amend_url`] and pass the target straight through.
pub fn amend_target() -> Result<AmendTarget, String> {
    let guard = STASH.lock_ignore_poison();
    let stashed = guard
        .as_ref()
        .ok_or("There's no report to add to: nothing was sent automatically this session.")?;
    let key = stashed
        .amend_key
        .clone()
        .ok_or("This report can't take a note. The server didn't hand back a key for it.")?;
    Ok(AmendTarget {
        id: stashed.id.clone(),
        key,
    })
}

/// Add a note (and optionally a reply-to address) to a report an auto-send already shipped.
/// Returns the report's id, so the UI can confirm against what it was showing.
///
/// `server_url` is a parameter for the same reason [`super::upload`] takes one: the endpoint
/// belongs to the caller, and a test can point this at a mock. Production callers build it with
/// [`super::error_report_amend_url`] from the target's id.
///
/// Amending is a Flow A act about a Flow B report: the person is typing right now and pressing a
/// button, which is exactly the per-report consent an address needs, and [`AttachedEmail`] is
/// how that consent is carried rather than assumed. Flow B's own send still never attaches one.
///
/// **A landed amend keeps the credential, so `can_amend` stays true and the user can come back
/// with a second thought.** What this client depends on: the credential stays usable for the
/// life of the report's index entry, and amendments accumulate rather than replace. The server
/// side of that contract, including how long the entry lives, is
/// `apps/api-server/src/telemetry/error-report-amend.ts`. Guarding against a double-click
/// belongs in the frontend disabling its button while the call is in flight, not in discarding
/// a credential that still works.
///
/// Mirrors [`super::upload`]'s error handling: the server's own `{"error": "..."}` body is
/// folded into the returned message, capped, and displayed only. ❌ Never branch on it.
pub async fn amend(
    target: AmendTarget,
    server_url: &str,
    user_note: Option<String>,
    email: Option<AttachedEmail>,
) -> Result<String, String> {
    send_amend(server_url, &target.key, user_note, email).await?;
    Ok(target.id)
}

/// The request body: `{ "amendKey": ..., "note"?: ..., "email"?: ... }`.
///
/// Built by hand rather than from a serializable struct so the one place the credential reaches
/// the wire is a single greppable line: [`AmendKey`] has no `Serialize` impl on purpose.
fn amend_body(key: &AmendKey, user_note: Option<String>, email: Option<AttachedEmail>) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("amendKey".to_string(), key.as_str().into());
    if let Some(note) = user_note {
        body.insert("note".to_string(), note.into());
    }
    if let Some(email) = email {
        body.insert("email".to_string(), email.into_inner().into());
    }
    serde_json::Value::Object(body)
}

/// True when this build must not reach an ingestion server: a `CI` run shouldn't pollute the
/// live channel even if something triggers an amend.
///
/// `cfg!(test)` opts the crate's own unit tests out. They're handed a mock URL they built
/// themselves, and CI sets `CI`, so without this the amend tests would assert nothing on the
/// one runner that matters.
#[cfg(not(feature = "playwright-e2e"))]
fn should_skip_network() -> bool {
    !cfg!(test) && std::env::var("CI").is_ok()
}

/// POST the amend request. Split from [`amend`] so the credential bookkeeping and the network
/// call read separately.
///
/// Skips the network in the same two cases [`super::upload`] does, for the same reasons: the
/// `playwright-e2e` feature compiles the request out (an E2E build is a release build, and its
/// reports would be indistinguishable from real users'), and a `CI` env var short-circuits it.
async fn send_amend(
    server_url: &str,
    key: &AmendKey,
    user_note: Option<String>,
    email: Option<AttachedEmail>,
) -> Result<(), String> {
    let body = amend_body(key, user_note, email);

    #[cfg(feature = "playwright-e2e")]
    {
        let _ = (server_url, body);
        log::info!(
            target: "cmdr_lib::error_reporter",
            "Skipping error report amend (E2E build).",
        );
        // Tail expression, not `return`: under this feature the block below is compiled out,
        // so this block IS the function body.
        Ok(())
    }
    #[cfg(not(feature = "playwright-e2e"))]
    {
        if should_skip_network() {
            log::info!(
                target: "cmdr_lib::error_reporter",
                "Skipping error report amend (CI).",
            );
            return Ok(());
        }

        // Shorter than the 30 s an upload gets: this request carries a note, not a bundle, and
        // someone is watching a dialog while it runs.
        const AMEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
        // Cap on the server's own explanation, matching `upload`. Keeps a stray HTML error page
        // out of a toast.
        const MAX_SERVER_DETAIL_CHARS: usize = 200;

        let client = reqwest::Client::builder()
            .timeout(AMEND_TIMEOUT)
            .build()
            .map_err(|e| format!("HTTP client: {e}"))?;

        let response = client
            .post(server_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("amend request: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let raw = response.text().await.unwrap_or_default();
            let detail: String = raw.trim().chars().take(MAX_SERVER_DETAIL_CHARS).collect();
            if detail.is_empty() {
                return Err(format!("server returned {status}"));
            }
            return Err(format!("server returned {status}: {detail}"));
        }

        log::info!(target: "cmdr_lib::error_reporter", "Added a note to an error report");
        Ok(())
    }
}

/// Serializes every test that drives the stash: it's process-global, so two such tests running
/// in parallel read each other's report. Mirrors `auto_dispatcher::TEST_LOCK`.
#[cfg(test)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub fn clear_for_test() {
    let mut guard = STASH.lock_ignore_poison();
    *guard = None;
}
