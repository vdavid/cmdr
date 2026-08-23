//! Connection/session management: session cloning, connection checks, and the
//! smb2 error-handling helpers (`handle_smb_result`,
//! `update_state_on_smb_error`) plus session (re)build helpers.

use super::mapping::map_smb_error;
use super::state::ConnectionState;
use super::{SmbConnectionParams, SmbVolume};
use cmdr_fs::volume::Retirement;
use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::host::VolumeHost;
use log::{debug, warn};
use smb2::client::tree::Tree;
use smb2::{ClientConfig, SmbClient};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

pub(super) static CLIENT_LOCK_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl SmbVolume {
    /// Checks that the connection is in `Direct` state. Returns
    /// `DeviceDisconnected` for `Disconnected`.
    fn check_connection(&self) -> Result<(), VolumeError> {
        match self.inner.connection_state() {
            ConnectionState::Direct => Ok(()),
            ConnectionState::Disconnected => Err(VolumeError::DeviceDisconnected(
                "SMB connection is disconnected".to_string(),
            )),
        }
    }

    /// Reads out a clone of `Arc<Tree>`. Cheap (`Arc::clone`).
    pub(super) async fn tree_arc(&self) -> Result<Arc<Tree>, VolumeError> {
        self.check_connection()?;
        let guard = self.inner.tree.read().await;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))
    }

    /// Briefly locks the client mutex, clones its `Connection` (cheap
    /// `Arc::clone`; all clones multiplex frames over the same SMB session),
    /// and releases the lock. Also reads out an `Arc<Tree>`. Returns both.
    ///
    /// Callers can then drive `Tree::download` / `Tree::read_file_compound` /
    /// `Tree::write_file_compound` on the owned `Connection` without holding
    /// any lock, enabling multiple concurrent copies on a single `SmbVolume`.
    pub(super) async fn clone_session(&self) -> Result<(Arc<Tree>, smb2::client::Connection), VolumeError> {
        self.check_connection()?;
        let tree = self.tree_arc().await?;
        let ticket = CLIENT_LOCK_TICKET.fetch_add(1, Ordering::Relaxed);
        let start = std::time::Instant::now();
        // TRACE: per-SMB-op mutex telemetry. At DEBUG it's the dominant scan-log source
        // (3 lines per listing). The stress-test `MutexCaptureLogger` still captures these
        // for hung-test triage (it sets max-level Trace). Real lock contention escalates
        // via `held_for`/`waited` at higher verbosity; bump with `RUST_LOG=…smb=trace`.
        log::trace!(
            "client-mutex: waiting ticket={} caller=clone_session share={}",
            ticket,
            self.inner.share_name
        );
        let conn = {
            let mut guard = self.inner.client.lock().await;
            log::trace!(
                "client-mutex: acquired ticket={} caller=clone_session share={} waited={:?}",
                ticket,
                self.inner.share_name,
                start.elapsed()
            );
            let acquired_at = std::time::Instant::now();
            let client = guard.as_mut().ok_or_else(|| {
                log::trace!(
                    "client-mutex: released ticket={} caller=clone_session held_for={:?} (no-session-bail)",
                    ticket,
                    acquired_at.elapsed()
                );
                VolumeError::DeviceDisconnected("SMB session not available".to_string())
            })?;
            let c = client.connection_mut().clone();
            log::trace!(
                "client-mutex: released ticket={} caller=clone_session held_for={:?}",
                ticket,
                acquired_at.elapsed()
            );
            c
        };
        Ok((tree, conn))
    }

    /// Maps an smb2 result, handling connection state transitions on error.
    ///
    /// `smb_path` is the share-relative path the operation ran on; it becomes the
    /// PAYLOAD of the path-carrying `VolumeError` variants (as its display form),
    /// which is what the frontend renders as the file's name. The server's own
    /// NTSTATUS wording goes to the log line here, never into the payload.
    pub(super) fn handle_smb_result<T>(
        &self,
        op_name: &str,
        smb_path: &str,
        result: Result<T, smb2::Error>,
    ) -> Result<T, VolumeError> {
        match result {
            Ok(val) => Ok(val),
            Err(e) => {
                let kind = e.kind();

                // On connection loss, transition to Disconnected
                if matches!(kind, smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired) {
                    warn!(
                        "SmbVolume::{}(share={}): connection lost ({}), transitioning to Disconnected",
                        op_name, self.inner.share_name, e
                    );
                    self.inner.transition_to_disconnected();
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
                    debug!("SmbVolume::{}(share={}): {}", op_name, self.inner.share_name, e);
                } else {
                    warn!("SmbVolume::{}(share={}): {}", op_name, self.inner.share_name, e);
                }

                Err(map_smb_error(e, &self.to_display_path(smb_path)))
            }
        }
    }
}

/// Builds a fresh smb2 session using the given params. Returns the connected
/// client + tree on success.
pub(super) async fn build_session(params: &SmbConnectionParams) -> Result<(SmbClient, Tree), smb2::Error> {
    use crate::build_smb_addr;

    let config = ClientConfig {
        addr: build_smb_addr(&params.server, params.port),
        timeout: Duration::from_secs(10),
        username: params.username.clone(),
        password: params.password.clone(),
        domain: String::new(),
        auto_reconnect: false,
        compression: true,
        dfs_enabled: false,
        dfs_target_overrides: Default::default(),
    };
    let mut client = SmbClient::connect(config).await?;
    let tree = client.connect_share(&params.share_name).await?;
    Ok((client, tree))
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

/// If an smb2 error indicates the session is dead, transition state to
/// `Disconnected` and emit `volume-connection-changed`. Mirrors `handle_smb_result`
/// for contexts without `&self` (the streaming-read producer task).
///
/// `retirement` is the share's flag: a retired share still tracks its own state
/// for the holders reading through it, but must not announce a disconnect under a
/// volume id somebody else now owns.
pub(super) fn update_state_on_smb_error(
    host: &VolumeHost,
    state: &AtomicU8,
    retirement: &Retirement,
    volume_id: &str,
    err: &smb2::Error,
) {
    if matches!(
        err.kind(),
        smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired
    ) {
        let prev = state.swap(ConnectionState::Disconnected as u8, Ordering::Relaxed);
        if prev != ConnectionState::Disconnected as u8 && !retirement.is_retired() {
            host.events()
                .connection_changed(volume_id, ConnectionState::Disconnected.into());
        }
    }
}
