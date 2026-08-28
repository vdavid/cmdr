//! Error reporter Tauri commands.
//!
//! Flow A's preview/send pair, a debug-only save-to-disk command, and the two commands that
//! reach a report Flow B already sent (read what went out, add a note to it). Business logic
//! lives in `crate::error_reporter`. These wrappers just shape inputs/outputs for the IPC layer.

use crate::error_reporter::{
    self, AttachedEmail, BundleKind, BundleManifest, BundleRequest, BundleScope, FLOW_A_BUNDLE_CAP_MB,
    settings_defaults::SettingValue,
};
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
    let bundle = error_reporter::build_bundle(&app, flow_a_request(None, user_note, email)?)?;
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
) -> Result<SendResult, String> {
    let bundle = error_reporter::build_bundle(&app, flow_a_request(id, user_note, email)?)?;
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
/// against what it was showing. Errs when nothing was auto-sent this run or the server never
/// handed back an amend key; `can_amend` from [`get_auto_sent_report_preview`] is the flag to
/// branch on, not the message.
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
pub async fn amend_error_report(user_note: Option<String>, email: Option<String>) -> Result<AmendResult, String> {
    let note = validate_user_note(user_note)?;
    // One read of the stash resolves both halves, so the URL below and the credential the
    // request carries can't come from two different reports.
    let target = error_reporter::auto_sent::amend_target()?;
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
    let mut bundle = error_reporter::build_bundle(&app, flow_a_request(id, user_note, email)?)?;
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
) -> Result<BundleRequest, String> {
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

fn validate_user_note(user_note: Option<String>) -> Result<Option<String>, String> {
    match user_note {
        Some(n) if n.chars().count() > MAX_USER_NOTE_CHARS => Err(format!(
            // allowed-pluralize-noun: both counts are guaranteed > MAX_USER_NOTE_CHARS (100_000).
            "User note is too long ({} chars). Maximum is {MAX_USER_NOTE_CHARS} chars.",
            n.chars().count(),
        )),
        other => Ok(other),
    }
}
