//! The IPC surface for S3 accounts that the protocol-agnostic
//! `commands::servers` facade has no reason to widen: the secret, and whether a
//! mounted place can come back on its own.
//!
//! Pass-throughs. The connect flow lives in `network::s3_volume_wiring`, the
//! place list in `network::s3_known_places`, and the secret store in
//! `network::keychain`.
//!
//! ❗ **The secret is the ACCOUNT's**: one entry per endpoint plus access key id
//! (`S3ConnectionParams::credential_service`, scoped by the key id), which every
//! bucket place under that key reads.

use serde::{Deserialize, Serialize};

use crate::network::keychain::{self, KeychainError};
use crate::network::s3_known_places::{self, KnownS3Place, S3ProviderChoice};
use crate::network::s3_volume_wiring;
use cmdr_s3::UnattendedReconnect;
use std::time::Duration;

use crate::deadline::blocking_with_timeout;
use crate::s3_costs::{CostEstimate, CostEstimateRequest};

/// Whether an S3 volume can actually come back on its own as it stands. The
/// WebDAV twin (`WebdavUnattendedReconnect`), for the same reasons: ❌ never
/// derive it in the frontend from a credential check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum S3UnattendedReconnect {
    /// On, and it works.
    Possible,
    /// The switch is off.
    SwitchOff,
    /// ❗ On, and nothing is stored to redial with: the state a UI warns about.
    NoStoredSecret,
}

impl From<UnattendedReconnect> for S3UnattendedReconnect {
    fn from(answer: UnattendedReconnect) -> Self {
        match answer {
            UnattendedReconnect::Possible => Self::Possible,
            UnattendedReconnect::SwitchOff => Self::SwitchOff,
            UnattendedReconnect::NoStoredSecret => Self::NoStoredSecret,
        }
    }
}

/// The answer when the provider can't make an endpoint, so there's no key to
/// file a secret under. ❗ `Other`, ❌ not `AccessDenied`: nobody said no.
fn not_an_account() -> KeychainError {
    KeychainError::Other("the provider doesn't make an endpoint".to_string())
}

/// The answer when the secret store didn't come back in time.
fn keychain_timed_out() -> KeychainError {
    KeychainError::Other("the secret store didn't answer".to_string())
}

/// Saves the secret access key for one account.
///
/// ❗ **This command IS the "remember the secret" switch**, the WebDAV twin's
/// contract: `has_s3_credentials` reads it back, `delete_s3_credentials` turns
/// it off, and there's no second flag anywhere. On a blocking task: the store
/// can put a Keychain prompt in front of it.
#[tauri::command]
#[specta::specta]
pub async fn save_s3_credentials(
    provider: S3ProviderChoice,
    access_key_id: String,
    secret: String,
) -> Result<(), KeychainError> {
    let access_key_id = access_key_id.trim().to_string();
    let Some(service) = s3_volume_wiring::credential_service(&provider, &access_key_id) else {
        return Err(not_an_account());
    };
    blocking_with_timeout(Duration::from_secs(15), Err(keychain_timed_out()), move || {
        keychain::save_credentials(&service, Some(&access_key_id), &access_key_id, &secret)
    })
    .await
}

/// Whether a secret is stored for one account. ❗ No command hands the secret
/// itself to the frontend, which asks through `servers.rs`'s `has_server_secret`.
/// A store that didn't answer in time reads as `false`.
pub(crate) async fn has_s3_credentials(provider: S3ProviderChoice, access_key_id: String) -> bool {
    let access_key_id = access_key_id.trim().to_string();
    let Some(service) = s3_volume_wiring::credential_service(&provider, &access_key_id) else {
        return false;
    };
    blocking_with_timeout(Duration::from_secs(15), false, move || {
        keychain::has_credentials(&service, Some(&access_key_id))
    })
    .await
}

/// Forgets the stored secret for one account, and so for every place under it.
/// The frontend asks through `servers.rs`'s `forget_server_secret`.
pub(crate) async fn delete_s3_credentials(
    provider: S3ProviderChoice,
    access_key_id: String,
) -> Result<(), KeychainError> {
    let access_key_id = access_key_id.trim().to_string();
    let Some(service) = s3_volume_wiring::credential_service(&provider, &access_key_id) else {
        return Err(not_an_account());
    };
    blocking_with_timeout(Duration::from_secs(15), Err(keychain_timed_out()), move || {
        keychain::delete_credentials(&service, Some(&access_key_id))
    })
    .await
}

/// One saved S3 place as the frontend reads it: the stored entry plus the
/// volume id its row and its switcher entry carry, so a caller matches by id
/// rather than re-deriving one.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedS3Place {
    /// The place's volume id (`cmdr_fs::volume::s3_volume_id`).
    pub volume_id: String,
    /// The provider preset, with Other's endpoint, region, and path style.
    pub provider: S3ProviderChoice,
    /// The account's key.
    pub access_key_id: String,
    /// The bucket, or `null` for the account root.
    pub bucket: Option<String>,
    /// The name a person gave its ACCOUNT, empty when nobody did. ❗ The
    /// account's, ❌ never the place's: a bucket reads as its own name.
    pub display_name: String,
    /// The place's "Reconnect automatically" switch.
    pub auto_reconnect: bool,
    /// Whether it shows in the volume switcher.
    pub pinned: bool,
}

impl SavedS3Place {
    fn of(entry: KnownS3Place) -> Option<Self> {
        Some(Self {
            volume_id: entry.volume_id()?,
            display_name: s3_known_places::account_name(&entry),
            provider: entry.provider,
            access_key_id: entry.access_key_id,
            bucket: entry.bucket,
            auto_reconnect: entry.auto_reconnect,
            pinned: entry.pinned,
        })
    }
}

/// Every S3 place the user has saved, with its provider, for an edit sheet
/// that has to show (and resend) what identifies the place. A place whose
/// provider no longer makes an endpoint has no id, so it's left out.
/// What a copy, move, or delete about to start will cost at list prices, one
/// entry per S3 provider it touches (`crate::s3_costs`). Reads the dialog's
/// settled scan preview, never S3; the price table may come off disk, hence the
/// blocking pool and the deadline, which answers "no estimate".
#[tauri::command]
#[specta::specta]
pub async fn estimate_operation_cost(app: tauri::AppHandle, request: CostEstimateRequest) -> Vec<CostEstimate> {
    let data_dir = crate::config::resolved_app_data_dir(&app).ok();
    blocking_with_timeout(ESTIMATE_TIMEOUT, Vec::new(), move || {
        crate::s3_costs::estimate(&request, data_dir.as_deref())
    })
    .await
}

const ESTIMATE_TIMEOUT: Duration = Duration::from_secs(2);

#[tauri::command]
#[specta::specta]
pub fn get_known_s3_places() -> Vec<SavedS3Place> {
    s3_known_places::all()
        .into_iter()
        .filter_map(SavedS3Place::of)
        .collect()
}

/// Whether an S3 volume can come back on its own as it stands. `null` when
/// nothing S3 is registered under that id. ❗ Reads the store: ask when a
/// banner renders, ❌ never poll.
#[tauri::command]
#[specta::specta]
pub async fn get_s3_unattended_reconnect(volume_id: String) -> Option<S3UnattendedReconnect> {
    s3_volume_wiring::unattended_reconnect(&volume_id)
        .await
        .map(S3UnattendedReconnect::from)
}
