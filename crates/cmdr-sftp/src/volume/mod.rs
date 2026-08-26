//! The SFTP backend: a `Volume` over one SSH connection with one SFTP channel on
//! it.
//!
//! There is no OS mount under this and never will be: every listing, read, and
//! write rides the channel. The volume's root is the remote directory it was
//! opened at, so the paths it hands out ARE remote paths and nothing has to be
//! translated between two spellings of the same tree.
//!
//! Nothing here names the application. What the backend needs from it arrives
//! through the [`VolumeHost`] seams handed to [`connect_sftp_volume`].
//! `CLAUDE.md` has the must-knows, `DETAILS.md` the decisions.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Weak};

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::settings::BackendName;
use cmdr_fs::volume::{Retirement, VolumeError};
use tokio_util::sync::CancellationToken;

use crate::auth::{AuthRungUsed, UnattendedReconnect};
use crate::errors::SftpConnectError;
use crate::params::SftpConnectionParams;
use crate::transport::{self, DialOutcome, HostKeyPrompt, HostKeyPromptKind, SshConnection};

mod copy;
mod mapping;
mod mutation;
mod paths;
mod query;
mod reconnect;
mod scan;
mod state;
mod streams;
mod volume_impl;
mod writes;

pub use state::ConnectionState;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// This backend's settings namespace, for everything it reads through
/// [`VolumeHost::settings`]. A namespace, not a classification: nothing branches
/// on it, and the app resolves it through a table.
const BACKEND: BackendName = "sftp";

/// What opening a volume produced.
///
/// ❗ A typed outcome rather than an error, because "the key needs approving" is
/// an ordinary first-connection step and not a failure. ❌ Never a stringly
/// typed result.
pub enum SftpConnectOutcome {
    /// A live volume.
    Connected(SftpVolume),
    /// The server's host key needs a human. **No session is held across the
    /// prompt**: this dial has already been dropped, and approval is followed by
    /// a fresh one.
    NeedsHostKeyApproval(HostKeyPrompt),
}

/// A volume backed by an SFTP server.
pub struct SftpVolume {
    /// Display name, as the app chose to label the server.
    name: String,
    /// The remote directory this volume is rooted at. Immutable: a different
    /// root is a different instance, so `root()` stays a plain borrow.
    root: PathBuf,
    inner: Arc<SftpVolumeInner>,
}

/// The connection-scoped half: what every instance of this volume shares.
struct SftpVolumeInner {
    /// From `sftp_volume_id(host, port, username)`, the key every piece of
    /// durable per-volume state is filed under.
    volume_id: String,
    /// How to reach the server, and how to reach it again.
    params: SftpConnectionParams,
    /// Which rung built the LIVE session, which is what decides whether a
    /// dropped one may rebuild itself (`crate::auth::reconnect_policy`).
    ///
    /// Behind a lock because a reconnect may land on a different rung than the
    /// first dial did: an agent identity can be added or removed between them.
    rung: std::sync::Mutex<AuthRungUsed>,
    /// The live session. `None` once disconnected, at which point every
    /// operation fails fast rather than hanging.
    ///
    /// An `Arc` inside so a caller clones the transport out from under a short
    /// read guard and drives its operation without holding the lock: the SFTP
    /// engine multiplexes on one channel, so N operations genuinely overlap.
    session: tokio::sync::RwLock<Option<Arc<SshConnection>>>,
    /// The state the host was last told about, so a server that is down doesn't
    /// produce one event per failing operation (`state.rs`).
    state: AtomicU8,
    /// Whether the registry still serves this volume under its id. Read back by
    /// the reconnect loop through a `SelfHandle`.
    retirement: Retirement,
    /// This state's own weak reference, for the background work that outlives the
    /// call that started it. Set by `Arc::new_cyclic`.
    me: Weak<SftpVolumeInner>,
    /// Single-flight around a session rebuild: concurrent callers (the frontend's
    /// backoff tick and the backend's own loop) wait here and the second one
    /// finds a live session.
    reconnect_lock: tokio::sync::Mutex<()>,
    /// Set by `on_unmount`. A reconnect in flight bails rather than installing a
    /// session into a volume the app has forgotten.
    unmounted: AtomicBool,
    /// Whether the user's per-server "reconnect automatically" switch is on.
    ///
    /// ❗ The FIRST gate on every unattended dial, ahead of the rung, and ❌
    /// independent of whether a secret is remembered. Live rather than read from
    /// `params`, so flipping the switch takes effect without a remount.
    auto_reconnect: AtomicBool,
    /// Whether the one unattended authentication attempt has been spent
    /// (`reconnect.rs`). ❌ Never a loop: repeated refusals lock accounts.
    auth_attempt_spent: AtomicBool,
    /// Everything this backend asks the app around it.
    host: VolumeHost,
}

impl SftpVolume {
    /// The volume id every listing-cache lookup and connection event uses.
    pub fn volume_id(&self) -> &str {
        &self.inner.volume_id
    }

    /// Which credential built the live session.
    pub fn auth_rung(&self) -> AuthRungUsed {
        self.inner.auth_rung()
    }

    /// Moves the user's "reconnect automatically" switch, on a volume that is
    /// already mounted.
    ///
    /// ❗ Its meaning is exactly "may Cmdr redial unattended when the session
    /// drops", and ❌ nothing else: it never stores a secret, never forgets one,
    /// and never changes what a person can do by hand.
    ///
    /// ❗ Switching it ON while the volume sits `Disconnected` starts the backoff
    /// loop then and there. Without that, the switch would look broken in exactly
    /// the moment a user flips it to fix a volume that is already down.
    pub fn set_auto_reconnect(&self, on: bool) {
        let was = self.inner.auto_reconnect.swap(on, Ordering::Relaxed);
        if on && !was {
            self.inner.start_reconnect_loop_if_down();
        }
    }

    /// Whether an unattended reconnect can actually happen as this volume stands.
    ///
    /// ❗ The backend's answer to "the switch is on but nothing comes back", so
    /// ❌ no frontend has to derive it from a rung and a `has_credentials` call.
    /// Reads the secret store only for the rungs that redial out of it, because a
    /// needless read is a needless Keychain prompt.
    pub async fn unattended_reconnect(&self) -> UnattendedReconnect {
        self.inner.unattended_reconnect().await
    }

    /// The live transport, cloned out from under a short read guard.
    ///
    /// ❗ Clone and release. Holding the guard across an operation would
    /// serialize every other operation behind it, which is exactly the
    /// concurrency the one channel exists to provide.
    pub(super) async fn clone_session(&self) -> Result<Arc<SshConnection>, VolumeError> {
        self.inner
            .session
            .read()
            .await
            .clone()
            .ok_or_else(|| VolumeError::DeviceDisconnected(self.inner.volume_id.clone()))
    }

    /// Drops the live session.
    ///
    /// ❗ Dropping IS the clean shutdown, and ❌ there is deliberately no
    /// `Sftp::close()` call anywhere in this crate: it awaits a read task that
    /// only ends at reader EOF, which a `russh` channel never reaches, so it
    /// hangs forever. `transport.rs` has the full note.
    pub async fn disconnect(&self) {
        self.inner.unmounted.store(true, Ordering::Relaxed);
        self.inner.mark_gone_silently();
        self.inner.session.write().await.take();
    }

    /// What the server said it can do, for the cells that assert on the probe
    /// itself rather than on a path that branches on it.
    #[cfg(any(test, feature = "testing"))]
    pub async fn server_extensions(&self) -> Result<crate::extensions::ServerExtensions, VolumeError> {
        Ok(self.clone_session().await?.extensions())
    }

    /// Drops the live session the way a server going away would, ❗ WITHOUT
    /// starting the backoff loop, so a cell drives the recovery itself and its
    /// timing is its own.
    ///
    /// The state stays `Connected` because that is what it is until something
    /// notices: this reproduces the moment before an operation fails, which is
    /// exactly where a reconnect cell wants to begin.
    #[cfg(any(test, feature = "testing"))]
    pub async fn simulate_session_loss(&self) {
        self.inner.session.write().await.take();
    }
}

/// Opens an SFTP volume, or reports that its host key needs approving.
///
/// `volume_id` must be the one the caller registers the volume under; production
/// callers derive it from `sftp_volume_id(host, port, username)`.
///
/// Cancelling `cancel` ends the attempt where it stands and leaves ❗ nothing
/// behind: no volume to register, no approval recorded, no secret written. The
/// lever each phase uses, the SFTP hello's included: `transport.rs`
/// § "Cancelling a connect".
///
/// ❗ The dial runs inside a task on `host.runtime()` and this function awaits
/// the join handle, so dropping THIS future abandons the handle and never the
/// dial. `transport.rs` has the detail, including why that indirection outlived
/// the `openssh-sftp-client` panic that forced it.
pub async fn connect_sftp_volume(
    name: &str,
    volume_id: &str,
    params: SftpConnectionParams,
    host: VolumeHost,
    cancel: CancellationToken,
) -> Result<SftpConnectOutcome, SftpConnectError> {
    let root = params.remote_root.clone();
    let auto_reconnect = params.auto_reconnect;
    let outcome = reconnect::guarded_dial(&host, params.clone(), None, cancel.clone()).await?;

    // ❗ A cancel that lands as the session does still ends the user's connect.
    // Answering here is what makes "cancelled" and "registered" mutually
    // exclusive: the caller never gets a volume it has to clean up, and the
    // session inside `outcome` goes away with it.
    if cancel.is_cancelled() {
        return Err(SftpConnectError::Cancelled);
    }

    match outcome {
        DialOutcome::NeedsHostKeyApproval(prompt) => Ok(SftpConnectOutcome::NeedsHostKeyApproval(prompt)),
        DialOutcome::Connected { connection, rung } => {
            // PII-free: an SFTP session came up. ❌ No host, account, port, or
            // path ever crosses, hashed or otherwise.
            host.analytics().record("sftp_connected", &[]);
            Ok(SftpConnectOutcome::Connected(SftpVolume {
                name: name.to_string(),
                root,
                inner: Arc::new_cyclic(|me| SftpVolumeInner {
                    volume_id: volume_id.to_string(),
                    params,
                    rung: std::sync::Mutex::new(rung),
                    session: tokio::sync::RwLock::new(Some(Arc::new(connection))),
                    state: AtomicU8::new(ConnectionState::Connected as u8),
                    retirement: Retirement::new(),
                    me: me.clone(),
                    reconnect_lock: tokio::sync::Mutex::new(()),
                    unmounted: AtomicBool::new(false),
                    auto_reconnect: AtomicBool::new(auto_reconnect),
                    auth_attempt_spent: AtomicBool::new(false),
                    host,
                }),
            }))
        }
    }
}

/// What an approval attempt produced.
#[derive(Debug)]
pub enum HostKeyApproval {
    /// The fingerprint is now the trusted key for this server, and the next dial
    /// walks past the prompt.
    Recorded,
    /// ❗ Nothing was written. The server presents a different key than the one
    /// approved, so this carries what it presents NOW and the caller starts the
    /// approval over on that.
    Superseded(HostKeyPrompt),
}

/// Records a host key a human approved, ❗ only if the server still presents it.
///
/// The second half of the two-phase flow, and the half that carries its
/// security. Time passes between a prompt being shown and a person clicking it,
/// and the approval names ONE key: writing whatever was clicked without asking
/// the server again lets an approval be replayed against a key the user never
/// read. So the fingerprint on its way in has to be the fingerprint on the wire,
/// or nothing is written at all.
///
/// ❗ The re-check is a key exchange and nothing more
/// ([`transport::presented_host_key`]): no credential is offered, so an approval
/// can never spend an authentication attempt. Afterwards the caller dials
/// afresh — ❌ no session is held across the prompt.
pub async fn approve_host_key(
    host: &VolumeHost,
    server: &str,
    port: u16,
    algorithm: &str,
    fingerprint: &str,
) -> Result<HostKeyApproval, SftpConnectError> {
    let presented = transport::presented_host_key(server, port, host).await?;
    if presented.algorithm != algorithm || presented.fingerprint != fingerprint {
        log::warn!(
            target: "volume",
            "an sftp host-key approval named a key {server}:{port} no longer presents; recording nothing"
        );
        return Ok(HostKeyApproval::Superseded(HostKeyPrompt {
            host: server.to_string(),
            port,
            algorithm: presented.algorithm,
            fingerprint: presented.fingerprint,
            // Something IS stored for this server if the approval was for a key
            // we already held; either way the user is looking at a second,
            // different key, which is the alarm rather than the routine path.
            kind: HostKeyPromptKind::Changed,
        }));
    }
    transport::approve(host, server, port, algorithm, fingerprint);
    Ok(HostKeyApproval::Recorded)
}

// The suites asserting on this backend's own behavior. White-box by nature: they
// build a volume with no session behind it and drive the path translation and the
// query surface directly.
#[cfg(test)]
mod cancel_test;
#[cfg(test)]
mod conformance_test;
#[cfg(test)]
mod host_seam_test;
#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod scan_test;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod write_path_test;
