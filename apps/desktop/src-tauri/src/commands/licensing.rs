//! Tauri commands for licensing.

use crate::licensing;

/// Get the current app status (personal, commercial, or expired).
#[tauri::command]
pub fn get_license_status(app: tauri::AppHandle) -> licensing::AppStatus {
    licensing::get_app_status(&app)
}

/// Get the window title based on current license status.
#[tauri::command]
pub fn get_window_title(app: tauri::AppHandle) -> String {
    let status = licensing::get_app_status(&app);
    licensing::get_window_title(&status)
}

/// Activate a license key or short code (verify + commit in one call).
/// If the input is a short code (CMDR-XXXX-XXXX-XXXX), it first exchanges it for the full key.
/// Kept for backward compatibility — new code should use verify_license + commit_license.
#[tauri::command]
pub async fn activate_license(
    app: tauri::AppHandle,
    license_key: String,
) -> Result<licensing::LicenseInfo, licensing::LicenseActivationError> {
    licensing::activate_license_async(&app, &license_key).await
}

/// Verify a license key or short code without writing anything to disk.
/// Returns the verify result (LicenseInfo + full key) for the frontend to inspect
/// before deciding whether to commit.
#[tauri::command]
pub async fn verify_license(license_key: String) -> Result<licensing::VerifyResult, licensing::LicenseActivationError> {
    licensing::verify_license_async(&license_key).await
}

/// Persist a verified license key to disk and update caches.
/// Only call after verification confirms the key is valid.
#[tauri::command]
pub fn commit_license(
    app: tauri::AppHandle,
    license_key: String,
    short_code: Option<String>,
) -> Result<licensing::LicenseInfo, licensing::LicenseActivationError> {
    licensing::commit_license(&app, &license_key, short_code.as_deref())
}

/// Get information about the current license (if any).
#[tauri::command]
pub fn get_license_info(app: tauri::AppHandle) -> Option<licensing::LicenseInfo> {
    licensing::get_license_info(&app)
}

/// Mark the expiration modal as shown (so it won't show again).
#[tauri::command]
pub fn mark_expiration_modal_shown(app: tauri::AppHandle) {
    licensing::mark_expiration_modal_shown(&app);
}

/// Mark the commercial reminder as dismissed (resets the 30-day timer).
#[tauri::command]
pub fn mark_commercial_reminder_dismissed(app: tauri::AppHandle) {
    licensing::mark_commercial_reminder_dismissed(&app);
}

/// Reset license data (debug builds only).
#[tauri::command]
pub fn reset_license(app: tauri::AppHandle) {
    licensing::reset_license(&app);
}

/// Check if license needs re-validation with the server.
#[tauri::command]
pub fn needs_license_validation(app: tauri::AppHandle) -> bool {
    licensing::needs_validation(&app)
}

/// Check if a server validation has ever completed for the current license.
#[tauri::command]
pub fn has_license_been_validated(app: tauri::AppHandle) -> bool {
    licensing::has_been_validated(&app)
}

/// Validate license with server (async - call when needs_license_validation returns true).
/// If `transaction_id` is provided, uses it directly (for pre-commit validation).
/// If `None`, reads from the stored license (for periodic re-validation).
/// Returns Err on network/upstream errors so the frontend can distinguish from server rejection.
#[tauri::command]
pub async fn validate_license_with_server(
    app: tauri::AppHandle,
    transaction_id: Option<String>,
) -> Result<licensing::AppStatus, String> {
    licensing::validate_license_async(&app, transaction_id.as_deref()).await
}
