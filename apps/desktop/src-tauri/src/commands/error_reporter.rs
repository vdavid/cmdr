//! Error reporter Tauri commands.
//!
//! Two preview/send commands plus a debug-only save-to-disk command. Business logic lives
//! in `crate::error_reporter` — these wrappers just shape inputs/outputs for the IPC layer.

use crate::error_reporter::{self, BundleKind, BundleManifest, BundleScope, FLOW_A_BUNDLE_CAP_MB};
use serde::Serialize;

/// Server URL for error report ingestion. Mirrors the crash reporter pattern.
#[cfg(debug_assertions)]
const ERROR_REPORT_URL: &str = "http://localhost:8787/error-report";

#[cfg(not(debug_assertions))]
const ERROR_REPORT_URL: &str = "https://api.getcmdr.com/error-report";

/// Server still enforces a 10 MB total payload cap; this is the cheaper client-side
/// guardrail so we don't waste effort building a bundle that'd be rejected.
const MAX_USER_NOTE_CHARS: usize = 100_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPayload {
    pub id: String,
    pub size_bytes: usize,
    pub manifest: BundleManifest,
    pub sample_first: Vec<String>,
    pub sample_last: Vec<String>,
    pub total_redacted_lines: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub id: String,
}

/// Build the bundle in-memory and return preview metadata. No network. No disk writes.
/// The zip bytes are dropped after measuring so we don't ferry MB across IPC.
///
/// Scope: last 24 hours of log content, capped at 10 MB compressed (see
/// [`error_reporter::cap_bundle_to_mb`]).
#[tauri::command]
pub async fn prepare_error_report_preview(
    app: tauri::AppHandle,
    user_note: Option<String>,
) -> Result<PreviewPayload, String> {
    let note = validate_user_note(user_note)?;
    let bundle = error_reporter::build_bundle(&app, BundleKind::User, note, BundleScope::Last24Hours)?;
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

/// Re-build the bundle and upload it. Returns the server-issued ID — display *that* to
/// the user, not any locally-generated ID from a prior `prepare` call.
#[tauri::command]
pub async fn send_error_report(app: tauri::AppHandle, user_note: Option<String>) -> Result<SendResult, String> {
    let note = validate_user_note(user_note)?;
    let bundle = error_reporter::build_bundle(&app, BundleKind::User, note, BundleScope::Last24Hours)?;
    let capped = error_reporter::cap_bundle_to_mb(bundle.zip_bytes, FLOW_A_BUNDLE_CAP_MB);
    let result = error_reporter::upload(capped, &bundle.manifest, ERROR_REPORT_URL).await?;
    Ok(SendResult { id: result.id })
}

/// Debug-only escape hatch: build the bundle and write it to the app data dir as a `.zip`.
/// Helpful when iterating on the redactor or the manifest format.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn save_error_report_to_disk(app: tauri::AppHandle, user_note: Option<String>) -> Result<String, String> {
    let note = validate_user_note(user_note)?;
    let mut bundle = error_reporter::build_bundle(&app, BundleKind::User, note, BundleScope::Last24Hours)?;
    bundle.zip_bytes = error_reporter::cap_bundle_to_mb(bundle.zip_bytes, FLOW_A_BUNDLE_CAP_MB);
    let path = error_reporter::save_bundle_to_disk(&app, &bundle)?;
    Ok(path.display().to_string())
}

fn validate_user_note(user_note: Option<String>) -> Result<Option<String>, String> {
    match user_note {
        Some(n) if n.chars().count() > MAX_USER_NOTE_CHARS => Err(format!(
            "User note is too long ({} chars). Maximum is {MAX_USER_NOTE_CHARS} chars.",
            n.chars().count(),
        )),
        other => Ok(other),
    }
}
