//! Application license status and validation.
//!
//! This module handles:
//! - License status checking (personal, supporter, commercial)
//! - Server-side validation for subscription status
//! - Caching for offline use (30-day grace period)
//! - Mock mode for local testing

use crate::licensing::verification::{LicenseInfo, get_license_info};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri_plugin_store::StoreExt;

/// How often to re-validate license (7 days in seconds).
const VALIDATION_INTERVAL_SECS: u64 = 7 * 24 * 60 * 60;

/// Grace period for offline use (30 days in seconds).
const OFFLINE_GRACE_PERIOD_SECS: u64 = 30 * 24 * 60 * 60;

/// How often to show commercial license reminder to Personal users (30 days in seconds).
const COMMERCIAL_REMINDER_INTERVAL_SECS: u64 = 30 * 24 * 60 * 60;

/// Store keys for cached validation data.
const STORE_KEY_CACHED_STATUS: &str = "cached_license_status";
const STORE_KEY_LAST_VALIDATION: &str = "last_validation_timestamp";
const STORE_KEY_EXPIRATION_SHOWN: &str = "expiration_modal_shown";
const STORE_KEY_REMINDER_LAST_DISMISSED: &str = "commercial_reminder_last_dismissed";

/// Type of license.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseType {
    Supporter,
    CommercialSubscription,
    CommercialPerpetual,
}

/// Current status of the application license.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppStatus {
    /// No license - personal use only.
    #[serde(rename_all = "camelCase")]
    Personal {
        /// Whether to show the commercial license reminder modal.
        show_commercial_reminder: bool,
    },
    /// Supporter license (personal use with badge).
    #[serde(rename_all = "camelCase")]
    Supporter {
        /// Whether to show the commercial license reminder modal.
        show_commercial_reminder: bool,
    },
    /// Active commercial license.
    #[serde(rename_all = "camelCase")]
    Commercial {
        license_type: LicenseType,
        organization_name: Option<String>,
        expires_at: Option<String>,
    },
    /// Expired commercial license - reverted to personal.
    #[serde(rename_all = "camelCase")]
    Expired {
        organization_name: Option<String>,
        expired_at: String,
        show_modal: bool,
    },
}

/// Cached license status from server validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedLicenseStatus {
    pub status: String, // "active", "expired", "invalid"
    pub license_type: Option<LicenseType>,
    pub organization_name: Option<String>,
    pub expires_at: Option<String>,
    pub cached_at: u64,
}

/// Get the current application status.
///
/// Priority:
/// 1. Check for mock mode (debug builds only)
/// 2. Check for stored license key
/// 3. Validate with server if needed (every 7 days)
/// 4. Fall back to cached status or personal use
pub fn get_app_status(app: &tauri::AppHandle) -> AppStatus {
    // In debug builds, check for mock mode first
    #[cfg(debug_assertions)]
    if let Some(status) = get_mock_status(app) {
        return status;
    }

    // Check for a valid license key
    let license_info = match get_license_info(app) {
        Some(info) => info,
        None => {
            return AppStatus::Personal {
                show_commercial_reminder: should_show_commercial_reminder(app),
            };
        }
    };

    // Get cached status
    get_cached_or_validate(app, &license_info)
}

/// Check if license needs re-validation (called by frontend to trigger async validation).
pub fn needs_validation(app: &tauri::AppHandle) -> bool {
    // In mock mode, skip server validation entirely
    #[cfg(debug_assertions)]
    if std::env::var("CMDR_MOCK_LICENSE").is_ok() {
        return false;
    }

    let store = match app.store("license.json") {
        Ok(s) => s,
        Err(_) => return true,
    };

    let last_validation: Option<u64> = store.get(STORE_KEY_LAST_VALIDATION).and_then(|v| v.as_u64());
    let now = current_timestamp();

    match last_validation {
        Some(ts) => now.saturating_sub(ts) > VALIDATION_INTERVAL_SECS,
        None => true,
    }
}

/// Validate license with server asynchronously.
/// Returns the updated AppStatus after validation.
pub async fn validate_license_async(app: &tauri::AppHandle) -> AppStatus {
    // In debug builds, check for mock mode first
    #[cfg(debug_assertions)]
    if let Some(status) = get_mock_status(app) {
        return status;
    }

    // Check for a valid license key
    let license_info = match get_license_info(app) {
        Some(info) => info,
        None => {
            return AppStatus::Personal {
                show_commercial_reminder: should_show_commercial_reminder(app),
            };
        }
    };

    // Call the license server
    let response = crate::licensing::validation_client::validate_with_server(&license_info.transaction_id).await;

    match response {
        Some(resp) => {
            // Convert response to LicenseType
            let license_type = resp.license_type.as_deref().and_then(string_to_license_type);

            // Update cache
            update_cached_status(
                app,
                &resp.status,
                license_type,
                resp.organization_name.clone(),
                resp.expires_at.clone(),
            );

            // Return the new status based on the response
            response_to_app_status(app, &resp)
        }
        None => {
            // Network error - fall back to cached status
            log::warn!("License validation failed, using cached status");
            get_app_status(app)
        }
    }
}

/// Convert validation response to AppStatus.
fn response_to_app_status(
    app: &tauri::AppHandle,
    resp: &crate::licensing::validation_client::ValidationResponse,
) -> AppStatus {
    let license_type = resp.license_type.as_deref().and_then(string_to_license_type);
    to_app_status(app, &resp.status, license_type, resp.organization_name.clone(), resp.expires_at.clone())
}

/// Convert string to LicenseType.
fn string_to_license_type(s: &str) -> Option<LicenseType> {
    match s {
        "supporter" => Some(LicenseType::Supporter),
        "commercial_subscription" => Some(LicenseType::CommercialSubscription),
        "commercial_perpetual" => Some(LicenseType::CommercialPerpetual),
        _ => None,
    }
}

/// Shared logic for converting a status string + license metadata into AppStatus.
fn to_app_status(
    app: &tauri::AppHandle,
    status: &str,
    license_type: Option<LicenseType>,
    organization_name: Option<String>,
    expires_at: Option<String>,
) -> AppStatus {
    match status {
        "active" => match license_type {
            Some(LicenseType::Supporter) => AppStatus::Supporter {
                show_commercial_reminder: should_show_commercial_reminder(app),
            },
            Some(lt) => AppStatus::Commercial {
                license_type: lt,
                organization_name,
                expires_at,
            },
            None => AppStatus::Personal {
                show_commercial_reminder: should_show_commercial_reminder(app),
            },
        },
        "expired" => {
            let show_modal = !expiration_modal_shown(app);
            AppStatus::Expired {
                organization_name,
                expired_at: expires_at.unwrap_or_else(|| "unknown".to_string()),
                show_modal,
            }
        }
        _ => AppStatus::Personal {
            show_commercial_reminder: should_show_commercial_reminder(app),
        },
    }
}

/// Get cached status or fallback to personal use.
fn get_cached_or_validate(app: &tauri::AppHandle, license_info: &LicenseInfo) -> AppStatus {
    let store = match app.store("license.json") {
        Ok(s) => s,
        Err(_) => {
            return AppStatus::Personal {
                show_commercial_reminder: should_show_commercial_reminder(app),
            };
        }
    };

    // Check if we have cached status
    let cached: Option<CachedLicenseStatus> = store
        .get(STORE_KEY_CACHED_STATUS)
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let now = current_timestamp();

    // Use cached status if available and within grace period
    if let Some(cached) = cached {
        let cache_age = now.saturating_sub(cached.cached_at);

        if cache_age <= OFFLINE_GRACE_PERIOD_SECS {
            return cached_to_app_status(app, &cached);
        }
    }

    // No valid cache - for first-time validation, create initial cache from license key
    // The license key contains the email/transaction info but not subscription status
    // We'll return Personal until async validation completes
    log::info!(
        "License key found for {} but no cached status, returning Personal until validation",
        license_info.email
    );
    AppStatus::Personal {
        show_commercial_reminder: should_show_commercial_reminder(app),
    }
}

/// Convert cached status to AppStatus.
fn cached_to_app_status(app: &tauri::AppHandle, cached: &CachedLicenseStatus) -> AppStatus {
    to_app_status(app, &cached.status, cached.license_type, cached.organization_name.clone(), cached.expires_at.clone())
}

/// Check if expiration modal has been shown for current expiration.
fn expiration_modal_shown(app: &tauri::AppHandle) -> bool {
    let store = match app.store("license.json") {
        Ok(s) => s,
        Err(_) => return false,
    };
    store
        .get(STORE_KEY_EXPIRATION_SHOWN)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Mark expiration modal as shown.
pub fn mark_expiration_modal_shown(app: &tauri::AppHandle) {
    if let Ok(store) = app.store("license.json") {
        store.set(STORE_KEY_EXPIRATION_SHOWN, serde_json::json!(true));
    }
}

/// Check if we should show the commercial license reminder.
/// Returns true if 30+ days have passed since the last dismissal or first launch.
fn should_show_commercial_reminder(app: &tauri::AppHandle) -> bool {
    let store = match app.store("license.json") {
        Ok(s) => s,
        Err(_) => return false, // Can't check, don't show
    };

    let last_dismissed: Option<u64> = store.get(STORE_KEY_REMINDER_LAST_DISMISSED).and_then(|v| v.as_u64());

    match last_dismissed {
        Some(ts) => {
            let now = current_timestamp();
            now.saturating_sub(ts) >= COMMERCIAL_REMINDER_INTERVAL_SECS
        }
        None => {
            // First time seeing this user - initialize the timer to now
            // so they won't see the reminder until 30 days from now
            store.set(
                STORE_KEY_REMINDER_LAST_DISMISSED,
                serde_json::json!(current_timestamp()),
            );
            false
        }
    }
}

/// Mark commercial reminder as dismissed (resets the 30-day timer).
pub fn mark_commercial_reminder_dismissed(app: &tauri::AppHandle) {
    if let Ok(store) = app.store("license.json") {
        store.set(
            STORE_KEY_REMINDER_LAST_DISMISSED,
            serde_json::json!(current_timestamp()),
        );
    }
}

/// Update cached license status from server response.
pub fn update_cached_status(
    app: &tauri::AppHandle,
    status: &str,
    license_type: Option<LicenseType>,
    organization_name: Option<String>,
    expires_at: Option<String>,
) {
    if let Ok(store) = app.store("license.json") {
        let cached = CachedLicenseStatus {
            status: status.to_string(),
            license_type,
            organization_name,
            expires_at,
            cached_at: current_timestamp(),
        };
        store.set(STORE_KEY_CACHED_STATUS, serde_json::json!(cached));
        store.set(STORE_KEY_LAST_VALIDATION, serde_json::json!(current_timestamp()));

        // Reset expiration shown flag if status changes from expired
        if status != "expired" {
            store.delete(STORE_KEY_EXPIRATION_SHOWN);
        }
    }
}

/// Get the window title based on license status.
pub fn get_window_title(status: &AppStatus) -> String {
    match status {
        AppStatus::Personal { .. } => "Cmdr – Personal use only".to_string(),
        AppStatus::Supporter { .. } => "Cmdr – Personal".to_string(),
        AppStatus::Commercial { .. } => "Cmdr".to_string(),
        AppStatus::Expired { .. } => "Cmdr – Personal use only".to_string(),
    }
}

/// Reset license data (for testing only).
#[cfg(debug_assertions)]
pub fn reset_license(app: &tauri::AppHandle) {
    if let Ok(store) = app.store("license.json") {
        store.delete("license_key");
        store.delete(STORE_KEY_CACHED_STATUS);
        store.delete(STORE_KEY_LAST_VALIDATION);
        store.delete(STORE_KEY_EXPIRATION_SHOWN);
        store.delete(STORE_KEY_REMINDER_LAST_DISMISSED);
    }
}

#[cfg(not(debug_assertions))]
pub fn reset_license(_app: &tauri::AppHandle) {
    // No-op in release builds
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

// ============================================================================
// Mock mode for local testing (debug builds only)
// ============================================================================

/// Get mock status from environment variable.
///
/// Set CMDR_MOCK_LICENSE to one of:
/// - "personal" - No license (no reminder)
/// - "personal_reminder" - No license (shows commercial reminder modal)
/// - "supporter" - Supporter badge (no reminder)
/// - "supporter_reminder" - Supporter badge (shows commercial reminder modal)
/// - "commercial" - Active commercial subscription
/// - "perpetual" - Active perpetual license
/// - "expired" - Expired subscription (shows modal)
/// - "expired_no_modal" - Expired subscription (modal already shown)
#[cfg(debug_assertions)]
fn get_mock_status(_app: &tauri::AppHandle) -> Option<AppStatus> {
    let mock_value = std::env::var("CMDR_MOCK_LICENSE").ok()?;

    match mock_value.to_lowercase().as_str() {
        "personal" => Some(AppStatus::Personal {
            show_commercial_reminder: false,
        }),
        "personal_reminder" => Some(AppStatus::Personal {
            show_commercial_reminder: true,
        }),
        "supporter" => Some(AppStatus::Supporter {
            show_commercial_reminder: false,
        }),
        "supporter_reminder" => Some(AppStatus::Supporter {
            show_commercial_reminder: true,
        }),
        "commercial" => Some(AppStatus::Commercial {
            license_type: LicenseType::CommercialSubscription,
            organization_name: Some("Test Corporation".to_string()),
            expires_at: Some("2027-01-10T00:00:00Z".to_string()),
        }),
        "perpetual" => Some(AppStatus::Commercial {
            license_type: LicenseType::CommercialPerpetual,
            organization_name: Some("Perpetual Inc.".to_string()),
            expires_at: None,
        }),
        "expired" => Some(AppStatus::Expired {
            organization_name: Some("Expired Corp".to_string()),
            expired_at: "2026-01-01T00:00:00Z".to_string(),
            show_modal: true,
        }),
        "expired_no_modal" => Some(AppStatus::Expired {
            organization_name: Some("Expired Corp".to_string()),
            expired_at: "2026-01-01T00:00:00Z".to_string(),
            show_modal: false,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_window_title_personal() {
        let status = AppStatus::Personal {
            show_commercial_reminder: false,
        };
        assert_eq!(get_window_title(&status), "Cmdr – Personal use only");
    }

    #[test]
    fn test_get_window_title_personal_with_reminder() {
        let status = AppStatus::Personal {
            show_commercial_reminder: true,
        };
        assert_eq!(get_window_title(&status), "Cmdr – Personal use only");
    }

    #[test]
    fn test_get_window_title_supporter() {
        let status = AppStatus::Supporter {
            show_commercial_reminder: false,
        };
        assert_eq!(get_window_title(&status), "Cmdr – Personal");
    }

    #[test]
    fn test_get_window_title_commercial() {
        let status = AppStatus::Commercial {
            license_type: LicenseType::CommercialSubscription,
            organization_name: Some("Test Corp".to_string()),
            expires_at: Some("2027-01-01".to_string()),
        };
        assert_eq!(get_window_title(&status), "Cmdr");
    }

    #[test]
    fn test_get_window_title_commercial_perpetual() {
        let status = AppStatus::Commercial {
            license_type: LicenseType::CommercialPerpetual,
            organization_name: None,
            expires_at: None,
        };
        assert_eq!(get_window_title(&status), "Cmdr");
    }

    #[test]
    fn test_get_window_title_expired() {
        let status = AppStatus::Expired {
            organization_name: Some("Old Corp".to_string()),
            expired_at: "2026-01-01".to_string(),
            show_modal: true,
        };
        assert_eq!(get_window_title(&status), "Cmdr – Personal use only");
    }

    #[test]
    fn test_license_type_serialization() {
        assert_eq!(serde_json::to_string(&LicenseType::Supporter).unwrap(), "\"supporter\"");
        assert_eq!(
            serde_json::to_string(&LicenseType::CommercialSubscription).unwrap(),
            "\"commercial_subscription\""
        );
        assert_eq!(
            serde_json::to_string(&LicenseType::CommercialPerpetual).unwrap(),
            "\"commercial_perpetual\""
        );
    }

    #[test]
    fn test_app_status_personal_serialization() {
        let status = AppStatus::Personal {
            show_commercial_reminder: true,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"personal\""));
        assert!(json.contains("\"showCommercialReminder\":true"));
    }

    #[test]
    fn test_app_status_supporter_serialization() {
        let status = AppStatus::Supporter {
            show_commercial_reminder: false,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"supporter\""));
        assert!(json.contains("\"showCommercialReminder\":false"));
    }

    #[test]
    fn test_app_status_commercial_serialization() {
        let status = AppStatus::Commercial {
            license_type: LicenseType::CommercialSubscription,
            organization_name: Some("Acme".to_string()),
            expires_at: Some("2027-01-01".to_string()),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"commercial\""));
        // Fields are camelCase due to serde rename_all
        assert!(json.contains("Acme"));
        assert!(json.contains("commercial_subscription")); // LicenseType is snake_case
    }

    #[test]
    fn test_app_status_expired_serialization() {
        let status = AppStatus::Expired {
            organization_name: None,
            expired_at: "2026-01-01".to_string(),
            show_modal: true,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"expired\""));
        // Fields are camelCase due to serde rename_all on AppStatus
        assert!(json.contains("2026-01-01"));
        assert!(json.contains("true")); // showModal value
    }

    #[test]
    fn test_cached_license_status_round_trip() {
        let cached = CachedLicenseStatus {
            status: "active".to_string(),
            license_type: Some(LicenseType::CommercialSubscription),
            organization_name: Some("Test Inc".to_string()),
            expires_at: Some("2027-06-15".to_string()),
            cached_at: 1704067200,
        };

        let json = serde_json::to_string(&cached).unwrap();
        let deserialized: CachedLicenseStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.status, "active");
        assert_eq!(deserialized.license_type, Some(LicenseType::CommercialSubscription));
        assert_eq!(deserialized.organization_name, Some("Test Inc".to_string()));
        assert_eq!(deserialized.expires_at, Some("2027-06-15".to_string()));
        assert_eq!(deserialized.cached_at, 1704067200);
    }

    #[test]
    fn test_commercial_reminder_interval_is_30_days() {
        // Verify the constant is 30 days in seconds
        assert_eq!(COMMERCIAL_REMINDER_INTERVAL_SECS, 30 * 24 * 60 * 60);
    }

    #[test]
    fn test_expired_status_structure() {
        // Test that expired status has correct structure
        let status = AppStatus::Expired {
            organization_name: Some("Expired Corp".to_string()),
            expired_at: "2026-01-01T00:00:00Z".to_string(),
            show_modal: true,
        };
        assert!(matches!(status, AppStatus::Expired { show_modal: true, .. }));

        let status_no_modal = AppStatus::Expired {
            organization_name: Some("Expired Corp".to_string()),
            expired_at: "2026-01-01T00:00:00Z".to_string(),
            show_modal: false,
        };
        assert!(matches!(status_no_modal, AppStatus::Expired { show_modal: false, .. }));
    }

    #[test]
    fn test_commercial_status_structure() {
        // Test commercial subscription
        let subscription = AppStatus::Commercial {
            license_type: LicenseType::CommercialSubscription,
            organization_name: Some("Test Corporation".to_string()),
            expires_at: Some("2027-01-10T00:00:00Z".to_string()),
        };
        if let AppStatus::Commercial { organization_name, .. } = subscription {
            assert_eq!(organization_name, Some("Test Corporation".to_string()));
        }

        // Test commercial perpetual
        let perpetual = AppStatus::Commercial {
            license_type: LicenseType::CommercialPerpetual,
            organization_name: Some("Perpetual Inc.".to_string()),
            expires_at: None,
        };
        if let AppStatus::Commercial { expires_at, .. } = perpetual {
            assert_eq!(expires_at, None);
        }
    }
}
