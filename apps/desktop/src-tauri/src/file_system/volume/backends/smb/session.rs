//! Connection/session management: session cloning, connection checks, and the
//! smb2 error-handling helpers (`handle_smb_result`, `with_smb_sync`,
//! `update_state_on_smb_error`) plus session (re)build helpers.

use super::*;

pub(super) static CLIENT_LOCK_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl SmbVolume {
    /// Checks that the connection is in `Direct` state. Returns
    /// `DeviceDisconnected` for `Disconnected`.
    fn check_connection(&self) -> Result<(), VolumeError> {
        match self.connection_state() {
            ConnectionState::Direct => Ok(()),
            ConnectionState::Disconnected => Err(VolumeError::DeviceDisconnected(
                "SMB connection is disconnected".to_string(),
            )),
        }
    }

    /// Reads out a clone of `Arc<Tree>`. Cheap (`Arc::clone`).
    pub(super) async fn tree_arc(&self) -> Result<Arc<Tree>, VolumeError> {
        self.check_connection()?;
        let guard = self.tree.read().await;
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
            self.share_name
        );
        let conn = {
            let mut guard = self.client.lock().await;
            log::trace!(
                "client-mutex: acquired ticket={} caller=clone_session share={} waited={:?}",
                ticket,
                self.share_name,
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
    pub(super) fn handle_smb_result<T>(&self, op_name: &str, result: Result<T, smb2::Error>) -> Result<T, VolumeError> {
        match result {
            Ok(val) => Ok(val),
            Err(e) => {
                let kind = e.kind();

                // On connection loss, transition to Disconnected
                if matches!(kind, smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired) {
                    warn!(
                        "SmbVolume::{}(share={}): connection lost ({}), transitioning to Disconnected",
                        op_name, self.share_name, e
                    );
                    self.transition_to_disconnected();
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
                    debug!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                } else {
                    warn!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                }

                Err(map_smb_error(e))
            }
        }
    }

    /// Runs an smb2 operation synchronously. For sync-only contexts (`on_unmount`, tests).
    ///
    /// This exists only for contexts where
    /// no async runtime is available (for example, cleanup paths called from drop-like code).
    fn with_smb_sync<F, T>(&self, op_name: &str, f: F) -> Result<T, VolumeError>
    where
        F: FnOnce(&mut SmbClient, &Tree) -> Result<T, smb2::Error>,
    {
        if self.connection_state() == ConnectionState::Disconnected {
            return Err(VolumeError::DeviceDisconnected(
                "SMB connection is disconnected".to_string(),
            ));
        }

        let tree_arc = {
            let guard = self.tree.blocking_read();
            guard
                .as_ref()
                .cloned()
                .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))?
        };

        let mut guard = self.client.blocking_lock();

        let client = guard
            .as_mut()
            .ok_or_else(|| VolumeError::DeviceDisconnected("SMB session not available".to_string()))?;

        match f(client, &tree_arc) {
            Ok(val) => Ok(val),
            Err(e) => {
                let kind = e.kind();

                if matches!(kind, smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired) {
                    warn!(
                        "SmbVolume::{}(share={}): connection lost ({}), transitioning to Disconnected",
                        op_name, self.share_name, e
                    );
                    self.transition_to_disconnected();
                } else if matches!(kind, smb2::ErrorKind::NotFound) {
                    debug!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                } else {
                    warn!("SmbVolume::{}(share={}): {}", op_name, self.share_name, e);
                }

                Err(map_smb_error(e))
            }
        }
    }
}

/// Builds a fresh smb2 session using the given params. Returns the connected
/// client + tree on success.
pub(super) async fn build_session(params: &SmbConnectionParams) -> Result<(SmbClient, Tree), smb2::Error> {
    use crate::network::smb_connection::build_smb_addr;

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
pub(super) async fn refresh_credentials_from_store(params: &SmbConnectionParams) -> Option<SmbConnectionParams> {
    let server = params.server.clone();
    let share = params.share_name.clone();

    let creds = tokio::task::spawn_blocking(move || {
        // Try share-level first (more specific), then server-level.
        crate::network::keychain::get_credentials(&server, Some(&share))
            .or_else(|_| crate::network::keychain::get_credentials(&server, None))
            .ok()
    })
    .await
    .ok()
    .flatten()?;

    Some(SmbConnectionParams {
        server: params.server.clone(),
        share_name: params.share_name.clone(),
        port: params.port,
        username: creds.username,
        password: creds.password,
    })
}

/// If an smb2 error indicates the session is dead, transition state to
/// `Disconnected` and emit `smb-connection-changed`. Mirrors `handle_smb_result`
/// for contexts without `&self` (the streaming-read producer task).
pub(super) fn update_state_on_smb_error(state: &AtomicU8, volume_id: &str, err: &smb2::Error) {
    if matches!(
        err.kind(),
        smb2::ErrorKind::ConnectionLost | smb2::ErrorKind::SessionExpired
    ) {
        let prev = state.swap(ConnectionState::Disconnected as u8, Ordering::Relaxed);
        if prev != ConnectionState::Disconnected as u8 {
            emit_state_change(volume_id, "disconnected");
        }
    }
}
