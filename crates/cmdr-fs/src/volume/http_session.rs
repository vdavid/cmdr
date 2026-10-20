//! The connection lifecycle the HTTP backends (`cmdr-webdav`, `cmdr-s3`)
//! share: one client per volume, dropped on the first request that finds the
//! server gone, and rebuilt by a probe.
//!
//! HTTP holds no session, so "disconnected" means the last wire-touching
//! request failed with a transport error. Every such request runs inside
//! [`HttpBackend::noting`], which races it against the client's silence watch
//! ([`liveness`](super::liveness)) and hands a `DeviceDisconnected` to
//! [`HttpBackend::note_lost_session`]: that flips the state once, drops the
//! client, and starts the 2/5/15/30/60/120 s backoff loop.
//!
//! **Two independent switches decide what may happen, in this order.**
//!
//! 1. **"Reconnect automatically"**, the user's per-server switch. Off means ❌
//!    no unattended probe, ever, whatever is stored.
//! 2. **The store.** A probe needs the secret; nothing stored means nothing to
//!    try, and the frontend learns that through [`UnattendedReconnect`].
//!
//! ❌ **Never loop on an authentication attempt.** Repeated refusals lock
//! accounts. One unattended try, latched by the session's `auth_attempt_spent`;
//! only a human clears it. ❗ **A typed secret dies with the client it built.**
//! The store is only ever REFRESHED, never seeded.
//!
//! A backend brings only what differs: its client ([`SessionClient`]), its
//! probe and the map from its refusals ([`HttpBackend::dial`]), and where its
//! secret is filed ([`HttpBackend::credential_key`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use super::host::VolumeHost;
use super::host::events::VolumeConnection;
use super::liveness::{Liveness, Timings};
use super::secret_store::{SignIn, SignedIn};
use super::{CellState, ConnectionReporter, ConnectionState, ReportedState, Retires, SelfHandle, VolumeError};

mod reconnect;

/// How an HTTP volume's connection stands. HTTP has no session, so
/// "connected" means the last request that reached the wire came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HttpConnectionState {
    /// Requests are being answered.
    Connected = 0,
    /// The last request found the server gone; something is expected to retry.
    Disconnected = 1,
    /// The server refused the stored secret. Only the user moves this forward:
    /// retrying costs an authentication attempt and buys nothing.
    NeedsCredentials = 2,
}

impl CellState for HttpConnectionState {
    const LIVE: Self = Self::Connected;
    const LOST: Self = Self::Disconnected;

    fn to_byte(self) -> u8 {
        self as u8
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Connected,
            2 => Self::NeedsCredentials,
            _ => Self::Disconnected,
        }
    }
}

impl ReportedState for HttpConnectionState {
    fn reported(self) -> VolumeConnection {
        match self {
            Self::Connected => VolumeConnection::Connected,
            Self::Disconnected => VolumeConnection::Disconnected,
            Self::NeedsCredentials => VolumeConnection::NeedsCredentials,
        }
    }
}

impl HttpConnectionState {
    /// What `Volume::connection_state` publishes, for the switcher dot and the
    /// pane's connect views.
    ///
    /// ❗ A method, ❌ not `From<HttpConnectionState> for ConnectionState`: that
    /// impl prints as `connection → http_session`, welding the two into a
    /// module cycle (`scripts/check/checks/DETAILS.md` § "Rust module cycles",
    /// trap 3).
    pub fn published(self) -> ConnectionState {
        match self {
            Self::Connected => ConnectionState::Direct,
            Self::Disconnected => ConnectionState::Disconnected,
            Self::NeedsCredentials => ConnectionState::NeedsSignIn,
        }
    }
}

/// Whether an unattended reconnect can actually happen as a volume stands.
///
/// ❗ The backend's answer to "the switch is on but nothing comes back", so ❌
/// no frontend has to derive it from a `has_credentials` call. One rung only:
/// an HTTP backend redials out of the secret store or not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnattendedReconnect {
    /// The switch is on and the store holds a secret: a dropped connection
    /// comes back on its own (one attempt; a refusal stops it).
    Possible,
    /// The user's "reconnect automatically" switch is off. Nothing redials,
    /// however full the store.
    SwitchOff,
    /// The switch is on but nothing is remembered, so there is nothing to
    /// redial with. Only the user moves this forward.
    NoStoredSecret,
}

/// Why a dial didn't produce a client, in the terms the lifecycle acts on.
#[derive(Debug)]
pub enum DialRefusal {
    /// The server refused the secret: the one unattended attempt is spent, and
    /// only a person moves the volume forward.
    Rejected,
    /// Only a person moves this forward, but nothing was spent on finding that
    /// out.
    NeedsUser,
    /// The network or the server. Worth trying again; the error is what the
    /// attempt answers.
    Transient(VolumeError),
}

/// Where an account's secret is filed in the store.
pub struct CredentialKey {
    /// The store's service name for the server.
    pub service: String,
    /// The account within it: the username, or the access key id.
    pub scope: String,
}

/// The client a session holds: its silence watch, and a cheap request that
/// proves the server answers.
pub trait SessionClient: Send + Sync + 'static {
    /// The evidence that the server is still there, which every request feeds.
    fn liveness(&self) -> &Arc<Liveness>;
    /// Asks the server something cheap on a fresh connection; whether anything
    /// answered.
    fn ping(&self) -> impl Future<Output = bool> + Send;
}

/// A backend's connection-scoped state, which owns an [`HttpSession`] and says
/// how to dial.
///
/// The provided methods are the whole lifecycle; ❌ don't override them.
pub trait HttpBackend: Retires + Send + Sync + Sized + 'static {
    /// The client a dial builds.
    type Client: SessionClient;

    /// The backend's name in log lines (`webdav`, `s3`).
    const BACKEND: &'static str;

    /// The session this state carries.
    fn session(&self) -> &HttpSession<Self>;

    /// Everything the backend asks the app around it.
    fn host(&self) -> &VolumeHost;

    /// Where this volume's secret is filed.
    fn credential_key(&self) -> CredentialKey;

    /// Builds a client with `secret` and proves it with the backend's probe.
    /// Runs under the single-flight lock; a refusal maps to what the lifecycle
    /// does next.
    fn dial(&self, secret: &str) -> impl Future<Output = Result<Self::Client, DialRefusal>> + Send;

    /// A rebuilt client just went in, for work the outage held back (S3's
    /// unfinished-upload sweep). Defaults to nothing.
    fn client_installed(&self) {}

    /// Runs `work`, noticing on the way out if the answer says the server is
    /// gone. ❗ Every delegator that can reach the wire wraps itself in this:
    /// with no watcher, the operations ARE the detector.
    ///
    /// It also counts `work` as waiting on the server, which the silence watch
    /// looks after, and cuts it with `DeviceDisconnected` the moment the client
    /// is found gone, by the watch or by another operation: a silent server
    /// closes nothing, so nothing else would ever end the wait.
    fn noting<T, W>(self: &Arc<Self>, work: W) -> impl Future<Output = Result<T, VolumeError>> + Send
    where
        W: Future<Output = Result<T, VolumeError>> + Send,
    {
        async move {
            let session = self.session();
            // ❗ The generation is read with the client, BEFORE `work` runs, so a
            // failure is pinned to the client the call started on: one a
            // reconnect replaced meanwhile is old news and moves nothing. `work`
            // takes its own client a moment later, so the read can only be older
            // than the one it used, which at worst leaves a real loss for the next
            // call to notice, never a healthy client marked lost.
            let (client, ran_on) = {
                let installed = session.client.read().await;
                (installed.clone(), session.connection.generation())
            };
            let outcome = match client {
                // No client: the work answers `DeviceDisconnected` on its own, at once.
                None => work.await,
                Some(client) => {
                    let liveness = Arc::clone(client.liveness());
                    let waiting = liveness.begin();
                    if waiting.needs_a_watch() {
                        reconnect::watch_over(&**self, &client);
                    }
                    drop(client);
                    let outcome = tokio::select! {
                        biased;
                        outcome = work => outcome,
                        () = liveness.lost().cancelled() => {
                            Err(VolumeError::DeviceDisconnected(session.connection.volume_id().to_string()))
                        }
                    };
                    drop(waiting);
                    outcome
                }
            };
            if let Err(error) = &outcome {
                self.note_lost_session(error, ran_on);
            }
            outcome
        }
    }

    /// Notices, once, that the server behind this volume is gone. ❗ Acts only
    /// on the `Connected` → `Disconnected` edge, and only for the client the
    /// failing call RAN on (`ran_on`, read before the call started): a call
    /// that started on a client a reconnect has since replaced must not drop
    /// the fresh one and cut every request waiting on it.
    fn note_lost_session(self: &Arc<Self>, error: &VolumeError, ran_on: u64) {
        if !matches!(error, VolumeError::DeviceDisconnected(_)) {
            return;
        }
        let session = self.session();
        if !session.connection.lose(ran_on) {
            return;
        }
        let owner = Arc::clone(self);
        let handle = session.self_handle();
        let auto_reconnect = session.auto_reconnect.load(Ordering::Relaxed);
        // ❗ Dropped HERE when nothing holds the lock, so an `attempt_reconnect`
        // the frontend fires on the event it just got doesn't find the dead
        // client still installed and answer "fine" without probing. (`rebuild`
        // treats one found under a non-`Connected` state as dead anyway.)
        let dropped = match session.client.try_write() {
            Ok(mut client) => {
                session.drop_dead_client(&mut client);
                true
            }
            Err(_) => false,
        };
        self.host().runtime().spawn(async move {
            if !dropped {
                let session = owner.session();
                session.drop_dead_client(&mut *session.client.write().await);
            }
            drop(owner);
            if auto_reconnect {
                reconnect::run_reconnect_loop(handle).await;
            }
        });
    }

    /// Probes now, on the unattended terms. Single-flight.
    fn attempt_reconnect(&self) -> impl Future<Output = Result<(), VolumeError>> + Send {
        async move {
            reconnect::rebuild(self, None)
                .await
                .map_err(|stalled| reconnect::report(self, stalled))
        }
    }

    /// The attended variant: `sign_in` is what a person just typed for THIS
    /// volume's account (the backend checks the account first: another one is
    /// another volume). ❗ The secret is filed only once it CONNECTED, the way
    /// its Remember box asks (`secret_store::keep_signed_in_secret`): written
    /// before the dial, a typo replaced the secret that worked (#530). An
    /// attended rebuild always dials, so `Ok` means this secret was accepted.
    fn reconnect_with_sign_in(&self, sign_in: SignIn) -> impl Future<Output = Result<SignedIn, VolumeError>> + Send {
        async move {
            self.session().auth_attempt_spent.store(false, Ordering::Relaxed);
            reconnect::rebuild(self, Some(sign_in.secret.clone()))
                .await
                .map_err(|stalled| reconnect::report(self, stalled))?;
            Ok(self.remember_sign_in(sign_in).await)
        }
    }

    /// Files a sign-in that already connected, the way its Remember box asks,
    /// ❌ without dialing: the second half of
    /// [`reconnect_with_sign_in`](Self::reconnect_with_sign_in), and the sheet's
    /// "Try again" after a refused save (#603). The backend checks the account
    /// first, as it does for a sign-in.
    fn remember_sign_in(&self, sign_in: SignIn) -> impl Future<Output = SignedIn> + Send {
        async move {
            let kept = reconnect::keep_signed_in_secret(self, &sign_in).await;
            SignedIn::after(&sign_in, kept)
        }
    }

    /// Moves the user's "reconnect automatically" switch on a mounted volume.
    /// ❗ Switching it ON while the volume sits `Disconnected` starts the
    /// backoff loop then and there.
    fn set_auto_reconnect(&self, on: bool) {
        let was = self.session().auto_reconnect.swap(on, Ordering::Relaxed);
        if on && !was {
            reconnect::start_reconnect_loop_if_down(self);
        }
    }

    /// Whether an unattended reconnect can happen as this volume stands. Reads
    /// the secret store only when the switch is on, because a needless read is
    /// a needless Keychain prompt.
    fn unattended_reconnect(&self) -> impl Future<Output = UnattendedReconnect> + Send {
        async move {
            if !self.session().auto_reconnect.load(Ordering::Relaxed) {
                return UnattendedReconnect::SwitchOff;
            }
            if reconnect::stored_secret(self).await.is_some() {
                UnattendedReconnect::Possible
            } else {
                UnattendedReconnect::NoStoredSecret
            }
        }
    }

    /// The volume is leaving the registry: no more reconnects, and the client
    /// goes. ❌ No connection event: the frontend learns through
    /// `volumes-changed`.
    fn unmount(self: &Arc<Self>) {
        let session = self.session();
        session.unmounted.store(true, Ordering::Relaxed);
        session.connection.mark_gone_silently();
        let owner = Arc::clone(self);
        self.host().runtime().spawn(async move {
            owner.session().client.write().await.take();
        });
    }
}

/// The lifecycle's state: the live client, the connection it reports, and the
/// switches that gate a rebuild.
pub struct HttpSession<B: HttpBackend> {
    /// The live client. `None` once a request found the server gone, at which
    /// point every operation fails fast rather than each one timing out.
    client: tokio::sync::RwLock<Option<Arc<B::Client>>>,
    /// The state the host was last told about, packed with the installed
    /// client's generation.
    connection: ConnectionReporter<HttpConnectionState>,
    /// The owner's own weak reference, for the background work that outlives
    /// the call that started it.
    me: Weak<B>,
    /// Single-flight around a client rebuild.
    reconnect_lock: tokio::sync::Mutex<()>,
    /// Set on unmount. A reconnect in flight bails rather than installing a
    /// client into a volume the app has forgotten.
    unmounted: AtomicBool,
    /// Whether the user's per-server "reconnect automatically" switch is on.
    /// Live, so flipping it needs no remount.
    auto_reconnect: AtomicBool,
    /// Whether the one unattended authentication attempt has been spent. ❌
    /// Never a loop: repeated refusals lock accounts.
    auth_attempt_spent: AtomicBool,
    /// How long silence may last before the server counts as gone. A field
    /// rather than the constant so a cell can shorten it and still run in real
    /// time.
    silence: std::sync::RwLock<Timings>,
}

impl<B: HttpBackend> HttpSession<B> {
    /// A session for `volume_id` on `client` (`None` for a volume with no
    /// server behind it), owned by the state `me` points at: build it inside
    /// `Arc::new_cyclic`.
    pub fn new(
        me: &Weak<B>,
        volume_id: &str,
        host: &VolumeHost,
        client: Option<B::Client>,
        auto_reconnect: bool,
    ) -> Self {
        Self {
            client: tokio::sync::RwLock::new(client.map(Arc::new)),
            connection: ConnectionReporter::new(volume_id, host, HttpConnectionState::Connected),
            me: me.clone(),
            reconnect_lock: tokio::sync::Mutex::new(()),
            unmounted: AtomicBool::new(false),
            auto_reconnect: AtomicBool::new(auto_reconnect),
            auth_attempt_spent: AtomicBool::new(false),
            silence: std::sync::RwLock::new(Timings::PRODUCTION),
        }
    }

    /// The connection's state, generation, and retirement flag.
    pub fn connection(&self) -> &ConnectionReporter<HttpConnectionState> {
        &self.connection
    }

    /// The owner's own handle, for the background work that has to keep asking
    /// whether the registry still serves it.
    pub fn self_handle(&self) -> SelfHandle<B> {
        SelfHandle::new(self.me.clone())
    }

    /// The live client, cloned out from under a short read guard. ❗ Clone and
    /// release: holding the guard across a request would serialize every other
    /// request behind it.
    pub async fn client(&self) -> Result<Arc<B::Client>, VolumeError> {
        self.installed_client()
            .await
            .ok_or_else(|| VolumeError::DeviceDisconnected(self.connection.volume_id().to_string()))
    }

    /// The live client, if there is one.
    pub async fn installed_client(&self) -> Option<Arc<B::Client>> {
        self.client.read().await.clone()
    }

    /// Drops the live client and stops reconnecting, silently. There is no
    /// session to close: dropping IS the shutdown, and the connection pool goes
    /// with it.
    pub async fn disconnect(&self) {
        self.unmounted.store(true, Ordering::Relaxed);
        self.connection.mark_gone_silently();
        self.client.write().await.take();
    }

    /// Shortens the silence ladder, for a cell that runs it in real time. Takes
    /// effect from the next watch started.
    #[cfg(any(test, feature = "testing"))]
    pub fn set_silence_timings(&self, timings: Timings) {
        use crate::ignore_poison::RwLockIgnorePoison;
        *self.silence.write_ignore_poison() = timings;
    }

    /// Drops the live client the way a server going away would, ❗ WITHOUT
    /// starting the backoff loop, so a cell drives the recovery itself. The
    /// state stays `Connected` until an operation notices.
    #[cfg(any(test, feature = "testing"))]
    pub async fn simulate_loss(&self) {
        self.client.write().await.take();
    }

    /// Takes the installed client out as dead, and cuts every operation still
    /// waiting on it. ❗ Only under a non-`Connected` state: a late task must
    /// never take the fresh client a reconnect just installed, which `rebuild`
    /// marks `Connected` under the same write guard.
    fn drop_dead_client(&self, client: &mut Option<Arc<B::Client>>) {
        if self.connection.state() == HttpConnectionState::Connected {
            return;
        }
        if let Some(dead) = client.take() {
            dead.liveness().declare_lost();
        }
    }
}
