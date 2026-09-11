//! SFTP volume wiring: turns "the user asked for this server" into a registered
//! `SftpVolume`.
//!
//! ❗ **A backend never registers itself.** This module knows both the backend
//! and the volume registry, and neither of those knows this module — the same
//! shape `mtp::volume_wiring` and `network::smb_upgrade` take. The rule and its
//! rationale: `network/DETAILS.md` § "Backends never register themselves".
//!
//! The commands above this (`commands/sftp.rs`) are pass-throughs; the flow lives
//! here, because a connect is three things happening in one order: dial, register
//! (retiring any predecessor), and remember the server for next time.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_sftp::auth::UnattendedReconnect;
use cmdr_sftp::volume::HostKeyApproval;
use cmdr_sftp::{SftpConnectError, SftpConnectOutcome, SftpConnectionParams, SftpVolume};

use super::connect_wiring::{self, AttemptTable};
use super::live_server_edit::{self, ConnectedPlace, PlaceEdit};
use super::one_shot_credentials::{self, SecretOffer};
use super::saved_server_fields::{self, SavedServerOutcome};
use super::sftp_known_servers::{self, KnownSftpServer};

/// What a connect attempt produced, in the terms a sign-in UI branches on.
///
/// ❗ Every outcome is a variant, including the ones that look like failures.
/// The frontend has to tell "the key needs approving" from "the password is
/// wrong" from "nothing was ever offered", and ❌ none of those may be recovered
/// from a message.
pub enum SftpConnection {
    /// A live volume, registered under `volume_id`.
    Connected {
        /// The id every listing, tab, and index entry is filed under.
        volume_id: String,
        /// Which credential proved us, which is what decides what a dropped
        /// session may do on its own.
        rung: cmdr_sftp::auth::AuthRungUsed,
    },
    /// The server's host key needs a human. ❗ No session is held across the
    /// prompt: this dial has already been dropped.
    NeedsHostKeyApproval(cmdr_sftp::transport::HostKeyPrompt),
    /// The key is explicitly revoked in `~/.ssh/known_hosts`. ❌ Not approvable:
    /// a revocation says the key is known to be compromised.
    HostKeyRevoked {
        /// The SSH key-type name the server presented.
        algorithm: String,
        /// Its OpenSSH `SHA256:…` fingerprint.
        fingerprint: String,
    },
    /// Every rung was refused. ❗ Retrying with the same secret can lock the
    /// account, so only a freshly typed one moves this forward.
    AuthenticationRejected,
    /// Nothing was ever offered: no agent, no readable key file, no stored
    /// secret. ❗ Not the same as a rejection, and telling someone who has never
    /// entered a password that theirs is wrong is what collapsing the two does.
    NeedsCredentials,
    /// The handshake didn't finish inside the connect budget.
    TimedOut,
    /// No route, refused, DNS, or the SFTP subsystem itself declining.
    Unreachable,
    /// The user called it off. ❗ Nothing was registered, remembered, or stored,
    /// so there is nothing to report and nothing to retry.
    Cancelled,
}

// ============================================================================
// Calling a connect off
// ============================================================================

/// The connect attempts a user could still call off, and the guard that empties
/// the table. The mechanism, and why each backend holds its OWN table:
/// `connect_wiring.rs`.
static ATTEMPTS: AttemptTable = AttemptTable::new("an sftp");

/// Calls off the connect filed under `attempt_id`, answering whether one was
/// running. An id nobody is holding is a plain `false`.
pub fn cancel_connect(attempt_id: &str) -> bool {
    ATTEMPTS.cancel(attempt_id)
}

/// Dials `params`, and on success registers the volume and remembers the server.
///
/// `attempt_id` is the caller's own name for this attempt, and what
/// [`cancel_connect`] needs to call it off. ❗ A cancelled connect leaves
/// nothing behind: no volume, no saved server, no secret.
///
/// `secret` is what a sign-in sheet just collected, and `None` is every dial
/// that isn't answering one (the dial then reads the store, as it always has).
/// ❗ `remember: false` runs the dial against a store wrapper that answers this
/// one account from memory and forgets it when the attempt ends, so ❌ no secret
/// reaches the Keychain and none is held by the volume:
/// `one_shot_credentials.rs`.
///
/// ❗ Every dial goes through `cmdr_sftp::connect_sftp_volume`, and a connect the
/// caller walks away from leaves the far end nothing: the SFTP hello's teardown
/// runs from a guard's `Drop` rather than from anyone's `await`. Calling one OFF
/// goes through the token instead, which stops the dial where it stands in every
/// phase and is what makes it answer `Cancelled`.
/// `crates/cmdr-sftp/DETAILS.md` § "2. An abandoned `Sftp::new`" has the terms.
///
/// `display_name` and `start_folder` aren't connection params: they travel
/// beside `params` into the saved entry, which a connect rebuilds whole. ❗ So a
/// caller that doesn't set them passes the SAVED values, or a connect would wipe
/// what an edit stored. A start folder the root no longer holds is dropped here
/// rather than saved. The volume is named by the label
/// (`saved_server_fields::server_label`), so an unnamed server's tab and switcher
/// row both read `username@host`.
pub async fn connect_and_register(
    display_name: &str,
    start_folder: Option<String>,
    params: SftpConnectionParams,
    attempt_id: &str,
    secret: Option<SecretOffer>,
) -> SftpConnection {
    let volume_id = cmdr_fs::volume::sftp_volume_id(&params.host, params.port, &params.username);
    let start_folder = saved_server_fields::start_folder_for_root(&params.remote_root.to_string_lossy(), start_folder);
    let (host, _offer) =
        one_shot_credentials::host_for_dial(&params.credential_service(), &params.username, secret).await;
    let (cancel, _attempt) = ATTEMPTS.register(attempt_id);
    let label = saved_server_fields::server_label(display_name, &params.username, &params.host);
    let outcome = cmdr_sftp::connect_sftp_volume(&label, &volume_id, params.clone(), host, cancel).await;

    let volume = match outcome {
        Ok(SftpConnectOutcome::Connected(volume)) => volume,
        Ok(SftpConnectOutcome::NeedsHostKeyApproval(prompt)) => return SftpConnection::NeedsHostKeyApproval(prompt),
        Err(e) => return failed(e),
    };

    let rung = volume.auth_rung();
    connect_wiring::install_retiring_incumbent(&volume_id, Arc::new(volume)).await;
    sftp_known_servers::remember(KnownSftpServer {
        host: params.host.clone(),
        port: params.port,
        username: params.username.clone(),
        display_name: display_name.to_string(),
        remote_root: params.remote_root.to_string_lossy().to_string(),
        start_folder,
        key_file: params.key_file.as_ref().map(|p| p.to_string_lossy().to_string()),
        use_agent: params.use_agent,
        auto_reconnect: params.auto_reconnect,
        // A first connect pins the new place; `remember` carries the stored value
        // across for a server that is already saved, so a reconnect can't re-pin
        // one the user unpinned.
        pinned: true,
        last_connected_at: chrono::Utc::now().to_rfc3339(),
    });
    log::info!(target: "volume", "registered SFTP volume {volume_id}");
    SftpConnection::Connected { volume_id, rung }
}

/// The typed connect errors, widened into the outcome the frontend branches on.
///
/// The diagnostic string stays in the log: ❌ no backend prose reaches a user,
/// and the frontend's own copy is what a person reads.
fn failed(error: SftpConnectError) -> SftpConnection {
    match error {
        SftpConnectError::HostKeyRevoked { algorithm, fingerprint } => {
            SftpConnection::HostKeyRevoked { algorithm, fingerprint }
        }
        SftpConnectError::AuthenticationRejected => SftpConnection::AuthenticationRejected,
        SftpConnectError::NeedsCredentials => SftpConnection::NeedsCredentials,
        SftpConnectError::TimedOut => SftpConnection::TimedOut,
        SftpConnectError::Cancelled => SftpConnection::Cancelled,
        SftpConnectError::Unreachable(what) | SftpConnectError::Transport(what) => {
            log::info!(target: "volume", "an sftp connection didn't come up: {what}");
            SftpConnection::Unreachable
        }
    }
}

/// Saves a server without dialing it: an edit, or an add that doesn't connect.
///
/// ❗ The saved entry is the durable copy and a CONNECTED place holds a live one,
/// so an edit moves both, with no redial: the label and the root through an
/// instance sharing the session, the "reconnect automatically" switch at once,
/// and the key file and agent switch for the next unattended redial. The live
/// session confirms a moved root or start folder first, and ❗ a refusal moves
/// NOTHING, the store included. The order and the refusals: `live_server_edit.rs`.
pub async fn save_without_connecting(server: KnownSftpServer) -> SavedServerOutcome {
    let Ok(start_folder) =
        saved_server_fields::start_folder_under_root(&server.remote_root, server.start_folder.as_deref())
    else {
        return SavedServerOutcome::StartFolderOutsideRoot;
    };
    let server = KnownSftpServer { start_folder, ..server };
    let volume_id = cmdr_fs::volume::sftp_volume_id(&server.host, server.port, &server.username);
    let manager = crate::file_system::volume::manager::get_volume_manager();
    // Typed rather than a guess at the id's shape, the same way `disconnect` asks.
    let Some(live) = manager
        .get(&volume_id)
        .filter(|volume| volume.as_any().is::<SftpVolume>())
    else {
        // Not connected: the store is the only copy, and the next connect dials it.
        sftp_known_servers::remember(server);
        return SavedServerOutcome::Saved;
    };
    let sftp = live
        .as_any()
        .downcast_ref::<SftpVolume>()
        .expect("filtered to an SftpVolume above");
    let place = ConnectedPlace {
        volume_id,
        live: Arc::clone(&live),
        app_prefix: cmdr_fs::volume::sftp_app_root(&server.host, server.port, &server.username),
        saved_start_folder: sftp_known_servers::find(&server.host, server.port, &server.username)
            .and_then(|saved| saved.start_folder),
    };
    let label = server.label();
    let edit = PlaceEdit {
        label: &label,
        remote_root: &server.remote_root,
        start_folder: server.start_folder.as_deref(),
    };
    let accepted = match live_server_edit::check(
        &place,
        &edit,
        |name: &str, root: &Path| Arc::new(sftp.sharing_connection(name, root)) as Arc<dyn Volume>,
        live_server_edit::CHECK_BUDGET,
    )
    .await
    {
        Ok(accepted) => accepted,
        Err(refusal) => return refusal,
    };
    sftp.set_auto_reconnect(server.auto_reconnect);
    sftp.set_redial_params(
        Path::new(&server.remote_root),
        server.key_file.as_deref().map(PathBuf::from),
        server.use_agent,
    );
    sftp_known_servers::remember(server);
    accepted.install(manager);
    SavedServerOutcome::Saved
}

/// Whether an unattended reconnect can actually happen for a mounted volume.
///
/// `None` when nothing SFTP is registered under that id, which is the honest
/// answer: the rung is a fact about a live session, and there is no session to
/// have one.
pub async fn unattended_reconnect(volume_id: &str) -> Option<UnattendedReconnect> {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let volume = manager.get(volume_id)?;
    let sftp = volume.as_any().downcast_ref::<SftpVolume>()?;
    Some(sftp.unattended_reconnect().await)
}

/// Records a host key a human approved, ❗ only if the server still presents it.
///
/// Pure delegation to `cmdr_sftp::volume::approve_host_key`, which is where the
/// re-check lives; this exists so the command layer stays a pass-through.
pub async fn approve_host_key(
    host: &str,
    port: u16,
    algorithm: &str,
    fingerprint: &str,
) -> Result<HostKeyApproval, SftpConnectError> {
    cmdr_sftp::volume::approve_host_key(&crate::volume_host::host(), host, port, algorithm, fingerprint).await
}

/// Drops an SFTP volume's session and takes it out of the registry.
///
/// Answers whether there was one to disconnect. ❗ Dropping the session IS the
/// clean shutdown: `SftpVolume::disconnect` takes it out of its lock and lets it
/// go, and ❌ there is no `Sftp::close()` anywhere, because it hangs forever over
/// a `russh` channel.
pub async fn disconnect(volume_id: &str) -> bool {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let Some(volume) = manager.get(volume_id) else {
        return false;
    };
    // Typed rather than a guess at the id's shape: only an `SftpVolume` has a
    // session to drop, and asking the value itself is what makes "is this an SFTP
    // volume?" a fact instead of a string match.
    let Some(sftp) = volume.as_any().downcast_ref::<SftpVolume>() else {
        return false;
    };
    sftp.disconnect().await;
    manager.unregister(volume_id);
    crate::volume_broadcast::emit_volumes_changed();
    log::info!(target: "volume", "disconnected SFTP volume {volume_id}");
    true
}

#[cfg(test)]
#[path = "sftp_volume_wiring_test.rs"]
mod sftp_volume_wiring_test;
