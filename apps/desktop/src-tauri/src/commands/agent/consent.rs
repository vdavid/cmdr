//! The opt-in gate (plan §12).
//!
//! The copy version + the gate predicate live in `crate::agent::consent` so both the send
//! path (structural enforcement) and these status/accept commands share one source.

use serde::Serialize;
use tauri::AppHandle;

use super::{now_secs, with_read_connection, with_write_connection};
use crate::agent::consent::CONSENT_COPY_VERSION;
use crate::agent::store;

/// Whether the user has opted into Ask Cmdr, and the audit of what they accepted. The rail
/// gates on `accepted` (the CURRENT copy version): a never-accepted or stale-version record
/// re-shows the consent screen, and nothing is ever sent to a provider without it.
#[derive(Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AskCmdrConsentStatus {
    /// True only when the user accepted the CURRENT `current_version`. The one flag the
    /// rail and the settings toggle read.
    pub accepted: bool,
    /// The copy version the user must have accepted to be `accepted`.
    pub current_version: u32,
    /// The version the user last accepted, or `None` if never.
    pub accepted_version: Option<u32>,
    /// When the user last accepted (unix secs), or `None` if never.
    pub accepted_at: Option<i64>,
}

/// The Ask Cmdr consent status: whether the user opted into the CURRENT consent copy, plus
/// the audit of what/when they accepted. Reads `main.db`; a missing store reads as
/// not-accepted, so the gate stays closed rather than failing open.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_consent_status(app: AppHandle) -> Result<AskCmdrConsentStatus, String> {
    let not_accepted = AskCmdrConsentStatus {
        accepted: false,
        current_version: CONSENT_COPY_VERSION,
        accepted_version: None,
        accepted_at: None,
    };
    with_read_connection(app, not_accepted, move |conn| {
        let stored = store::get_consent(conn)?;
        Ok(AskCmdrConsentStatus {
            accepted: stored.map(|c| c.version) == Some(CONSENT_COPY_VERSION),
            current_version: CONSENT_COPY_VERSION,
            accepted_version: stored.map(|c| c.version),
            accepted_at: stored.map(|c| c.at),
        })
    })
    .await
}

/// Record the user's opt-in to the current consent copy (timestamp + copy version), so the
/// rail unlocks. Idempotent.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_accept_consent(app: AppHandle) -> Result<(), String> {
    let now = now_secs();
    let handle = app.clone();
    let recorded = with_write_connection(app, move |conn| store::set_consent(conn, CONSENT_COPY_VERSION, now)).await;
    // Consent is the wake loop's first gate, and it reads a cached answer rather than the
    // store. Without this the pipeline would keep refusing every rollup until the next launch.
    crate::agent::wake::refresh_readiness(&handle);
    recorded
}

/// Turn Ask Cmdr off by clearing consent (the settings "turn off" path). The next rail
/// open re-shows the consent screen. No delete of chats — history stays.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_revoke_consent(app: AppHandle) -> Result<(), String> {
    let handle = app.clone();
    let cleared = with_write_connection(app, store::clear_consent).await;
    crate::agent::wake::refresh_readiness(&handle);
    cleared
}
