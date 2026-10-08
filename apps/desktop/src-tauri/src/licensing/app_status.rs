//! Application license status and validation.
//!
//! This module handles:
//! - License status checking (personal, commercial)
//! - Periodic server checks, which only ever learn a signed verdict (revoked, expired, renewed)
//! - Caching that verdict, and the clock guard for time-limited licenses
//! - Mock mode for local testing
//!
//! What a license is worth offline is decided in `offline_policy.rs`.

use crate::licensing::offline_policy::{self, KeyTerms, LicenseState};
use crate::licensing::verification::{LicenseInfo, SignedValidationAnswer, get_license_info};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Manager;
use tauri_plugin_store::StoreExt;

/// How often to re-validate license (7 days in seconds).
const VALIDATION_INTERVAL_SECS: u64 = 7 * 24 * 60 * 60;

/// Don't re-attempt a failed periodic revalidation more often than this (seconds).
const FAILED_VALIDATION_RETRY_COOLDOWN_SECS: u64 = 60;

/// Serializes server validations so a burst of concurrent triggers coalesces into one request
/// instead of a stampede of identical ones.
static VALIDATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Unix timestamp of the last failed validation attempt (0 = never failed).
static LAST_FAILED_VALIDATION_AT: AtomicU64 = AtomicU64::new(0);

/// The clock guard only writes when the clock has moved on by this much, so a status check (which
/// runs often) doesn't write the store every time.
const CLOCK_HIGH_WATER_STEP_SECS: u64 = 60 * 60;

/// How often to show commercial license reminder to Personal users (30 days in seconds).
const COMMERCIAL_REMINDER_INTERVAL_SECS: u64 = 30 * 24 * 60 * 60;

/// Store keys for cached validation data.
const STORE_KEY_CACHED_STATUS: &str = "cached_license_status";
const STORE_KEY_LAST_VALIDATION: &str = "last_validation_timestamp";
const STORE_KEY_EXPIRATION_SHOWN: &str = "expiration_modal_shown";
const STORE_KEY_REMINDER_LAST_DISMISSED: &str = "commercial_reminder_last_dismissed";
/// The latest time this install has seen (Unix seconds). Time-limited licenses are judged against
/// it, so setting the clock back doesn't stretch one. A verified server answer resets it.
const STORE_KEY_CLOCK_HIGH_WATER: &str = "license_clock_high_water";

/// Type of license.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LicenseType {
    CommercialSubscription,
    CommercialPerpetual,
}

/// Current status of the application license.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppStatus {
    /// No license - personal use only.
    #[serde(rename_all = "camelCase")]
    Personal {
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
/// 1. Check for mock mode (debug and E2E builds only)
/// 2. Check for stored license key
/// 3. Validate with server if needed (every 7 days)
/// 4. Fall back to cached status or personal use
pub fn get_app_status(app: &tauri::AppHandle) -> AppStatus {
    // In debug and E2E builds, check for mock mode first
    #[cfg(any(debug_assertions, feature = "playwright-e2e"))]
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
    #[cfg(any(debug_assertions, feature = "playwright-e2e"))]
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

/// Check if a server validation has ever completed successfully.
/// Returns false if no `last_validation_timestamp` exists (license was committed locally but never
/// server-verified).
pub fn has_been_validated(app: &tauri::AppHandle) -> bool {
    let store = match app.store("license.json") {
        Ok(s) => s,
        Err(_) => return false,
    };
    store.get(STORE_KEY_LAST_VALIDATION).and_then(|v| v.as_u64()).is_some()
}

/// Validate license with server asynchronously.
/// If `transaction_id` is provided, uses it directly (for pre-commit validation).
/// If `None`, reads from the stored license (for periodic 7-day re-validation).
/// Returns the updated AppStatus after validation.
pub async fn validate_license_async(app: &tauri::AppHandle, transaction_id: Option<&str>) -> Result<AppStatus, String> {
    use crate::licensing::validation_client::ValidationOutcome;

    // In debug and E2E builds, check for mock mode first
    #[cfg(any(debug_assertions, feature = "playwright-e2e"))]
    if let Some(status) = get_mock_status(app) {
        return Ok(status);
    }

    // Resolve the transaction ID: use explicit parameter or fall back to stored license
    let resolved_transaction_id = match transaction_id {
        Some(id) => id.to_string(),
        None => match get_license_info(app) {
            Some(info) => info.transaction_id,
            None => {
                return Ok(AppStatus::Personal {
                    show_commercial_reminder: should_show_commercial_reminder(app),
                });
            }
        },
    };

    // Single-flight: hold the lock across the server call so concurrent triggers (multiple
    // windows firing validation at startup) coalesce instead of stampeding the server.
    let _guard = VALIDATION_LOCK.lock().await;

    // Periodic revalidation (no explicit transaction ID) can short-circuit; explicit activation
    // always goes through.
    if transaction_id.is_none() {
        if !needs_validation(app) {
            // Another caller validated successfully while we waited for the lock.
            return Ok(get_app_status(app));
        }
        if failed_validation_recently(LAST_FAILED_VALIDATION_AT.load(Ordering::Relaxed), current_timestamp()) {
            log::debug!(
                "Skipping license revalidation: last attempt failed under {FAILED_VALIDATION_RETRY_COOLDOWN_SECS}s ago"
            );
            return Err("Couldn't reach the license server".to_string());
        }
    }

    // Call the license server
    let outcome = crate::licensing::validation_client::validate_with_server(&resolved_transaction_id).await;

    match outcome {
        ValidationOutcome::Success(answer) => {
            LAST_FAILED_VALIDATION_AT.store(0, Ordering::Relaxed);
            let license_type = answer.license_type.as_deref().and_then(string_to_license_type);

            // The server signed a definitive answer for this request: cache it, and take its clock
            // as the truth (this is also how a clock that ran ahead gets corrected).
            update_cached_status(
                app,
                &answer.status,
                license_type,
                answer.organization_name.clone(),
                answer.expires_at.clone(),
            );
            if let Some(server_now) = offline_policy::parse_timestamp(&answer.signed_at) {
                set_clock_high_water(app, server_now);
            }

            Ok(response_to_app_status(app, &answer))
        }
        ValidationOutcome::Unverified => {
            // validation_client logged why. An answer we can't verify changes nothing.
            LAST_FAILED_VALIDATION_AT.store(current_timestamp(), Ordering::Relaxed);
            Err("Couldn't verify the license server's answer".to_string())
        }
        ValidationOutcome::UpstreamError => {
            LAST_FAILED_VALIDATION_AT.store(current_timestamp(), Ordering::Relaxed);
            log::warn!("License server couldn't reach Paddle");
            Err("License server couldn't reach payment provider".to_string())
        }
        ValidationOutcome::NetworkError => {
            // validation_client already logged the error with detail; don't log it twice.
            LAST_FAILED_VALIDATION_AT.store(current_timestamp(), Ordering::Relaxed);
            Err("Couldn't reach the license server".to_string())
        }
    }
}

/// True when a validation attempt failed less than the cooldown ago.
fn failed_validation_recently(last_failed_at: u64, now: u64) -> bool {
    last_failed_at != 0 && now.saturating_sub(last_failed_at) < FAILED_VALIDATION_RETRY_COOLDOWN_SECS
}

/// Convert validation response to AppStatus.
fn response_to_app_status(app: &tauri::AppHandle, resp: &SignedValidationAnswer) -> AppStatus {
    let license_type = resp.license_type.as_deref().and_then(string_to_license_type);
    to_app_status(
        app,
        &resp.status,
        license_type,
        resp.organization_name.clone(),
        resp.expires_at.clone(),
    )
}

/// Convert string to LicenseType.
///
/// Legacy "supporter" keys are treated as personal (returns `None`).
pub fn string_to_license_type(s: &str) -> Option<LicenseType> {
    match s {
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

/// The status from the signed key and the last verified server answer, with no network involved.
/// `offline_policy::resolve_license_state` makes the call; this adds the store reads and UI flags.
fn get_cached_or_validate(app: &tauri::AppHandle, license_info: &LicenseInfo) -> AppStatus {
    let cached: Option<CachedLicenseStatus> = app
        .store("license.json")
        .ok()
        .and_then(|store| store.get(STORE_KEY_CACHED_STATUS))
        .and_then(|v| serde_json::from_value(v).ok());

    let key = KeyTerms {
        license_type: license_info.license_type.as_deref().and_then(string_to_license_type),
        expires_at: license_info.expires_at.clone(),
    };
    let organization_name = cached
        .as_ref()
        .and_then(|c| c.organization_name.clone())
        .or_else(|| license_info.organization_name.clone());

    match offline_policy::resolve_license_state(&key, cached.as_ref(), effective_now(app)) {
        LicenseState::Active {
            license_type,
            expires_at,
        } => AppStatus::Commercial {
            license_type,
            organization_name,
            expires_at,
        },
        LicenseState::Expired { expired_at } => AppStatus::Expired {
            organization_name,
            expired_at,
            show_modal: !expiration_modal_shown(app),
        },
        LicenseState::Unlicensed => AppStatus::Personal {
            show_commercial_reminder: should_show_commercial_reminder(app),
        },
    }
}

/// The clock-guarded "now": the later of the system clock and the latest time this install has
/// seen, so winding the clock back can't stretch a time-limited license. Perpetual licenses don't
/// read the clock at all.
fn effective_now(app: &tauri::AppHandle) -> u64 {
    let now = current_timestamp();
    let Ok(store) = app.store("license.json") else {
        return now;
    };
    let high_water = store
        .get(STORE_KEY_CLOCK_HIGH_WATER)
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if now >= high_water.saturating_add(CLOCK_HIGH_WATER_STEP_SECS) {
        store.set(STORE_KEY_CLOCK_HIGH_WATER, serde_json::json!(now));
    }
    now.max(high_water)
}

/// Replace the clock guard with the server's signed time.
fn set_clock_high_water(app: &tauri::AppHandle, timestamp: u64) {
    if let Ok(store) = app.store("license.json") {
        store.set(STORE_KEY_CLOCK_HIGH_WATER, serde_json::json!(timestamp));
    }
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

/// Write cached license status without marking it as server-validated.
/// Used by `commit_license` so the app shows the correct license type immediately,
/// while `needs_validation()` still returns true (triggering server verification on next launch).
pub fn write_cached_status_without_validation(
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

        if status != "expired" {
            store.delete(STORE_KEY_EXPIRATION_SHOWN);
        }
    }

    refresh_window_title(app);
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

    refresh_window_title(app);
}

/// The main window's OS-level title, from the licence status. This is the one
/// Mission Control and the Window menu read; the in-app title bar draws its own
/// from the same catalog key (`routes/(main)/+page.svelte`), so a language
/// switch moves it live without a round-trip here.
///
/// Reads the catalog through [`crate::intl::menu_t`] rather than the webview's
/// `t()`: `lib.rs` sets this title during `setup`, before any frontend code
/// runs. A commercial licence gets the bare product name, which is a brand and
/// so carries no catalog key.
pub fn get_window_title(status: &AppStatus) -> String {
    match status {
        AppStatus::Personal { .. } | AppStatus::Expired { .. } => {
            crate::intl::menu_t("licensing.windowTitle.personalUse")
        }
        AppStatus::Commercial { .. } => "Cmdr".to_string(),
    }
}

/// Re-reads the licence status and re-applies the main window's OS-level title.
///
/// The in-app title bar is reactive and follows the status by itself; the OS-level one is a
/// `set_title` call and stays wherever it was last put. Every write to the cached status calls
/// this, so the two can't drift apart within a session. Without it, someone activating a licence
/// kept "Cmdr – Personal use only" in the Dock's window list, Mission Control, and the Window
/// menu until the next launch.
///
/// A no-op before the main window exists, so `setup` can call it on the way past.
pub fn refresh_window_title(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let title = get_window_title(&get_app_status(app));
    if let Err(e) = window.set_title(&title) {
        log::warn!("Couldn't update the main window title: {}", e);
    }
}

/// Reset license data, returning the app to unlicensed (Personal) state.
pub fn reset_license(app: &tauri::AppHandle) {
    crate::licensing::verification::clear_license_cache();
    if let Ok(store) = app.store("license.json") {
        store.delete("license_key");
        store.delete("license_short_code");
        store.delete(STORE_KEY_CACHED_STATUS);
        store.delete(STORE_KEY_LAST_VALIDATION);
        store.delete(STORE_KEY_EXPIRATION_SHOWN);
        store.delete(STORE_KEY_REMINDER_LAST_DISMISSED);
        store.delete(STORE_KEY_CLOCK_HIGH_WATER);
    }

    refresh_window_title(app);
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

// ============================================================================
// Mock mode for local testing (debug and E2E builds only)
// ============================================================================

/// Get mock status from environment variable.
///
/// Compiled into debug builds and `playwright-e2e` builds: the i18n screenshot run drives its
/// license surfaces through this on the release-profile E2E binary. Neither ships, so a stray
/// `CMDR_MOCK_LICENSE` can never unlock a user's install.
///
/// Set CMDR_MOCK_LICENSE to one of:
/// - "personal" - No license (no reminder)
/// - "personal_reminder" - No license (shows commercial reminder modal)
/// - "commercial" - Active commercial subscription
/// - "perpetual" - Active perpetual license
/// - "expired" - Expired subscription (shows modal)
/// - "expired_no_modal" - Expired subscription (modal already shown)
#[cfg(any(debug_assertions, feature = "playwright-e2e"))]
fn get_mock_status(_app: &tauri::AppHandle) -> Option<AppStatus> {
    let mock_value = std::env::var("CMDR_MOCK_LICENSE").ok()?;

    match mock_value.to_lowercase().as_str() {
        "personal" => Some(AppStatus::Personal {
            show_commercial_reminder: false,
        }),
        "personal_reminder" => Some(AppStatus::Personal {
            show_commercial_reminder: true,
        }),
        // Legacy: treat "supporter" / "supporter_reminder" as Personal
        "supporter" => Some(AppStatus::Personal {
            show_commercial_reminder: false,
        }),
        "supporter_reminder" => Some(AppStatus::Personal {
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

    /// The title's words live in the catalog now, so these three pin the part
    /// this function still owns: WHICH key each licence status maps to. They also
    /// hold the locale lock and pin English, because `menu_t` reads a
    /// process-wide catalog that another test writes — unpinned, they'd assert
    /// English on a contributor's Hungarian Mac and fail.
    #[test]
    fn test_get_window_title_personal() {
        let _guard = crate::intl::native_strings::lock_active_locale_for_tests();
        crate::intl::set_language_preference(Some("en".to_string()));
        let status = AppStatus::Personal {
            show_commercial_reminder: false,
        };
        assert_eq!(get_window_title(&status), "Cmdr – Personal use only");
    }

    #[test]
    fn test_get_window_title_personal_with_reminder() {
        let _guard = crate::intl::native_strings::lock_active_locale_for_tests();
        crate::intl::set_language_preference(Some("en".to_string()));
        let status = AppStatus::Personal {
            show_commercial_reminder: true,
        };
        assert_eq!(get_window_title(&status), "Cmdr – Personal use only");
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
        let _guard = crate::intl::native_strings::lock_active_locale_for_tests();
        crate::intl::set_language_preference(Some("en".to_string()));
        let status = AppStatus::Expired {
            organization_name: Some("Old Corp".to_string()),
            expired_at: "2026-01-01".to_string(),
            show_modal: true,
        };
        assert_eq!(get_window_title(&status), "Cmdr – Personal use only");
    }

    #[test]
    fn test_license_type_serialization() {
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
    fn test_failed_validation_recently() {
        // Never failed
        assert!(!failed_validation_recently(0, 1_000_000));
        // Failed just now
        assert!(failed_validation_recently(1_000_000, 1_000_000));
        // Failed inside the cooldown window
        assert!(failed_validation_recently(
            1_000_000,
            1_000_000 + FAILED_VALIDATION_RETRY_COOLDOWN_SECS - 1
        ));
        // Failed exactly at the cooldown boundary: retry allowed
        assert!(!failed_validation_recently(
            1_000_000,
            1_000_000 + FAILED_VALIDATION_RETRY_COOLDOWN_SECS
        ));
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
