//! License verification and status management.
//!
//! Uses Ed25519 signatures for offline license validation.
//! The public key is embedded at compile time.

mod app_status;
mod validation_client;
mod verification;

pub use app_status::{
    AppStatus, LicenseType, get_app_status, get_window_title, mark_commercial_reminder_dismissed,
    mark_expiration_modal_shown, needs_validation, reset_license, update_cached_status, validate_license_async,
};
pub use verification::{LicenseInfo, activate_license, activate_license_async, get_license_info};

use serde::{Deserialize, Serialize};

/// License data encoded in the license key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseData {
    pub email: String,
    pub transaction_id: String,
    pub issued_at: String,
    #[serde(rename = "type")]
    pub license_type: Option<String>,
    #[serde(rename = "organizationName")]
    pub organization_name: Option<String>,
}
