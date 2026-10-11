//! S3 volume wiring: turns "the user asked for this place" into a registered
//! `S3Volume`.
//!
//! ❗ **A backend never registers itself.** The same shape as
//! `webdav_volume_wiring`: dial, register (retiring any predecessor), and
//! remember the place for next time, in that order. The rule:
//! `network/DETAILS.md` § "Backends never register themselves".

use std::sync::Arc;

use cmdr_s3::{S3ConnectError, S3ConnectionParams, S3Volume, UnattendedReconnect};

use super::connect_wiring::{self, AttemptTable};
use super::one_shot_credentials::{self, SecretOffer};
use super::s3_known_places::{self, KnownS3Place, S3ProviderChoice};
use super::saved_server_fields::SavedServerOutcome;

/// What a connect attempt produced, in the terms a sign-in UI branches on.
///
/// ❗ Every outcome is a variant, including the ones that look like failures,
/// and ❌ none may be recovered from a message.
#[derive(Debug, PartialEq, Eq)]
pub enum S3Connection {
    /// A live volume, registered under `volume_id`.
    Connected {
        /// The id every listing and tab is filed under.
        volume_id: String,
    },
    /// The server named the keys as wrong (`SignatureDoesNotMatch`,
    /// `InvalidAccessKeyId`).
    KeysRejected,
    /// The bucket refused this key: a wrong key, or one without rights here.
    AccessDenied,
    /// The account root needs `ListBuckets`, and this key may not (or, on some
    /// servers, its secret is wrong).
    BucketListRefused,
    /// No bucket by that name on this endpoint.
    BucketNotFound,
    /// The bucket lives in another region; `region` is the right one when the
    /// server said.
    RegionMismatch {
        /// The bucket's region, when known.
        region: Option<String>,
    },
    /// This Mac's clock is too far off for the server to accept a signature.
    ClockSkewed,
    /// The store held no secret, so nothing was offered.
    NeedsCredentials,
    /// The TLS handshake didn't trust the endpoint's certificate.
    CertificateUntrusted,
    /// The address answers, but not as S3.
    NotAnS3Endpoint,
    /// The provider's fields can't make an endpoint.
    InvalidProvider,
    /// The probe didn't finish inside the connect budget.
    TimedOut,
    /// No route, refused, DNS, or a transport-level breakdown.
    Unreachable,
    /// The user called it off, or the account moved to a new endpoint while the
    /// dial was out. Nothing was registered, remembered, or stored.
    Cancelled,
}

/// The connect attempts a user could still call off (`connect_wiring.rs`).
static ATTEMPTS: AttemptTable = AttemptTable::new("an s3");

/// Calls off the connect filed under `attempt_id`, answering whether one was
/// running.
pub fn cancel_connect(attempt_id: &str) -> bool {
    ATTEMPTS.cancel(attempt_id)
}

/// Calls off every connect dialing `place`, for a move that took it to a new
/// address (`server_move.rs`). Answers how many were running.
pub fn cancel_dials_to(place: &str) -> usize {
    ATTEMPTS.cancel_dials_to(place)
}

/// Dials the place `provider` + `access_key_id` + `bucket` names, and on
/// success registers the volume and remembers the place.
///
/// `secret` is what a sign-in sheet just collected (`None` reads the store);
/// `remember: false` keeps it out of the Keychain (`one_shot_credentials.rs`).
/// `display_name` is the ACCOUNT's name as an add typed it, adopted once the
/// dial lands; a blank one (every redial of a saved place) leaves the account's
/// name alone (`s3_known_places::adopt_typed_name`). ❗ A cancelled connect
/// leaves nothing behind.
pub async fn connect_and_register(
    display_name: &str,
    provider: S3ProviderChoice,
    access_key_id: &str,
    bucket: Option<&str>,
    auto_reconnect: bool,
    attempt_id: &str,
    secret: Option<SecretOffer>,
) -> S3Connection {
    let place = KnownS3Place {
        provider,
        access_key_id: access_key_id.trim().to_string(),
        bucket: bucket.map(str::trim).filter(|b| !b.is_empty()).map(str::to_string),
        auto_reconnect,
        pinned: true,
        last_connected_at: String::new(),
    };
    connect_and_register_since(
        connect_wiring::DialTicket::now(),
        display_name,
        place,
        attempt_id,
        secret,
    )
    .await
}

/// [`connect_and_register`] for `place` as the store holds it, read at
/// `set_out`, which was taken BEFORE that read (`connect_wiring::DialTicket`).
pub async fn connect_and_register_since(
    set_out: connect_wiring::DialTicket,
    display_name: &str,
    place: KnownS3Place,
    attempt_id: &str,
    secret: Option<SecretOffer>,
) -> S3Connection {
    let place = KnownS3Place {
        // A first connect pins the new place; `remember` keeps the stored pin
        // for a place already saved.
        pinned: true,
        last_connected_at: chrono::Utc::now().to_rfc3339(),
        ..place
    };
    let Ok(params) = place.params() else {
        return S3Connection::InvalidProvider;
    };
    let volume_id = s3_known_places::place_id(&params);
    // The account too: a move takes every place under the key, a bucket nobody
    // saved yet included.
    let places = vec![volume_id.clone(), s3_known_places::account_id(&params)];
    let (host, offer) =
        one_shot_credentials::host_for_dial(&params.credential_service(), params.access_key_id(), secret).await;
    let (cancel, attempt) = ATTEMPTS.register_dialing(attempt_id, places, set_out);
    // The root reads as its account, so a name typed in this very add is its label already.
    let label = match place.bucket {
        None if super::saved_server_fields::is_named(display_name) => display_name.trim().to_string(),
        _ => s3_known_places::place_label(&place),
    };
    let volume = match cmdr_s3::connect_s3_volume(&label, &volume_id, params, host, cancel).await {
        Ok(volume) => volume,
        Err(e) => return failed(e),
    };
    // The account moved while this dial was out: remembering the place here
    // would save the old endpoint again, beside the moved account
    // (`connect_wiring.rs`).
    let Some(_landing) = attempt.land().await else {
        connect_wiring::let_go(Arc::new(volume)).await;
        return S3Connection::Cancelled;
    };
    // Only now: a secret filed before the dial outlived a refused one.
    offer.went_through().await;
    connect_wiring::install_retiring_incumbent(&volume_id, Arc::new(volume)).await;
    s3_known_places::adopt_typed_name(&place, display_name);
    s3_known_places::remember(place);
    log::info!(target: "volume", "registered S3 volume {volume_id}");
    S3Connection::Connected { volume_id }
}

/// The typed connect errors, widened into the outcome the frontend branches on.
/// The diagnostic strings stay in the log.
fn failed(error: S3ConnectError) -> S3Connection {
    match error {
        S3ConnectError::KeysRejected => S3Connection::KeysRejected,
        S3ConnectError::AccessDenied => S3Connection::AccessDenied,
        S3ConnectError::BucketListRefused => S3Connection::BucketListRefused,
        S3ConnectError::NoSuchBucket => S3Connection::BucketNotFound,
        S3ConnectError::WrongRegion { region } => S3Connection::RegionMismatch { region },
        S3ConnectError::ClockSkewed => S3Connection::ClockSkewed,
        S3ConnectError::NeedsCredentials => S3Connection::NeedsCredentials,
        S3ConnectError::CertificateUntrusted => S3Connection::CertificateUntrusted,
        S3ConnectError::NotAnS3Endpoint => S3Connection::NotAnS3Endpoint,
        S3ConnectError::InvalidProvider => S3Connection::InvalidProvider,
        S3ConnectError::TimedOut => S3Connection::TimedOut,
        S3ConnectError::Cancelled => S3Connection::Cancelled,
        S3ConnectError::Unreachable(what) | S3ConnectError::Transport(what) => {
            log::info!(target: "volume", "an s3 connection didn't come up: {what}");
            S3Connection::Unreachable
        }
    }
}

/// Saves a place without dialing it: an edit, or an add that doesn't connect.
///
/// ❗ The identity (provider, key id, bucket) is the place itself, so an edit
/// moves only the "reconnect automatically" switch, which reaches a connected
/// volume at once. `display_name` is the ACCOUNT's name as an add typed it
/// (blank leaves it alone: `s3_known_places::adopt_typed_name`); renaming an
/// account is `s3_known_places::rename_account`. A name reaches the switcher and
/// the hub at once (both read the store) and a live volume on its next connect.
pub async fn save_without_connecting(place: KnownS3Place, display_name: &str) -> SavedServerOutcome {
    let Some(volume_id) = place.volume_id() else {
        // Nothing saved under an endpoint no dial could reach: there's no field
        // on the sheet this refusal could go under but the address, and it's
        // validated there first.
        return SavedServerOutcome::Unreachable;
    };
    on_live_volume(&volume_id, |live| live.set_auto_reconnect(place.auto_reconnect));
    s3_known_places::adopt_typed_name(&place, display_name);
    s3_known_places::remember(place);
    SavedServerOutcome::Saved
}

/// Moves "reconnect automatically" on a saved place: the store, and a connected
/// volume's live copy. Answers whether a saved entry was there.
pub fn apply_auto_reconnect(volume_id: &str, on: bool) -> bool {
    if !s3_known_places::set_auto_reconnect(volume_id, on) {
        return false;
    }
    on_live_volume(volume_id, |live| live.set_auto_reconnect(on));
    true
}

/// Whether an unattended reconnect can actually happen for a mounted volume.
/// `None` when nothing S3 is registered under that id.
pub async fn unattended_reconnect(volume_id: &str) -> Option<UnattendedReconnect> {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let volume = manager.get(volume_id)?;
    let s3 = volume.as_any().downcast_ref::<S3Volume>()?;
    Some(s3.unattended_reconnect().await)
}

/// Drops an S3 volume's client and takes it out of the registry, answering
/// whether there was one.
pub async fn disconnect(volume_id: &str) -> bool {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let Some(volume) = manager.get(volume_id) else {
        return false;
    };
    // Typed rather than a guess at the id's shape.
    let Some(s3) = volume.as_any().downcast_ref::<S3Volume>() else {
        return false;
    };
    s3.disconnect().await;
    manager.unregister(volume_id);
    crate::volume_broadcast::emit_volumes_changed();
    log::info!(target: "volume", "disconnected S3 volume {volume_id}");
    true
}

/// Runs `act` on the registered `S3Volume` under `volume_id`, if that's what
/// is there. Typed rather than a guess at the id's shape.
fn on_live_volume(volume_id: &str, act: impl FnOnce(&S3Volume)) {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    if let Some(live) = manager.get(volume_id)
        && let Some(s3) = live.as_any().downcast_ref::<S3Volume>()
    {
        act(s3);
    }
}

/// The secret-store service for an account, built by the crate's own
/// `credential_service` so what a writer files is exactly what a dial reads.
pub fn credential_service(provider: &S3ProviderChoice, access_key_id: &str) -> Option<String> {
    let params = S3ConnectionParams::new(provider.to_provider().ok()?, access_key_id, None).ok()?;
    Some(params.credential_service())
}

#[cfg(test)]
#[path = "s3_volume_wiring_test.rs"]
mod s3_volume_wiring_test;
