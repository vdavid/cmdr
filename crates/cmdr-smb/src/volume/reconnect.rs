//! Reconnect state machine and watcher lifecycle: the in-place session
//! rebuild (`do_attempt_reconnect`), the credentialed variant, watcher
//! start/stop, and the backend-autonomous watcher-death reconnect loop.

use super::mapping::map_smb_error;
use super::session::{build_session, refresh_credentials_from_store};
use super::state::ConnectionState;
use super::{SmbConnectionParams, SmbVolumeInner};
use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::SelfHandle;
use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::host::credentials::StoredCredentials;
use cmdr_fs::volume::host::events::VolumeConnection;
use log::{debug, info, warn};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

impl SmbVolumeInner {
    /// Cancels the existing watcher task (if any). The watcher exits on its
    /// next `select!` iteration. Best-effort: if the watcher already exited on
    /// a connection error, the send is a no-op.
    pub(super) fn stop_watcher(&self) {
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
    /// `SmbVolumeInner::do_attempt_reconnect`; the watcher bails on errors and we
    /// respawn here on the next successful reconnect).
    pub(super) fn spawn_watcher(&self, params: &SmbConnectionParams) {
        use crate::build_smb_addr;

        // A retired share no longer owns its volume id, and the watcher is
        // id-scoped: it feeds the listing cache and the index for that id, and
        // its death path drives that id's reconnect. Spawning one here would have
        // a retired share driving somebody else's state.
        if self.is_retired() {
            return;
        }

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        let addr = build_smb_addr(&params.server, params.port);
        let share = params.share_name.clone();
        let username = params.username.clone();
        let password = params.password.clone();
        let volume = super::watcher::WatchedVolume {
            volume_id: self.volume_id.clone(),
            // The watcher outlives the call that spawned it and reports listing
            // changes and watch gaps for the whole share, so it carries its own
            // clone of the share's host rather than reaching back through a
            // handle that stops answering the moment the share is retired.
            host: self.host.clone(),
            // The share this watcher belongs to, as a handle rather than an id:
            // it answers "is this still the volume the app routes to?" without a
            // registry lookup that could resolve to a SUCCESSOR, and it stops
            // answering the moment the registry retires the share.
            share: self.self_handle(),
            // The share's CURRENT mount root, shared rather than copied: the
            // watcher outlives a promotion (it belongs to the session, not to one
            // mount), and the paths it builds have to follow the root the registry
            // moved the ID to.
            mount_path: Arc::clone(&self.active_mount_path),
        };

        // The app's runtime, never `tokio::spawn`: a backend's watcher can be
        // started from a synchronous setup hook or an OS thread with no reactor,
        // where `tokio::spawn` panics.
        self.host.runtime().spawn(super::watcher::run_smb_watcher(
            addr, share, username, password, volume, cancel_rx,
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
    ///   `volume-connection-changed { state: "connected" }`.
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
                "SmbVolumeInner::attempt_reconnect(share={}): already Direct, skipping",
                self.share_name
            );
            return Ok(());
        }

        // The whole share is what a reconnect refusal is about, so its mount root
        // is the path a path-carrying variant would name. (Connect failures
        // classify as connection or auth kinds, which carry a diagnostic instead.)
        let share_root = self
            .active_mount_path
            .read_ignore_poison()
            .to_string_lossy()
            .into_owned();

        // First try: stored credentials (the ones that worked at original connect).
        let params_snapshot = { self.params.read().await.clone() };
        info!(
            "SmbVolumeInner::attempt_reconnect(share={}): trying with cached credentials",
            self.share_name
        );

        let first_attempt = build_session(&params_snapshot).await;
        let (client, tree) = match first_attempt {
            Ok(pair) => pair,
            Err(err) if crate::is_auth_error(&err) => {
                // Cached creds may be stale. Re-pull from the secret store and retry once.
                info!(
                    "SmbVolumeInner::attempt_reconnect(share={}): cached credentials rejected, re-pulling from secret store",
                    self.share_name
                );
                match refresh_credentials_from_store(&self.host, &params_snapshot).await {
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
                                    "SmbVolumeInner::attempt_reconnect(share={}): refreshed credentials also failed: {}",
                                    self.share_name, e2
                                );
                                // The password on the server changed and what we have
                                // saved no longer works. Tell the FE so it shows a
                                // "Sign in" prompt instead of the generic "unreachable".
                                self.emit_state_change_for_id(VolumeConnection::NeedsCredentials);
                                return Err(map_smb_error(e2, &share_root));
                            }
                        }
                    }
                    _ => {
                        // No fresh creds available, or they're identical to the cached ones.
                        warn!(
                            "SmbVolumeInner::attempt_reconnect(share={}): no fresh credentials available; giving up on this attempt",
                            self.share_name
                        );
                        self.emit_state_change_for_id(VolumeConnection::NeedsCredentials);
                        return Err(map_smb_error(err, &share_root));
                    }
                }
            }
            Err(e) => {
                warn!(
                    "SmbVolumeInner::attempt_reconnect(share={}): connect failed: {}",
                    self.share_name, e
                );
                return Err(map_smb_error(e, &share_root));
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
        // launch/upgrade half lives in `smb_upgrade::register_smb_volume`. Skipped
        // when retired: the index for this id belongs to whoever holds it now, which
        // ran the same hook when it registered.
        if !self.is_retired() {
            self.host.indexing().resume_after_reconnect(&self.volume_id);
        }

        info!("SmbVolumeInner::attempt_reconnect(share={}): success", self.share_name);
        Ok(())
    }

    /// Reconnect with freshly-entered credentials (the "Sign in" affordance after a
    /// `needs_credentials` give-up). Persists the new password server-level — mirroring how the
    /// login form saves, so the NEXT reconnect finds it silently — updates the in-memory
    /// params, then runs the standard reconnect. If these credentials are also wrong,
    /// `do_attempt_reconnect` re-emits `needs_credentials`, so a bad retry re-prompts rather than
    /// dead-ending.
    pub(super) async fn do_reconnect_with_credentials(
        &self,
        username: String,
        password: String,
    ) -> Result<(), VolumeError> {
        let server = { self.params.read().await.server.clone() };
        // Server-level (`scope: None`), so one password covers every share on it.
        let stored = StoredCredentials {
            username: username.clone(),
            secret: password.clone(),
        };
        if self
            .host
            .credentials()
            .save_credentials(&server, None, &stored)
            .is_err()
        {
            // Non-fatal: the in-memory params below still carry the creds for this
            // reconnect; only the "silent next time" guarantee is lost.
            warn!(
                "SmbVolumeInner::reconnect_with_credentials(share={}): the secret store didn't take the credentials",
                self.share_name
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
/// `do_attempt_reconnect` already re-pulled credentials and emitted `needs_credentials`,
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
/// proves the connection to the server broke, so we mark the share Disconnected
/// (a background disconnect may not have touched the main session yet, leaving it
/// falsely Direct) and drive `do_attempt_reconnect` on a bounded, growing backoff.
/// Reusing `do_attempt_reconnect` (not a second reconnect path) means:
/// - it coalesces with any FE-initiated reconnect (single-flight `reconnect_lock`),
/// - success rebuilds the session, RESPAWNS the watcher, and resumes the drive
///   index (the on-connect hook) — all in one place, no second state machine.
///
/// Stops early when the share is unmounted, retired (superseded or removed from
/// the registry), back to Direct (an FE reconnect won the race), or an auth
/// failure surfaced (the FE "Sign in" flow owns that). Gives up quietly once the
/// backoff is exhausted.
///
/// `share` is the handle to the share whose watcher died, and it is re-upgraded
/// every iteration. A watcher dying in the window around a supersede must not
/// mark the SUCCESSOR disconnected, and one dying after an eject must not keep a
/// reconnect loop running against a share the app has forgotten; the handle
/// answers both, because it is the share itself rather than a name to look up.
pub(super) fn spawn_watcher_death_reconnect(share: SelfHandle<SmbVolumeInner>) {
    /// The share, while it is still worth acting for: still allocated, still the
    /// registry's, and not unmounted.
    fn still_worth_reconnecting(share: &SelfHandle<SmbVolumeInner>) -> Option<Arc<SmbVolumeInner>> {
        let inner = share.live()?;
        if inner.unmounted.load(Ordering::Relaxed) {
            return None;
        }
        Some(inner)
    }

    let Some(runtime) = share.live().map(|inner| inner.host.runtime()) else {
        return; // gone or retired before the loop even started
    };
    runtime.spawn(async move {
        // The watcher's session died ⇒ the server connection is gone. Mark the
        // share Disconnected so `do_attempt_reconnect` actually rebuilds (it
        // no-ops while Direct) and respawns the watcher.
        let Some(inner) = still_worth_reconnecting(&share) else {
            return; // gone, retired, or unmounted
        };
        let volume_id = inner.volume_id.clone();
        inner.transition_to_disconnected();
        drop(inner);

        for (i, delay) in WATCHER_DEATH_RECONNECT_BACKOFF.iter().enumerate() {
            tokio::time::sleep(*delay).await;

            // Re-upgrade each iteration: a retirement or an unmount can land
            // inside any of these sleeps.
            let Some(inner) = still_worth_reconnecting(&share) else {
                return;
            };
            if inner.connection_state() == ConnectionState::Direct {
                debug!("smb backend reconnect: '{}' already Direct; done", volume_id);
                return; // an FE reconnect (or a prior attempt) won the race
            }

            match inner.do_attempt_reconnect().await {
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

#[cfg(test)]
#[path = "reconnect_test.rs"]
mod reconnect_test;
