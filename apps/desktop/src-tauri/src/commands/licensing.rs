//! Tauri commands for licensing.

use crate::licensing;

/// Get the current app status (personal, supporter, commercial, or expired).
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

/// Activate a license key or short code.
/// If the input is a short code (CMDR-XXXX-XXXX-XXXX), it first exchanges it for the full key.
#[tauri::command]
pub async fn activate_license(app: tauri::AppHandle, license_key: String) -> Result<licensing::LicenseInfo, String> {
    licensing::activate_license_async(&app, &license_key).await
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

/// Validate license with server (async - call when needs_license_validation returns true).
#[tauri::command]
pub async fn validate_license_with_server(app: tauri::AppHandle) -> licensing::AppStatus {
    licensing::validate_license_async(&app).await
}
