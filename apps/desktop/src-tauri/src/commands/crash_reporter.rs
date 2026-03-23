//! Crash reporter Tauri commands.
//!
//! Thin wrappers for crash file detection, dismissal, and sending.

use crate::config;
use crate::crash_reporter;

/// Server URL for crash report ingestion.
#[cfg(debug_assertions)]
const CRASH_REPORT_URL: &str = "http://localhost:8787/crash-report";

#[cfg(not(debug_assertions))]
const CRASH_REPORT_URL: &str = "https://license.getcmdr.com/crash-report";

/// Checks for a pending crash report from a previous session.
/// Returns the report as a JSON value, or `null` if none exists.
#[tauri::command]
pub fn check_pending_crash_report(app: tauri::AppHandle) -> Option<serde_json::Value> {
    let report = crash_reporter::take_pending_crash_report(&app)?;
    serde_json::to_value(report).ok()
}

/// Deletes the crash report file without sending it.
#[tauri::command]
pub fn dismiss_crash_report(app: tauri::AppHandle) {
    let Ok(data_dir) = config::resolved_app_data_dir(&app) else {
        return;
    };
    let crash_path = data_dir.join("crash-report.json");
    let _ = std::fs::remove_file(crash_path);
}

/// Sends the crash report to the ingestion server, then deletes the local file.
/// Skipped in dev mode and CI to avoid polluting production data.
#[tauri::command]
pub async fn send_crash_report(app: tauri::AppHandle, report: serde_json::Value) -> Result<(), String> {
    let should_skip = cfg!(debug_assertions) || std::env::var("CI").is_ok();

    if !should_skip {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("Couldn't create HTTP client: {e}"))?;

        let response = client
            .post(CRASH_REPORT_URL)
            .json(&report)
            .send()
            .await
            .map_err(|e| format!("Couldn't send crash report: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("Crash report server returned {}", response.status()));
        }
    } else {
        log::info!("Crash reporter: skipping send (dev mode or CI)");
    }

    // Delete the local crash file after successful send (or skip)
    if let Ok(data_dir) = config::resolved_app_data_dir(&app) {
        let crash_path = data_dir.join("crash-report.json");
        let _ = std::fs::remove_file(crash_path);
    }

    Ok(())
}
