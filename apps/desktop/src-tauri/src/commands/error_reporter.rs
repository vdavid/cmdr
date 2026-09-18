//! Error reporter Tauri commands.
//!
//! Flow A's preview/send pair, a debug-only save-to-disk command, and the two commands that
//! reach a report Flow B already sent (read what went out, add a note to it). Business logic
//! lives in `crate::error_reporter`. These wrappers just shape inputs/outputs for the IPC layer.

use crate::error_reporter::{
    self, AttachedEmail, BundleKind, BundleManifest, BundleRequest, BundleScope, FLOW_A_BUNDLE_CAP_MB,
    settings_defaults::SettingValue,
};
use crate::server_request::ServerRequestError;
use serde::Serialize;
use std::collections::HashMap;

/// Server still enforces a 10 MB total payload cap; this is the cheaper client-side
/// guardrail so we don't waste effort building a bundle that'd be rejected.
const MAX_USER_NOTE_CHARS: usize = 100_000;

/// Pushes the FE settings-registry default map to the backend, where it feeds
/// [`crate::error_reporter::ResolvedSettings::from_settings`] so manifests don't
/// duplicate defaults between TypeScript and Rust.
///
/// Called once from `initializeSettings()` in `apps/desktop/src/lib/settings/settings-store.ts`
/// after the registry has loaded. Subsequent calls overwrite (HMR-safe in dev).
/// Failures are silent. The Rust side falls back to hardcoded defaults if the map
/// is missing or doesn't include a given key.
#[tauri::command]
#[specta::specta]
pub fn record_settings_defaults(defaults: HashMap<String, SettingValue>) {
    error_reporter::settings_defaults::record(defaults);
}

/// Records a freeform breadcrumb event for the error-report manifest.
///
/// Called from FE event handlers (navigation, dialog open/close, command dispatch)
/// to add triage context. Empty kinds and over-long messages are dropped silently
/// inside `error_reporter::breadcrumbs::record`. `ctx` is an optional structured
/// payload.
#[tauri::command]
#[specta::specta]
pub fn record_breadcrumb(kind: String, message: String, ctx: Option<serde_json::Value>) {
    error_reporter::breadcrumbs::record(&kind, &message, ctx);
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPayload {
    pub id: String,
    pub size_bytes: usize,
    pub manifest: BundleManifest,
    pub sample_first: Vec<String>,
    pub sample_last: Vec<String>,
    pub total_redacted_lines: usize,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub id: String,
}

/// A report Flow B already sent, as the dialog needs to render it. Same preview fields as
/// [`PreviewPayload`] plus `can_amend`, so one component covers a manual preview and an
/// auto-sent one.
///
/// `can_amend` is the whole reason the amend key isn't in here: the frontend needs to know
/// whether a note can still be added, never the credential that adds it.
#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AutoSentReport {
    pub id: String,
    pub can_amend: bool,
    pub size_bytes: usize,
    pub manifest: BundleManifest,
    pub sample_first: Vec<String>,
    pub sample_last: Vec<String>,
    pub total_redacted_lines: usize,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AmendResult {
    pub id: String,
}

/// Why a Flow A send or an amend didn't land, for the dialog to word from the catalog.
///
/// The request itself fails the way every call to Cmdr's api server does ([`ServerRequestError`]);
/// the other variants are this dialog's own. ❌ No variant carries a sentence: `detail` is for the
/// log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ErrorReportSendError {
    /// The note is over the cap. The dialog blocks this first, so it's a backstop.
    NoteTooLong { max_chars: usize },
    /// The bundle couldn't be built from the logs.
    BundleUnavailable { detail: String },
    /// Nothing was sent automatically this session, or the server didn't hand back a key for the
    /// report that was. `can_amend` from [`get_auto_sent_report_preview`] is the flag to check first.
    NotAmendable,
    /// The request to the api server didn't land.
    Server { failure: ServerRequestError },
}

impl From<ServerRequestError> for ErrorReportSendError {
    fn from(failure: ServerRequestError) -> Self {
        Self::Server { failure }
    }
}

impl std::fmt::Display for ErrorReportSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // allowed-pluralize-noun: max_chars is MAX_USER_NOTE_CHARS (100_000).
            Self::NoteTooLong { max_chars } => write!(f, "the note is over {max_chars} chars"),
            Self::BundleUnavailable { detail } => write!(f, "couldn't build the bundle: {detail}"),
            Self::NotAmendable => f.write_str("no auto-sent report can take a note"),
            Self::Server { failure } => write!(f, "{failure}"),
        }
    }
}

/// Build the bundle in-memory and return preview metadata. No network. No disk writes.
/// The zip bytes are dropped after measuring so we don't ferry MB across IPC.
///
/// The returned `id` is the one the dialog displays and offers a Copy button for, so pass it
/// straight back to [`send_error_report`]: that's how the report lands under the id the user
/// is holding.
///
/// Scope: last hour of log content (see [`BundleScope::flow_a_default`]). Capped at
/// 1 MB compressed during streaming via [`FLOW_A_BUNDLE_CAP_MB`] (early termination,
/// no post-hoc trimming). The trailing `cap_bundle_to_mb` is a defense-in-depth no-op
/// for this path (the streaming pipeline already enforces the cap) but stays in case
/// the manifest grows large enough to push the bundle over by itself.
#[tauri::command]
#[specta::specta]
pub async fn prepare_error_report_preview(
    app: tauri::AppHandle,
    user_note: Option<String>,
    email: Option<String>,
) -> Result<PreviewPayload, String> {
    let request = flow_a_request(None, user_note, email).map_err(|e| e.to_string())?;
    let bundle = error_reporter::build_bundle(&app, request)?;
    let capped = error_reporter::cap_bundle_to_mb(bundle.zip_bytes, FLOW_A_BUNDLE_CAP_MB);
    Ok(PreviewPayload {
        id: bundle.id,
        size_bytes: capped.len(),
        manifest: bundle.manifest,
        sample_first: bundle.sample_first,
        sample_last: bundle.sample_last,
        total_redacted_lines: bundle.total_redacted_lines,
    })
}

/// Re-build the bundle and upload it. Returns the report's ID.
///
/// Pass the `id` the preview returned so the report ships under the id the dialog showed;
/// omit it (or pass something that isn't an `ERR-XXXXX`) and a fresh one is minted.
#[tauri::command]
#[specta::specta]
pub async fn send_error_report(
    app: tauri::AppHandle,
    user_note: Option<String>,
    email: Option<String>,
    id: Option<String>,
) -> Result<SendResult, ErrorReportSendError> {
    let bundle = error_reporter::build_bundle(&app, flow_a_request(id, user_note, email)?)
        .map_err(|detail| ErrorReportSendError::BundleUnavailable { detail })?;
    let capped = error_reporter::cap_bundle_to_mb(bundle.zip_bytes, FLOW_A_BUNDLE_CAP_MB);
    let result = error_reporter::upload(capped, &bundle.manifest, &error_reporter::error_report_url()).await?;
    Ok(SendResult { id: result.id })
}

/// What the most recent Flow B auto-send shipped, or `None` when nothing has been auto-sent
/// this run (the stash dies with the process, and so does the toast that reads it).
///
/// Sync because it's a lock plus a few-KB clone: no filesystem, no network, nothing that can
/// hang the IPC handler thread.
#[tauri::command]
#[specta::specta]
pub fn get_auto_sent_report_preview() -> Option<AutoSentReport> {
    error_reporter::auto_sent::snapshot().map(|snapshot| AutoSentReport {
        id: snapshot.id,
        can_amend: snapshot.can_amend,
        size_bytes: snapshot.preview.size_bytes,
        manifest: snapshot.preview.manifest,
        sample_first: snapshot.preview.sample_first,
        sample_last: snapshot.preview.sample_last,
        total_redacted_lines: snapshot.preview.total_redacted_lines,
    })
}

/// Add a note (and optionally a reply-to address) to the report Flow B already sent.
///
/// Takes no id: there's only ever one stashed report. Returns its id so the UI can confirm
/// against what it was showing. Errs with `NotAmendable` when nothing was auto-sent this run or
/// the server never handed back an amend key; `can_amend` from [`get_auto_sent_report_preview`]
/// is the flag to branch on beforehand.
///
/// Callable more than once for the same report: amendments accumulate, and `can_amend` stays
/// true after one lands. Disable the button while the call is in flight rather than after it
/// returns.
///
/// An address here does NOT break the Flow-B-never-email rule: the person typed it into a
/// dialog and pressed the button, which is the explicit per-report action the invariant is
/// about. [`AttachedEmail`] is what carries that consent into the send.
#[tauri::command]
#[specta::specta]
pub async fn amend_error_report(
    user_note: Option<String>,
    email: Option<String>,
) -> Result<AmendResult, ErrorReportSendError> {
    let note = validate_user_note(user_note)?;
    // One read of the stash resolves both halves, so the URL below and the credential the
    // request carries can't come from two different reports.
    let target = error_reporter::auto_sent::amend_target().ok_or(ErrorReportSendError::NotAmendable)?;
    let url = error_reporter::error_report_amend_url(&target.id);
    let id = error_reporter::auto_sent::amend(target, &url, note, AttachedEmail::from_flow_a_dialog(email)).await?;
    Ok(AmendResult { id })
}

/// Debug-only escape hatch: build the bundle and write it to the app data dir as a `.zip`.
/// Helpful when iterating on the redactor or the manifest format.
///
/// Takes the same `id` as [`send_error_report`] so the dev path can't drift from the real one:
/// the zip on disk is the bundle the send would have shipped, id included.
#[cfg(debug_assertions)]
#[tauri::command]
#[specta::specta]
pub async fn save_error_report_to_disk(
    app: tauri::AppHandle,
    user_note: Option<String>,
    email: Option<String>,
    id: Option<String>,
) -> Result<String, String> {
    let request = flow_a_request(id, user_note, email).map_err(|e| e.to_string())?;
    let mut bundle = error_reporter::build_bundle(&app, request)?;
    bundle.zip_bytes = error_reporter::cap_bundle_to_mb(bundle.zip_bytes, FLOW_A_BUNDLE_CAP_MB);
    let path = error_reporter::save_bundle_to_disk(&app, &bundle)?;
    log::info!(
        target: "cmdr_lib::error_reporter",
        "Saved error report bundle to disk: id={} path={}",
        bundle.manifest.id,
        path.display(),
    );
    Ok(path.display().to_string())
}

/// Shapes a Flow A build request from what the dialog sent, validating the note on the way.
///
/// One place so preview, send, and save-to-disk can't diverge on note validation, id reuse,
/// or how an attached address is wrapped.
fn flow_a_request(
    id: Option<String>,
    user_note: Option<String>,
    email: Option<String>,
) -> Result<BundleRequest, ErrorReportSendError> {
    Ok(BundleRequest {
        kind: BundleKind::User,
        scope: BundleScope::flow_a_default(),
        id,
        user_note: validate_user_note(user_note)?,
        // Flow A IS the dialog: whoever reached this command typed the address and pressed the
        // button in the same interaction. That's the per-report consent `AttachedEmail` carries.
        email: AttachedEmail::from_flow_a_dialog(email),
    })
}

/// Send the log from the session that produced a crash report, as its own error report.
///
/// Reached only from the "crash report sent" toast's action, which is why it's a command of its
/// own rather than a flag on [`send_error_report`]. Two things make it different from Flow A:
///
/// - **Scope is the crash, not now.** `BundleScope::Window { first_error_at: crash_time }` walks
///   back 30 minutes from the crash rather than an hour from this launch. The interesting log
///   lines are in the PREVIOUS session, and however long the machine sat closed between the crash
///   and this launch is exactly how wrong Flow A's "last hour" would be.
/// - **The note is ours, not the user's.** `Log for CRASH-XXXXX` ties the bundle to the crash row
///   so triage can put the two side by side. It carries no user text, so nothing needs previewing.
///
/// [`BundleKind::User`] because a person clicked a button asking for this, which is also what gets
/// it emailed rather than left in Discord. ❌ No email is attached: `AttachedEmail` only comes from
/// the Flow A dialog, where someone typed an address in the same interaction.
#[tauri::command]
#[specta::specta]
pub async fn send_crash_log_report(
    app: tauri::AppHandle,
    crash_short_id: String,
    crash_timestamp: String,
) -> Result<SendResult, ErrorReportSendError> {
    let request = BundleRequest {
        kind: BundleKind::User,
        scope: crash_log_scope(&crash_timestamp),
        id: None,
        user_note: Some(crash_log_note(&crash_short_id)),
        email: None,
    };
    let bundle = error_reporter::build_bundle(&app, request)
        .map_err(|detail| ErrorReportSendError::BundleUnavailable { detail })?;
    let capped = error_reporter::cap_bundle_to_mb(bundle.zip_bytes, FLOW_A_BUNDLE_CAP_MB);
    let result = error_reporter::upload(capped, &bundle.manifest, &error_reporter::error_report_url()).await?;
    Ok(SendResult { id: result.id })
}

/// The note tying a crash-log bundle to its crash row.
///
/// The short id crosses IPC, so it's vetted rather than interpolated: anything that isn't a
/// well-formed `CRASH-XXXXX` is left out entirely rather than written into a note we email
/// ourselves. Losing the cross-reference beats carrying an arbitrary string into the report.
fn crash_log_note(crash_short_id: &str) -> String {
    if crate::short_id::matches(crate::crash_reporter::CRASH_SHORT_ID_PREFIX, crash_short_id) {
        format!("Log for {crash_short_id}")
    } else {
        "Log for a crash report".to_string()
    }
}

/// 30 minutes before the crash through now, or Flow A's last-hour window when the timestamp
/// doesn't parse. The fallback is deliberately still a send: a bundle scoped slightly wrong is
/// worth more than a button that quietly does nothing.
fn crash_log_scope(crash_timestamp: &str) -> BundleScope {
    match chrono::DateTime::parse_from_rfc3339(crash_timestamp) {
        Ok(at) => BundleScope::Window {
            first_error_at: at.with_timezone(&chrono::Utc),
        },
        Err(e) => {
            log::warn!("Crash log report: couldn't read the crash timestamp ({e}), falling back to the recent window");
            BundleScope::flow_a_default()
        }
    }
}

fn validate_user_note(user_note: Option<String>) -> Result<Option<String>, ErrorReportSendError> {
    match user_note {
        Some(n) if n.chars().count() > MAX_USER_NOTE_CHARS => Err(ErrorReportSendError::NoteTooLong {
            max_chars: MAX_USER_NOTE_CHARS,
        }),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_note_over_the_cap_is_turned_down_with_the_cap_and_one_at_it_passes() {
        let at_cap = "a".repeat(MAX_USER_NOTE_CHARS);
        assert_eq!(validate_user_note(Some(at_cap.clone())), Ok(Some(at_cap)));
        assert_eq!(
            validate_user_note(Some("a".repeat(MAX_USER_NOTE_CHARS + 1))),
            Err(ErrorReportSendError::NoteTooLong {
                max_chars: MAX_USER_NOTE_CHARS
            })
        );
    }

    /// The dialog switches on `type` and reads the nested request failure; the shape is the contract.
    #[test]
    fn the_wire_shape_nests_the_request_failure_under_server() {
        let value = serde_json::to_value(ErrorReportSendError::from(ServerRequestError::Refused {
            status: 413,
            detail: "too large".to_string(),
        }))
        .expect("serializes");
        assert_eq!(
            value,
            json!({ "type": "server", "failure": { "type": "refused", "status": 413, "detail": "too large" } })
        );
        assert_eq!(
            serde_json::to_value(ErrorReportSendError::NotAmendable).expect("serializes"),
            json!({ "type": "notAmendable" })
        );
    }
}
