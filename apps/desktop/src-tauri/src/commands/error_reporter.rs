//! Error reporter Tauri commands.
//!
//! Two preview/send commands plus a debug-only save-to-disk command. Business logic lives
//! in `crate::error_reporter` — these wrappers just shape inputs/outputs for the IPC layer.

use crate::error_reporter::{self, BundleKind, BundleManifest, BundleScope, FLOW_A_BUNDLE_CAP_MB};
use serde::Serialize;
use std::collections::HashMap;

/// Server URL for error report ingestion. Mirrors the crash reporter pattern.
#[cfg(debug_assertions)]
const ERROR_REPORT_URL: &str = "http://localhost:8787/error-report";

#[cfg(not(debug_assertions))]
const ERROR_REPORT_URL: &str = "https://api.getcmdr.com/error-report";

/// Server still enforces a 10 MB total payload cap; this is the cheaper client-side
/// guardrail so we don't waste effort building a bundle that'd be rejected.
const MAX_USER_NOTE_CHARS: usize = 100_000;

/// Cap on a single command-id length — guards against the FE accidentally pushing a
/// pasted blob in here. Real command IDs in `command-registry.ts` are well under 64
/// chars; 256 is generous.
const MAX_COMMAND_ID_CHARS: usize = 256;

/// Pushes the FE settings-registry default map to the backend, where it feeds
/// [`crate::error_reporter::ResolvedSettings::from_settings`] so manifests don't
/// duplicate defaults between TypeScript and Rust.
///
/// Called once from `initializeSettings()` in `apps/desktop/src/lib/settings/settings-store.ts`
/// after the registry has loaded. Subsequent calls overwrite (HMR-safe in dev).
/// Failures are silent — the Rust side falls back to hardcoded defaults if the map
/// is missing or doesn't include a given key.
#[tauri::command]
pub fn record_settings_defaults(defaults: HashMap<String, serde_json::Value>) {
    error_reporter::settings_defaults::record(defaults);
}

/// Records the most recent FE user-driven command for the error-report manifest.
///
/// Called from `handleCommandExecute` in `apps/desktop/src/routes/(main)/command-dispatch.ts`,
/// which is the single chokepoint for all keyboard / palette / menu commands. Cheap:
/// one `Mutex` write, no I/O. Drops silently if the input is malformed (we'd rather
/// keep the previous value than poison the manifest with garbage).
#[tauri::command]
pub fn record_user_action(command_id: String) {
    if command_id.is_empty() || command_id.chars().count() > MAX_COMMAND_ID_CHARS {
        return;
    }
    error_reporter::user_action::record(command_id.clone());
    // Same event also lands in the breadcrumb stream so triagers see it in context
    // alongside other FE events. Eventually `record_user_action` should be removed
    // entirely in favor of breadcrumbs (last_user_action becomes a derived view).
    error_reporter::breadcrumbs::record("command", &command_id, None);
}

/// Records a freeform breadcrumb event for the error-report manifest.
///
/// Called from FE event handlers (navigation, dialog open/close, etc.) to add
/// triage context. Validation matches `record_user_action`: empty inputs and
/// over-long fields are dropped silently. `ctx` is an optional structured payload.
#[tauri::command]
pub fn record_breadcrumb(kind: String, message: String, ctx: Option<serde_json::Value>) {
    error_reporter::breadcrumbs::record(&kind, &message, ctx);
}

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
/// Scope: last 24 hours of log content, capped at 1 MB compressed (see
/// [`error_reporter::cap_bundle_to_mb`] and [`FLOW_A_BUNDLE_CAP_MB`]).
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
    log::info!(
        target: "cmdr_lib::error_reporter",
        "Saved error report bundle to disk: id={} path={}",
        bundle.manifest.id,
        path.display(),
    );
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
