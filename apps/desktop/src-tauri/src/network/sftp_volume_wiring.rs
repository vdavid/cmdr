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

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_sftp::auth::UnattendedReconnect;
use cmdr_sftp::volume::HostKeyApproval;
use cmdr_sftp::{SftpConnectError, SftpConnectOutcome, SftpConnectionParams, SftpVolume};
use tokio_util::sync::CancellationToken;

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

/// The connect attempts a user could still call off, by the id their caller made
/// up for one, each with the serial that says WHICH attempt holds the entry.
///
/// ❗ The id is the CALLER's, and that is the whole point: a connect can hold for
/// half a minute, so a sign-in dialog has to arm its cancel button before the
/// command answers — and an id the backend handed back would only arrive once
/// the connect was already over. The serial is what keeps a repeated id honest:
/// a finishing attempt only ever takes its OWN entry out.
static ATTEMPTS: LazyLock<Mutex<HashMap<String, (u64, CancellationToken)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The next attempt's serial.
static NEXT_ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// Takes one attempt's entry out of the table when its connect ends, however it
/// ends.
///
/// ❗ A guard rather than a call at each exit: a connect leaves through eight
/// arms, and the one that forgets is a token nobody ever collects.
struct AttemptGuard {
    id: String,
    serial: u64,
}

impl Drop for AttemptGuard {
    fn drop(&mut self) {
        let mut attempts = ATTEMPTS.lock_ignore_poison();
        // Only if it's still ours: a second connect under the same id has
        // replaced the entry, and taking that one out would leave it uncancelable.
        if attempts.get(&self.id).is_some_and(|(serial, _)| *serial == self.serial) {
            attempts.remove(&self.id);
        }
    }
}

/// Files `attempt_id` as cancelable and hands back the token the dial runs under.
fn register_attempt(attempt_id: &str) -> (CancellationToken, AttemptGuard) {
    let cancel = CancellationToken::new();
    let serial = NEXT_ATTEMPT.fetch_add(1, Ordering::Relaxed);
    ATTEMPTS
        .lock_ignore_poison()
        .insert(attempt_id.to_string(), (serial, cancel.clone()));
    (
        cancel,
        AttemptGuard {
            id: attempt_id.to_string(),
            serial,
        },
    )
}

/// Calls off the connect filed under `attempt_id`, answering whether one was
/// running.
///
/// ❗ An id nobody is holding is a plain `false`: a cancel racing a connect that
/// just finished is ordinary, and there is nothing wrong to report about it. The
/// entry stays until the dial itself notices, so ❌ this never reports on what the
/// attempt then did.
pub fn cancel_connect(attempt_id: &str) -> bool {
    let Some((_, cancel)) = ATTEMPTS.lock_ignore_poison().get(attempt_id).cloned() else {
        return false;
    };
    cancel.cancel();
    log::info!(target: "volume", "an sftp connect was called off");
    true
}

/// Dials `params`, and on success registers the volume and remembers the server.
///
/// `attempt_id` is the caller's own name for this attempt, and what
/// [`cancel_connect`] needs to call it off. ❗ A cancelled connect leaves
/// nothing behind: no volume, no saved server, no secret.
///
/// ❗ Every dial goes through `cmdr_sftp::connect_sftp_volume`, which runs it in
/// a task and awaits the join handle, so a connect the caller walks away from
/// detaches rather than dropping mid-handshake. Calling one OFF goes through the
/// token instead, which stops the dial where it stands in every phase.
/// `crates/cmdr-sftp/DETAILS.md` § "Crate hazards" has why that split is shaped
/// the way it is.
pub async fn connect_and_register(
    display_name: &str,
    params: SftpConnectionParams,
    attempt_id: &str,
) -> SftpConnection {
    let volume_id = cmdr_fs::volume::sftp_volume_id(&params.host, params.port, &params.username);
    let (cancel, _attempt) = register_attempt(attempt_id);
    let outcome = cmdr_sftp::connect_sftp_volume(
        display_name,
        &volume_id,
        params.clone(),
        crate::volume_host::host(),
        cancel,
    )
    .await;

    let volume = match outcome {
        Ok(SftpConnectOutcome::Connected(volume)) => volume,
        Ok(SftpConnectOutcome::NeedsHostKeyApproval(prompt)) => return SftpConnection::NeedsHostKeyApproval(prompt),
        Err(e) => return failed(e),
    };

    let rung = volume.auth_rung();
    register(&volume_id, volume).await;
    sftp_known_servers::remember(KnownSftpServer {
        host: params.host.clone(),
        port: params.port,
        username: params.username.clone(),
        display_name: display_name.to_string(),
        remote_root: params.remote_root.to_string_lossy().to_string(),
        key_file: params.key_file.as_ref().map(|p| p.to_string_lossy().to_string()),
        use_agent: params.use_agent,
        auto_reconnect: params.auto_reconnect,
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

/// Installs the volume under `volume_id`, retiring whoever held that id.
///
/// ❗ `on_superseded`, ❌ never `on_unmount`: a running transfer, an open viewer
/// stream, and the indexer all hold an `Arc` across a re-registration, and
/// tearing the session down would kill every one of them on a connection that is
/// perfectly healthy.
async fn register(volume_id: &str, volume: SftpVolume) {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let volume: Arc<dyn cmdr_fs::volume::Volume> = Arc::new(volume);
    // Asked BEFORE retiring anyone: a registry that keeps the incumbent would
    // otherwise leave the id pointing at a volume whose background work we just
    // stopped.
    let refused = manager.would_keep_incumbent(volume_id, volume.root());
    if !refused && let Some(previous) = manager.get(volume_id) {
        let _ = tokio::task::spawn_blocking(move || previous.on_superseded()).await;
    }
    manager.register(volume_id, volume);
    crate::volume_broadcast::emit_volumes_changed();
}

/// Moves the "reconnect automatically" switch on a volume that is already
/// mounted, answering whether there was an SFTP volume under that id.
///
/// ❗ The saved-server entry is the durable copy and the volume holds a live one,
/// so editing a server that happens to be open has to move both. Without this, a
/// switch the user flipped would take effect on the next connect and not before.
pub fn apply_auto_reconnect(volume_id: &str, on: bool) -> bool {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let Some(volume) = manager.get(volume_id) else {
        return false;
    };
    // Typed rather than a guess at the id's shape, the same way `disconnect` asks.
    let Some(sftp) = volume.as_any().downcast_ref::<SftpVolume>() else {
        return false;
    };
    sftp.set_auto_reconnect(on);
    true
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
