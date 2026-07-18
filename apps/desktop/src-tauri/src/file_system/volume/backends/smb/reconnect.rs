//! Reconnect state machine and watcher lifecycle: the in-place session
//! rebuild (`do_attempt_reconnect`), the credentialed variant, watcher
//! start/stop, and the backend-autonomous watcher-death reconnect loop.

use super::*;

impl SmbVolume {
    /// Cancels the existing watcher task (if any). The watcher exits on its
    /// next `select!` iteration. Best-effort: if the watcher already exited on
    /// a connection error, the send is a no-op.
    fn stop_watcher(&self) {
        if let Some(tx) = self.watcher_cancel.lock().ok().and_then(|mut g| g.take()) {
            let _ = tx.send(());
        }
    }

    /// Spawns the background watcher task on its own dedicated smb2 session.
    /// Replaces any prior `watcher_cancel`. Called from `connect_smb_volume`
    /// (initial setup) and from `attempt_reconnect` (after a session rebuild).
    ///
    /// We could share the volume's session with the watcher (smb2 0.10's
    /// `Watcher` is `'static`, owns a `Connection` clone), but in practice
    /// stacking the watcher's CHANGE_NOTIFY long-polls on the same TCP
    /// session as heavy concurrent writes wedges Samba — the
    /// `smb_integration_concurrent_streaming_writes_no_deadlock` test
    /// hangs against `smb-consumer-maxreadsize` (64 KB max read/write) when
    /// the watcher shares the connection. Keeping the watcher on its own
    /// TCP+session matches the pre-smb2-0.10 isolation; what we *do* keep
    /// from the new API is the pipelining (`Watcher` keeps one CHANGE_NOTIFY
    /// pre-issued, closing the response→re-arm loss window) and the lack
    /// of internal reconnect (single source of truth is
    /// `SmbVolume::attempt_reconnect`; the watcher bails on errors and we
    /// respawn here on the next successful reconnect).
    pub(super) fn spawn_watcher(&self, params: &SmbConnectionParams) {
        use crate::network::smb_connection::build_smb_addr;

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        let addr = build_smb_addr(&params.server, params.port);
        let share = params.share_name.clone();
        let username = params.username.clone();
        let password = params.password.clone();
        let volume_id = self.volume_id.clone();
        let mount_path = self.mount_path.clone();

        tokio::spawn(super::super::smb_watcher::run_smb_watcher(
            addr, share, username, password, volume_id, mount_path, cancel_rx,
        ));

        if let Ok(mut guard) = self.watcher_cancel.lock() {
            *guard = Some(cancel_tx);
        }
    }

    /// Inherent body for the trait's `attempt_reconnect`. Lives here as a regular
    /// async method so the body isn't hidden inside a `Pin<Box<...>>` future.
    ///
    /// Idempotent and single-flight:
    /// - If state is already `Direct`, returns Ok cheaply.
    /// - On auth failure, re-pulls credentials from the secret store (in case the user updated
    ///   them) and retries once before giving up.
    /// - On success: stores the new client + tree, restarts the watcher, emits
    ///   `smb-connection-changed { state: "direct" }`.
    /// - On failure: state stays `Disconnected`; the FE backoff cycle decides whether to retry.
    pub(super) async fn do_attempt_reconnect(&self) -> Result<(), VolumeError> {
        // Bail early if `on_unmount` already ran. Doing this before taking the
        // lock means a queued caller doesn't pay the lock-acquisition cost for
        // a volume that's about to be (or already is) gone.
        if self.unmounted.load(Ordering::Relaxed) {
            return Err(VolumeError::DeviceDisconnected(
                "SMB volume has been unmounted".to_string(),
            ));
        }

        // Single-flight: concurrent callers (FE cycle tick + lazy nav-time
        // retry) all wait here, and the second arrival sees state==Direct.
        let _guard = self.reconnect_lock.lock().await;

        // Re-check `unmounted`: between releasing the early check and acquiring
        // the lock, `on_unmount` may have run on another thread.
        if self.unmounted.load(Ordering::Relaxed) {
            return Err(VolumeError::DeviceDisconnected(
                "SMB volume has been unmounted".to_string(),
            ));
        }

        if self.connection_state() == ConnectionState::Direct {
            debug!(
                "SmbVolume::attempt_reconnect(share={}): already Direct, skipping",
                self.share_name
            );
            return Ok(());
        }

        // First try: stored credentials (the ones that worked at original connect).
        let params_snapshot = { self.params.read().await.clone() };
        info!(
            "SmbVolume::attempt_reconnect(share={}): trying with cached credentials",
            self.share_name
        );

        let first_attempt = build_session(&params_snapshot).await;
        let (client, tree) = match first_attempt {
            Ok(pair) => pair,
            Err(err) if crate::network::smb_util::is_auth_error(&err) => {
                // Cached creds may be stale. Re-pull from the secret store and retry once.
                info!(
                    "SmbVolume::attempt_reconnect(share={}): cached credentials rejected, re-pulling from secret store",
                    self.share_name
                );
                match refresh_credentials_from_store(&params_snapshot).await {
                    Some(refreshed)
                        if refreshed.username != params_snapshot.username
                            || refreshed.password != params_snapshot.password =>
                    {
                        match build_session(&refreshed).await {
                            Ok(pair) => {
                                // Refreshed creds worked; persist them on the volume.
                                let mut params_w = self.params.write().await;
                                params_w.username = refreshed.username.clone();
                                params_w.password = refreshed.password.clone();
                                pair
                            }
                            Err(e2) => {
                                warn!(
                                    "SmbVolume::attempt_reconnect(share={}): refreshed credentials also failed: {}",
                                    self.share_name, e2
                                );
                                // The password on the server changed and what we have
                                // saved no longer works. Tell the FE so it shows a
                                // "Sign in" prompt instead of the generic "unreachable".
                                emit_state_change(&self.volume_id, "needs_auth");
                                return Err(map_smb_error(e2));
                            }
                        }
                    }
                    _ => {
                        // No fresh creds available, or they're identical to the cached ones.
                        warn!(
                            "SmbVolume::attempt_reconnect(share={}): no fresh credentials available; giving up on this attempt",
                            self.share_name
                        );
                        emit_state_change(&self.volume_id, "needs_auth");
                        return Err(map_smb_error(err));
                    }
                }
            }
            Err(e) => {
                warn!(
                    "SmbVolume::attempt_reconnect(share={}): connect failed: {}",
                    self.share_name, e
                );
                return Err(map_smb_error(e));
            }
        };

        // The session-build round-trip can take several seconds. The user may
        // have unmounted the volume in the meantime. Discard the freshly-built
        // session and bail rather than installing it into an orphaned volume
        // (which would leak the watcher task and the smb2 connection).
        if self.unmounted.load(Ordering::Relaxed) {
            drop(client);
            drop(tree);
            return Err(VolumeError::DeviceDisconnected(
                "SMB volume was unmounted during reconnect".to_string(),
            ));
        }

        // Install the new session.
        {
            let mut tree_guard = self.tree.write().await;
            *tree_guard = Some(Arc::new(tree));
        }
        {
            let mut client_guard = self.client.lock().await;
            *client_guard = Some(client);
        }

        // Restart the watcher with current params (which may include refreshed creds).
        self.stop_watcher();
        let params_now = self.params.read().await.clone();
        self.spawn_watcher(&params_now);

        // Flip state and emit. Doing this last means an observer that wakes
        // up on the event will see a fully-installed session.
        self.transition_to_direct();

        // The session is back. Resume the drive index if the user had it enabled
        // (a persisted index DB with a completed scan). Fire-and-forget: the hook
        // spawns, so we never start the async indexer while holding `reconnect_lock`
        // (still held here). No-op for a never-enabled share or an already-active
        // index. This is the in-place-reconnect half of index recovery; the
        // launch/upgrade half lives in `smb_upgrade::register_smb_volume`.
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        crate::indexing::resume_smb_index_if_enabled(self.volume_id.clone());

        info!("SmbVolume::attempt_reconnect(share={}): success", self.share_name);
        Ok(())
    }

    /// Reconnect with freshly-entered credentials (the "Sign in" affordance after a
    /// `needs_auth` give-up). Persists the new password server-level — mirroring how the
    /// login form saves, so the NEXT reconnect finds it silently — updates the in-memory
    /// params, then runs the standard reconnect. If these credentials are also wrong,
    /// `do_attempt_reconnect` re-emits `needs_auth`, so a bad retry re-prompts rather than
    /// dead-ending.
    pub(super) async fn do_reconnect_with_credentials(
        &self,
        username: String,
        password: String,
    ) -> Result<(), VolumeError> {
        let server = { self.params.read().await.server.clone() };
        if let Err(e) = crate::network::keychain::save_credentials(&server, None, &username, &password) {
            // Non-fatal: the in-memory params below still carry the creds for this
            // reconnect; only the "silent next time" guarantee is lost.
            warn!(
                "SmbVolume::reconnect_with_credentials(share={}): saving credentials failed: {}",
                self.share_name, e
            );
        }
        {
            let mut params_w = self.params.write().await;
            params_w.username = username;
            params_w.password = password;
        }
        self.do_attempt_reconnect().await
    }
}

/// The bounded, growing backoff between backend reconnect attempts after the live
/// watcher's session died. A handful of tries over a few minutes, then we give up
/// quietly — never hammering a truly-down server. The frontend reconnect manager
/// runs its OWN cadence while a pane is open; this is the no-pane / background /
/// restart safety net, coalesced with the FE through `do_attempt_reconnect`'s
/// single-flight.
const WATCHER_DEATH_RECONNECT_BACKOFF: [Duration; 6] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
];

/// Whether a failed reconnect attempt is terminal for the backoff loop (stop) vs.
/// transient (keep backing off). An auth failure (`PermissionDenied`) is terminal:
/// `do_attempt_reconnect` already re-pulled credentials and emitted `needs_auth`,
/// so only the user's "Sign in" (the FE flow) can fix it, and retrying risks
/// locking the account. Everything else (network down, timeout, server rebooting)
/// is transient.
fn reconnect_backoff_should_give_up(err: &VolumeError) -> bool {
    matches!(err, VolumeError::PermissionDenied(_))
}

/// Drive backend-autonomous reconnection after the live SMB watcher's session
/// died. The single caller is the watcher's fatal-error exit
/// (`smb_watcher::run_smb_watcher`), which has already flipped the index Stale.
///
/// The watcher runs on its own dedicated smb2 session; that session erroring
/// proves the connection to the server broke, so we mark the volume Disconnected
/// (a background disconnect may not have touched the main session yet, leaving it
/// falsely Direct) and drive `do_attempt_reconnect` on a bounded, growing backoff.
/// Reusing `do_attempt_reconnect` (not a second reconnect path) means:
/// - it coalesces with any FE-initiated reconnect (single-flight `reconnect_lock`),
/// - success rebuilds the session, RESPAWNS the watcher, and resumes the drive
///   index (the on-connect hook) — all in one place, no second state machine.
///
/// Stops early when the volume is unmounted, removed/replaced in the manager, back
/// to Direct (an FE reconnect won the race), or an auth failure surfaced (the FE
/// "Sign in" flow owns that). Gives up quietly once the backoff is exhausted.
pub(crate) fn spawn_watcher_death_reconnect(volume_id: String) {
    tokio::spawn(async move {
        // The watcher's session died ⇒ the server connection is gone. Mark the
        // volume Disconnected so `do_attempt_reconnect` actually rebuilds (it
        // no-ops while Direct) and respawns the watcher.
        {
            let Some(volume) = crate::file_system::get_volume_manager().get(&volume_id) else {
                return; // already gone from the manager
            };
            let Some(smb) = volume.as_any().downcast_ref::<SmbVolume>() else {
                return; // replaced by a non-SMB volume
            };
            if smb.unmounted.load(Ordering::Relaxed) {
                return;
            }
            smb.transition_to_disconnected();
        }

        for (i, delay) in WATCHER_DEATH_RECONNECT_BACKOFF.iter().enumerate() {
            tokio::time::sleep(*delay).await;

            // Re-resolve each iteration: an unmount/replace swaps the instance.
            let Some(volume) = crate::file_system::get_volume_manager().get(&volume_id) else {
                return;
            };
            let Some(smb) = volume.as_any().downcast_ref::<SmbVolume>() else {
                return;
            };
            if smb.unmounted.load(Ordering::Relaxed) {
                return;
            }
            if smb.connection_state() == ConnectionState::Direct {
                debug!("smb backend reconnect: '{}' already Direct; done", volume_id);
                return; // an FE reconnect (or a prior attempt) won the race
            }

            match smb.do_attempt_reconnect().await {
                Ok(()) => {
                    info!(
                        "smb backend reconnect: '{}' back online after watcher death (attempt {}/{})",
                        volume_id,
                        i + 1,
                        WATCHER_DEATH_RECONNECT_BACKOFF.len()
                    );
                    return;
                }
                Err(e) if reconnect_backoff_should_give_up(&e) => {
                    info!(
                        "smb backend reconnect: '{}' needs credentials ({}); stopping — the Sign-in flow owns recovery",
                        volume_id, e
                    );
                    return;
                }
                Err(e) => {
                    debug!(
                        "smb backend reconnect: '{}' attempt {}/{} failed: {}",
                        volume_id,
                        i + 1,
                        WATCHER_DEATH_RECONNECT_BACKOFF.len(),
                        e
                    );
                }
            }
        }
        info!(
            "smb backend reconnect: '{}' still down after {} attempts; giving up (retries on next access or the next watcher death)",
            volume_id,
            WATCHER_DEATH_RECONNECT_BACKOFF.len()
        );
    });
}

#[cfg(test)]
mod reconnect_backoff_tests {
    use super::*;

    /// The watcher-death backoff must be bounded (a handful of attempts) and
    /// monotonically growing (never hammer a truly-down server), and finite so the
    /// loop always gives up. Guards against an accidental unbounded or shrinking
    /// schedule during edits.
    #[test]
    fn backoff_is_bounded_and_monotonic() {
        let schedule = WATCHER_DEATH_RECONNECT_BACKOFF;
        assert!(
            (3..=8).contains(&schedule.len()),
            "a handful of attempts, not an endless loop: got {}",
            schedule.len()
        );
        for pair in schedule.windows(2) {
            assert!(pair[1] >= pair[0], "backoff must never shrink: {:?}", schedule);
        }
        let total: Duration = schedule.iter().sum();
        assert!(
            total <= Duration::from_secs(600),
            "the loop must give up within a few minutes: total {:?}",
            total
        );
    }

    /// An auth failure is terminal for the backoff loop (the FE Sign-in flow owns
    /// recovery; retrying risks locking the account); every other failure is
    /// transient and keeps the loop backing off.
    #[test]
    fn only_auth_failure_stops_the_backoff() {
        assert!(reconnect_backoff_should_give_up(&VolumeError::PermissionDenied(
            "bad creds".into()
        )));
        assert!(!reconnect_backoff_should_give_up(&VolumeError::DeviceDisconnected(
            "server down".into()
        )));
        assert!(!reconnect_backoff_should_give_up(&VolumeError::ConnectionTimeout(
            "slow".into()
        )));
        assert!(!reconnect_backoff_should_give_up(&VolumeError::IoError {
            message: "blip".into(),
            raw_os_error: None,
        }));
    }
}
