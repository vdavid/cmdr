//! Connection/session management: session cloning (the [`SessionLease`]),
//! connection checks, the smb2 error-handling helper (`handle_smb_result`), and
//! the credential refresh. Building a session is `session_build.rs`.

use super::mapping::map_smb_error;
use super::state::{ConnectionState, RanOn, SessionGeneration};
use super::transfer_session::{Member, TransferHold};
use super::{SmbConnectionParams, SmbVolume};
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::StoredCredentials;
use log::{debug, warn};
use smb2::SmbClient;
use smb2::client::tree::Tree;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub(super) static CLIENT_LOCK_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The volume's main session: the client and the share's tree, which are only
/// ever installed, read, and dropped together.
///
/// A tree works only on the connection it was connected on (smb2 refuses any
/// other with `HandleInvalidated`), so holding them as one value is what keeps
/// a reconnect from ever handing out the old tree with the new connection.
pub(super) struct MainSession {
    pub(super) client: SmbClient,
    pub(super) tree: Arc<Tree>,
    /// Which installed session this is, handed out with every lease on it.
    pub(super) generation: SessionGeneration,
}

impl MainSession {
    pub(super) fn new(client: SmbClient, tree: Tree, generation: SessionGeneration) -> Self {
        Self {
            client,
            tree: Arc::new(tree),
            generation,
        }
    }
}

/// One clone of the main session for a call to drive with no lock held: the
/// share's tree, a `Connection` clone, and which installed session both belong
/// to.
///
/// ❗ An error from a call on these goes through the funnel with THIS
/// `generation` (`handle_smb_result`, `SmbVolumeInner::note_smb_error`), ❌ never
/// a fresh read of the current one: that's what keeps a late error from a
/// session a reconnect already replaced from taking down its successor.
pub(super) struct SessionLease {
    pub(super) tree: Arc<Tree>,
    pub(super) conn: smb2::client::Connection,
    pub(super) generation: SessionGeneration,
}

/// A lease for a byte-moving call ([`SmbVolume::transfer_lease`]): the tree, a
/// connection clone, and which session they belong to.
///
/// ❗ An error from a call on these goes through the funnel with
/// [`ran_on`](Self::ran_on), exactly as a [`SessionLease`]'s generation does.
/// Keep the lease alive for as long as the connection clone is in use: on the
/// transfer session it carries the hold that stops the idle close.
pub(super) struct TransferLease {
    pub(super) tree: Arc<Tree>,
    pub(super) conn: smb2::client::Connection,
    pub(super) ran_on: RanOn,
    pub(super) on: LeaseOn,
}

/// Which session a [`TransferLease`] came from, and what it needs to reach that
/// session's client (`dfs_links.rs`).
pub(super) enum LeaseOn {
    Main,
    Transfer { member: Arc<Member>, _hold: TransferHold },
}

impl SmbVolume {
    /// Checks that the connection is in `Direct` state. Returns
    /// `DeviceDisconnected` for `Disconnected`.
    fn check_connection(&self) -> Result<(), VolumeError> {
        match self.inner.connection_state() {
            ConnectionState::Direct => Ok(()),
            ConnectionState::Disconnected => {
                Err(VolumeError::DeviceDisconnected("SMB connection is disconnected".into()))
            }
        }
    }

    /// Briefly locks the session, clones its `Connection` (cheap `Arc::clone`;
    /// all clones multiplex frames over the same SMB session) and its
    /// `Arc<Tree>`, and releases the lock. Both come from the same session, and
    /// the lease names its generation.
    ///
    /// Callers can then drive `Tree::download` / `Tree::read_file_compound_sized` /
    /// `Tree::write_file_compound` on the owned `Connection` without holding
    /// any lock, enabling multiple concurrent copies on a single `SmbVolume`.
    ///
    /// ❗ A connection the server closed while IDLE (Samba's `deadtime`: a QNAP
    /// hangs up after 10 quiet minutes) is torn down with nobody waiting on it,
    /// so the share still reads `Direct`. This lease is where that's found: it
    /// dials a fresh session in place, silently, and leases that one
    /// (`SmbVolumeInner::revive_idle_closed`). Only a revival that fails marks
    /// the share lost, which starts the pane's reconnect cycle.
    pub(super) async fn clone_session(&self) -> Result<SessionLease, VolumeError> {
        let lease = self.lease_installed_session().await?;
        if !lease.conn.is_disconnected() {
            return Ok(self.measured(lease));
        }
        let closed = lease.generation;
        drop(lease);
        if let Err(e) = self.inner.revive_idle_closed(closed).await {
            self.inner.transition_to_disconnected(closed);
            return Err(e);
        }
        let lease = self.lease_installed_session().await?;
        Ok(self.measured(lease))
    }

    /// Notes the credit window off a fresh lease, before any transfer has: a
    /// small-window server's first batch is clamped too. Once a transfer session
    /// is up, ITS window is what copies spend, and only `transfer_lease`
    /// measures it.
    fn measured(&self, lease: SessionLease) -> SessionLease {
        if !self.inner.transfer.is_up() {
            self.note_credit_capacity(&lease.conn);
        }
        lease
    }

    /// [`clone_session`](Self::clone_session)'s lease of whatever session is
    /// installed, live or not.
    async fn lease_installed_session(&self) -> Result<SessionLease, VolumeError> {
        self.check_connection()?;
        let ticket = CLIENT_LOCK_TICKET.fetch_add(1, Ordering::Relaxed);
        let start = std::time::Instant::now();
        // TRACE: per-SMB-op mutex telemetry. At DEBUG it's the dominant scan-log source
        // (3 lines per listing). The stress-test `MutexCaptureLogger` still captures these
        // for hung-test triage (it sets max-level Trace). Real lock contention escalates
        // via `held_for`/`waited` at higher verbosity; bump with `RUST_LOG=…smb=trace`.
        log::trace!(
            "client-mutex: waiting ticket={} caller=clone_session share={:?}",
            ticket,
            LogPrivate::volume(&self.inner.share_name)
        );
        let lease = {
            let mut guard = self.inner.session.lock().await;
            log::trace!(
                "client-mutex: acquired ticket={} caller=clone_session share={:?} waited={:?}",
                ticket,
                LogPrivate::volume(&self.inner.share_name),
                start.elapsed()
            );
            let acquired_at = std::time::Instant::now();
            let session = guard.as_mut().ok_or_else(|| {
                log::trace!(
                    "client-mutex: released ticket={} caller=clone_session held_for={:?} (no-session-bail)",
                    ticket,
                    acquired_at.elapsed()
                );
                VolumeError::DeviceDisconnected("SMB session not available".into())
            })?;
            let lease = SessionLease {
                tree: Arc::clone(&session.tree),
                conn: session.client.connection_mut().clone(),
                generation: session.generation,
            };
            log::trace!(
                "client-mutex: released ticket={} caller=clone_session held_for={:?}",
                ticket,
                acquired_at.elapsed()
            );
            lease
        };
        Ok(lease)
    }

    /// Refresh what the credit window can carry, while a `Connection` is in
    /// hand: `Volume::max_concurrent_ops` is sync and can't reach one. See
    /// `REPRESENTATIVE_COPY_REQUEST_BYTES`.
    fn note_credit_capacity(&self, conn: &smb2::client::Connection) {
        self.inner.credit_copy_capacity.store(
            conn.credit_capacity_for(super::REPRESENTATIVE_COPY_REQUEST_BYTES),
            Ordering::Relaxed,
        );
    }

    /// The connection a transfer rides right now, for the watchdog's liveness
    /// and receive-rate readings: the transfer session's while one is up, else
    /// the main session's, which is where [`transfer_lease`](Self::transfer_lease)
    /// falls back to.
    pub(super) fn transfer_connection(&self) -> &super::liveness::LiveConnection {
        if self.inner.transfer.is_up() {
            &self.inner.transfer.live
        } else {
            &self.inner.live_connection
        }
    }

    /// A lease for a BYTE-MOVING call: on the share's transfer session
    /// (`transfer_session.rs`), opening it first if nothing is open, or on the
    /// main session while the transfer session is down.
    ///
    /// The share must read `Direct` either way: the main session is the one
    /// answer to "is the share up", which a transfer's outage wait asks.
    pub(super) async fn transfer_lease(&self) -> Result<TransferLease, VolumeError> {
        self.check_connection()?;
        if !self.inner.unmounted.load(Ordering::Relaxed)
            && let Some(leased) = self.inner.transfer.lease().await
        {
            self.note_credit_capacity(&leased.conn);
            return Ok(TransferLease {
                tree: Arc::clone(&leased.member.tree),
                conn: leased.conn,
                ran_on: RanOn::Transfer(leased.generation),
                on: LeaseOn::Transfer {
                    member: leased.member,
                    _hold: leased.hold,
                },
            });
        }
        let SessionLease { tree, conn, generation } = self.clone_session().await?;
        self.note_credit_capacity(&conn);
        Ok(TransferLease {
            tree,
            conn,
            ran_on: RanOn::Main(generation),
            on: LeaseOn::Main,
        })
    }

    /// Maps an smb2 result, handling connection state transitions on error.
    ///
    /// `smb_path` is the share-relative path the operation ran on; it becomes the
    /// PAYLOAD of the path-carrying `VolumeError` variants (as its display form),
    /// which is what the frontend renders as the file's name. The server's own
    /// NTSTATUS wording goes to the log line here, never into the payload.
    ///
    /// `ran_on` is the generation of the lease the call ran on (see
    /// [`SessionLease`]); the session-state half is `SmbVolumeInner::note_smb_error`.
    pub(super) fn handle_smb_result<T>(
        &self,
        op_name: &str,
        smb_path: &str,
        ran_on: impl Into<RanOn>,
        result: Result<T, smb2::Error>,
    ) -> Result<T, VolumeError> {
        let e = match result {
            Ok(val) => return Ok(val),
            Err(e) => e,
        };
        let kind = e.kind();
        let ran_on = ran_on.into();
        if self.inner.note_error(ran_on, &e) {
            let lost = match ran_on {
                RanOn::Main(_) => "the session is lost",
                RanOn::Transfer(_) => "the transfer session is lost, the share stays up",
            };
            // A refused signature lands here too: it leaves the signed session in a
            // state nothing can vouch for, so whatever runs next (the pane's
            // reconnect, the notice's Reconnect) builds a fresh, freshly signed one.
            warn!(
                "SmbVolume::{}(share={:?}): source=backend, backend=smb2, error_kind={:?}, nt_status={:?}, detail={:?}; {}",
                op_name, // allowed-log-private: the volume method's name, not a user value
                LogPrivate::volume(&self.inner.share_name),
                kind,
                e.status(),
                cmdr_fs::log_detail::LogDetail(&e.to_string()).marking(&[LogPrivate::path(smb_path)]),
                lost
            );
        } else if matches!(
            kind,
            smb2::ErrorKind::NotFound | smb2::ErrorKind::IsADirectory | smb2::ErrorKind::AlreadyExists
        ) {
            // Expected fall-through cases: the caller is using the typed
            // `VolumeError` variant as a signal, not an error:
            // - `NotFound` for existence checks (rename dest, conflict detection)
            // - `IsADirectory` for `delete()`'s "try delete_file first, fall back to delete_directory"
            //   fast-path
            // - `AlreadyExists` for `copy_directory_streaming`'s "create_directory is idempotent for merge"
            //   path
            debug!(
                "SmbVolume::{}(share={:?}): backend=smb2, error_kind={:?}, nt_status={:?}, detail={:?}",
                op_name, // allowed-log-private: the volume method's name, not a user value
                LogPrivate::volume(&self.inner.share_name),
                kind,
                e.status(),
                cmdr_fs::log_detail::LogDetail(&e.to_string()).marking(&[LogPrivate::path(smb_path)])
            );
        } else {
            warn!(
                "SmbVolume::{}(share={:?}): backend=smb2, error_kind={:?}, nt_status={:?}, detail={:?}",
                op_name, // allowed-log-private: the volume method's name, not a user value
                LogPrivate::volume(&self.inner.share_name),
                kind,
                e.status(),
                cmdr_fs::log_detail::LogDetail(&e.to_string()).marking(&[LogPrivate::path(smb_path)])
            );
        }

        Err(map_smb_error(e, &self.to_display_path(smb_path)))
    }
}

/// Re-fetches credentials from the secret store for the given server/share.
/// Returns `None` if nothing is stored (in which case the cached creds are all
/// we have to work with).
///
/// Narrow first, then wide: a share-level entry beats the server-level one a
/// sign-in saves, so a share with its own password keeps it.
pub(super) async fn refresh_credentials_from_store(
    host: &VolumeHost,
    params: &SmbConnectionParams,
) -> Option<SmbConnectionParams> {
    let server = params.server.clone();
    let share = params.share_name.clone();
    let host = host.clone();

    // The store is synchronous down to the OS call, so it stays off the async
    // worker it would otherwise block.
    let creds = tokio::task::spawn_blocking(move || {
        host.credentials()
            .credentials(&server, Some(&share))
            .or_else(|| host.credentials().credentials(&server, None))
    })
    .await
    .ok()
    .flatten()?;

    Some(SmbConnectionParams {
        server: params.server.clone(),
        share_name: params.share_name.clone(),
        port: params.port,
        username: creds.username,
        password: creds.secret,
    })
}

/// After a sign-in WORKED, files the credentials that connected the way its
/// Remember box asks (#530).
///
/// - **Remember on**: an empty store gets the SERVER's entry (a checked box is a
///   promise), the one every sign-in sheet writes and "Forget saved password"
///   takes back.
/// - **Remember off**: refreshes an entry that's there, ❌ never creates one. No
///   entry means the person turned Remember off (the sheet forgets before it
///   dials), and writing one anyway put back the password they had just told
///   Cmdr to forget.
///
/// The entry is the one [`refresh_credentials_from_store`] reads: the share's
/// own, else the server's. ❗ A server entry holding ANOTHER account is left
/// alone: every other share on the server signs in with that one, so Remember
/// on files this share its own instead.
///
/// The reads and the write ride ONE blocking task, so a store that prompts asks
/// once. Answers whether the store took it; `false` costs only "silent next
/// time", ❌ never the session that just came up.
pub(super) async fn refresh_remembered_credentials(
    host: &VolumeHost,
    connected: &SmbConnectionParams,
    remember: bool,
) -> bool {
    let host = host.clone();
    let server = connected.server.clone();
    let share = connected.share_name.clone();
    let stored = StoredCredentials {
        username: connected.username.clone(),
        secret: connected.password.clone(),
    };
    let written = tokio::task::spawn_blocking(move || {
        let store = host.credentials();
        let scope = if store.credentials(&server, Some(&share)).is_some() {
            Some(share.as_str())
        } else {
            match store.credentials(&server, None) {
                Some(wide) if wide.username == stored.username => None,
                None if remember => None,
                Some(_) if remember => Some(share.as_str()),
                None | Some(_) => return Ok(()),
            }
        };
        store.save_credentials(&server, scope, &stored)
    })
    .await;
    matches!(written, Ok(Ok(())))
}

/// Tells the host about a tampering signal smb2 refused, for the app's security record: a reply
/// refused for its signature, or a negotiation the server didn't confirm (#931). The error funnel
/// (`SmbVolumeInner::note_smb_error`) and a failed reconnect call it for every smb2 error; the
/// host coalesces repeats. Read by kind, ❌ never the message.
pub(super) fn report_security_refusal(host: &VolumeHost, volume_id: &str, err: &smb2::Error) {
    use cmdr_fs::volume::host::events::SecurityRefusal;
    let refusal = match err.kind() {
        smb2::ErrorKind::SignatureInvalid => SecurityRefusal::ReplySignatureInvalid,
        smb2::ErrorKind::NegotiateValidationFailed => SecurityRefusal::NegotiationInvalid,
        _ => return,
    };
    host.events().security_refusal(volume_id, refusal);
}
