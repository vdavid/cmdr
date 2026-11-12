//! License verification and status management.
//!
//! Uses Ed25519 signatures for offline license validation.
//! The public key is embedded at compile time.

mod app_status;
mod device_id;
mod key_storage;
mod license_key;
mod offline_policy;
mod validation_client;
mod verification;

pub use app_status::{
    AppStatus, commit_license, get_app_status, has_been_validated, mark_commercial_reminder_dismissed,
    mark_expiration_modal_shown, needs_validation, refresh_window_title, reset_license, update_cached_status,
    validate_license_async, write_cached_status_without_validation,
};
pub use key_storage::move_license_key_into_secret_store;
pub use license_key::{LicenseInfo, VerifyResult, get_license_info, verify_license_async};
pub use verification::LicenseActivationError;

use serde::{Deserialize, Serialize};

/// Redact an email for logging: "test_user_14@example.com" -> "t***@example.com"
pub(crate) fn redact_email(email: &str) -> String {
    match email.find('@') {
        Some(0) | None => "***".to_string(),
        Some(at) => format!("{}***{}", &email[..1], &email[at..]),
    }
}

/// License data encoded in the license key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseData {
    pub email: String,
    #[serde(rename = "transactionId")]
    pub transaction_id: String,
    #[serde(rename = "issuedAt")]
    pub issued_at: String,
    #[serde(rename = "type")]
    pub license_type: Option<String>,
    #[serde(rename = "organizationName")]
    pub organization_name: Option<String>,
    #[serde(rename = "shortCode")]
    pub short_code: Option<String>,
    /// RFC 3339, only on a dated license. Signed in, so the app enforces it offline.
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<String>,
}

/// Type of license.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LicenseType {
    CommercialSubscription,
    CommercialPerpetual,
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
