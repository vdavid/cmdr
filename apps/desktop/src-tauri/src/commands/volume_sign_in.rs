//! The volume sign-in commands: what form a sign-in takes, reconnect, reconnect with
//! typed credentials, and filing a sign-in that already connected. Backend-neutral
//! despite two of the names: each asks whatever volume is registered under the id, so
//! SMB, SFTP, WebDAV, and S3 go through them unchanged, and the frontend never branches
//! on backend.

use crate::file_system::volume::reconnect_error::ReconnectError;
use crate::network::known_shares;

/// What FORM a "Sign in" affordance on this volume takes, right now: which fields
/// the sheet renders, and whether the username among them is editable.
///
/// ❗ **Asked when the affordance renders, ❌ never carried on a connect result.**
/// A backend that authenticates per connection can prove itself with a different
/// credential each time it dials, so an answer captured when the volume was
/// opened describes a session that may already be gone. This reads the live
/// volume, which is the only moment the answer is true.
///
/// A volume with no sign-in story of its own, and an id nothing is registered
/// under, both answer `Password` (`Volume::sign_in_prompt`'s default): the one
/// safe way to be wrong here is a needless password box, because a wrong
/// `Nothing` is a volume the user can't sign in to at all.
///
/// ❗ The answer is a tagged union, so the frontend switches on `kind` and
/// ❌ never derives the form from the protocol, the mode the sheet is in, or the
/// rung a connect result mentioned.
#[tauri::command]
#[specta::specta]
// No timeout wrapper: this reads a lock and a map, and reaches no device.
pub async fn get_volume_sign_in_state(volume_id: String) -> cmdr_fs::volume::SignInShape {
    use crate::file_system::volume::manager::get_volume_manager;

    match get_volume_manager().get(&volume_id) {
        Some(volume) => volume.sign_in_prompt(),
        // ❗ A saved SMB share not mounted right now: the share is the place, and
        // the account is a field on it (`docs/specs/saved-smb-shares.md`). No
        // guest, since an unauthenticated mount is what asking follows from.
        None if known_shares::share_by_volume_id(&volume_id).is_some() => {
            cmdr_fs::volume::SignInShape::UsernamePassword { guest_allowed: false }
        }
        None => cmdr_fs::volume::SignInShape::Password,
    }
}

/// Tries to rebuild a Disconnected volume's session in place.
///
/// ❗ Backend-neutral: every remote backend implements `attempt_reconnect`, and
/// the frontend's reconnect manager drives SMB, SFTP, and WebDAV through this
/// one command. Called on each backoff tick (and on "Retry now" / lazy nav-time
/// retry). The backend single-flights concurrent calls, so the frontend is free
/// to fire on its own schedule. Returns `Ok(())` on success (the volume now
/// reports `Direct`), or a typed [`ReconnectError`] saying why the rebuild
/// didn't happen.
///
/// A volume whose backend can't redial yields `ReconnectError::Volume` carrying
/// `VolumeError::NotSupported` (the trait default).
#[tauri::command]
#[specta::specta]
pub async fn reconnect_volume(volume_id: String) -> Result<(), ReconnectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| ReconnectError::VolumeNotFound {
            volume_id: volume_id.clone(),
        })?;

    volume.attempt_reconnect().await.map_err(ReconnectError::from)
}

/// What a sign-in that connected has to tell the person (`cmdr_fs::volume::secret_store::SignedIn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SignedInReply {
    /// The Remember box was on and the Keychain wouldn't keep the password. The
    /// connection stands; the sheet says `secret_not_stored`.
    pub remember_refused: bool,
}

/// Reconnects a volume with freshly-entered credentials.
///
/// ❗ Backend-neutral, like [`reconnect_volume`]. Invoked by the "Sign in"
/// affordance shown when an in-place reconnect gave up on an auth failure (a
/// `needs_credentials` `volume-connection-changed` event). The volume runs the
/// standard reconnect and, ❗ only once that connected, files the credentials the
/// way `remember` (the sheet's Remember box) asks: on saves them, off only
/// refreshes an entry still there (#530). On success the backend emits
/// `volume-connection-changed { state: "connected" }`.
///
/// ❗ Whether the USERNAME may change is the backend's call, not this command's:
/// SMB accepts a new one and rewrites its params, SFTP and WebDAV refuse because
/// the volume id IS the account. `SignInShape` is what tells the sheet which it
/// is. A backend that can't redial yields `ReconnectError::Volume` carrying
/// `VolumeError::NotSupported`.
#[tauri::command]
#[specta::specta]
pub async fn reconnect_volume_with_credentials(
    volume_id: String,
    username: String,
    password: String,
    remember: bool,
) -> Result<SignedInReply, ReconnectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| ReconnectError::VolumeNotFound {
            volume_id: volume_id.clone(),
        })?;

    volume
        .reconnect_with_credentials(cmdr_fs::volume::secret_store::SignIn {
            username,
            secret: password,
            remember,
        })
        .await
        .map(|signed_in| SignedInReply {
            remember_refused: signed_in.remember_refused,
        })
        .map_err(ReconnectError::from)
}

/// Files the credentials of a sign-in that already connected this volume, ❌
/// without dialing: the sign-in sheet's "Try again" after `remember_refused`
/// (#603). A second [`reconnect_volume_with_credentials`] would find the session
/// live (SFTP, SMB), dial nothing, and so save nothing.
///
/// ❗ Only for the secret that round connected with; the frontend keeps that
/// promise. The backend files it by the post-dial rule (`Volume::remember_sign_in`)
/// and refuses what the sign-in refuses.
#[tauri::command]
#[specta::specta]
pub async fn remember_volume_sign_in(
    volume_id: String,
    username: String,
    password: String,
    remember: bool,
) -> Result<SignedInReply, ReconnectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| ReconnectError::VolumeNotFound {
            volume_id: volume_id.clone(),
        })?;

    volume
        .remember_sign_in(cmdr_fs::volume::secret_store::SignIn {
            username,
            secret: password,
            remember,
        })
        .await
        .map(|signed_in| SignedInReply {
            remember_refused: signed_in.remember_refused,
        })
        .map_err(ReconnectError::from)
}

#[cfg(test)]
#[path = "volume_sign_in_test.rs"]
mod volume_sign_in_test;
