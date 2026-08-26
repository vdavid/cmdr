//! The SSH transport: the ONE module that names `russh`.
//!
//! ❗ Keep it that way. `russh` shipped eight breaking minors in eight months
//! (`docs/notes/sftp-crate-evaluation-2026-08-22.md`), and the whole point of
//! this boundary is that a bump is one file's problem. Everything above works in
//! [`SshConnection`], [`crate::trust::PresentedHostKey`], and
//! [`crate::auth::AuthRung`], none of which mention an SSH type.
//!
//! ## Three hazards this module is shaped around
//!
//! **A `Sftp::new` future has to be safe to abort**, and it is only from
//! `openssh-sftp-client` 0.15.8 on: before that, `tasks.rs` did
//! `tx.send(extensions).unwrap()` and panicked a spawned task whenever that
//! future was dropped before the server's hello arrived
//! (openssh-rust/openssh-sftp-client#176). ❗ 0.15.8 is a floor rather than a
//! preference: [`stop_engine`] aborts the future on every hello that doesn't
//! arrive, and `reconnect::guarded_dial` leans on the same fix.
//!
//! **An engine nobody waits for holds the socket by itself.** `Sftp::new` spawns
//! tasks that own the channel, and each of them owns a sender the `russh`
//! session task lives on, so dropping the session handle closes nothing while
//! they sit on a hello that never comes. [`stop_engine`] disconnects the session
//! rather than dropping it, which ends the session loop and errors those tasks
//! out with it.
//!
//! **`Sftp::close()` never returns over a `russh` channel.** It awaits a read
//! task that only ends at reader EOF, which a channel doesn't give until it's
//! closed. Dropping [`SshConnection`] is the clean shutdown: the engine's own
//! drop orders it and both tasks exit. ❌ There is no `close()` call in this
//! crate, and adding one hangs `disconnect_sftp_volume` forever.
//!
//! ## Cancelling a connect
//!
//! [`dial`] takes a [`CancellationToken`] and honours it at every await that can
//! wait on a server. All four phases stop where they stand; only the lever
//! differs:
//!
//! - The key exchange, the auth ladder, and the two channel requests all run
//!   through [`within`], which races the work against the token and DROPS it on
//!   a cancel. `russh` unwinds a dropped future cleanly.
//! - The SFTP hello runs in a task of its own, so dropping a future is no lever
//!   there. [`await_hello`] races the JOIN HANDLE and hands a hello that was
//!   cancelled or ran out of window to [`stop_engine`], which aborts the engine
//!   and disconnects the session. The far end sees the socket close at the
//!   cancel: 57 ms against `sftp-fixture-openssh`, and the measurement is one
//!   `docker exec` probe wide
//!   (`volume::cancel_test`, a hello peer that never answers, 2026-08-26).

use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::StoredCredentials;
use openssh_sftp_client::{Sftp, SftpOptions};
use russh::client::{self, AuthResult, Handler, KeyboardInteractiveAuthResponse};
use russh::keys::agent::AgentIdentity;
use russh::keys::{Algorithm, HashAlg, PrivateKeyWithHashAlg, PublicKey};
use russh::{Channel, Disconnect};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::auth::{AuthRung, AuthRungUsed, ladder};
use crate::errors::SftpConnectError;
use crate::extensions::ServerExtensions;
use crate::known_hosts::KnownHostsFile;
use crate::params::SftpConnectionParams;
use crate::trust::{self, HostKeyDecision, PresentedHostKey};

/// How long the TCP connect and key exchange get, and then the auth ladder
/// again on its own. Generous enough for a satellite link, short enough that a
/// black-holed address doesn't hold a sign-in dialog for most of a minute.
///
/// ❗ Two of these plus [`SUBSYSTEM_TIMEOUT`] is the 30 s a user could sit
/// through touching nothing. Cancelling ends it sooner at any point, which is
/// what makes the budget a backstop rather than the way out.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the SFTP subsystem gets to answer its hello once the SSH session is
/// up. A server with no `sftp-server` installed simply never answers.
const SUBSYSTEM_TIMEOUT: Duration = Duration::from_secs(10);

/// The channel window Cmdr advertises for data it RECEIVES, in bytes.
///
/// `russh`'s default is OpenSSH's own 2 MiB, and at 50 ms RTT that is the
/// binding ceiling on reads no matter how deep the request window goes: depth 8
/// and depth 16 measure the same 14–18 MB/s because eight 255 KiB requests
/// already fill the channel. Raising it is what let depth pay at all — 42 MB/s
/// at depth 32 (`docs/notes/sftp-crate-evaluation-2026-08-22.md`). It does
/// nothing for uploads: the server's window governs those and OpenSSH fixes it
/// at 2 MiB.
const CHANNEL_WINDOW_BYTES: u32 = 16 * 1024 * 1024;

/// A live SSH session with one SFTP channel on it.
///
/// Drop order is the shutdown order and it is deliberate: the engine goes first
/// so it stops writing, then the session closes the transport under it.
pub struct SshConnection {
    sftp: Sftp,
    /// What the server advertised in its hello, read once because it can't
    /// change while the session lives.
    extensions: ServerExtensions,
    /// Held only to keep the session alive. Dropping it closes the connection,
    /// which is exactly how this crate disconnects.
    _ssh: client::Handle<TrustHandler>,
}

impl SshConnection {
    /// The SFTP engine. Every method on it takes `&self`, so callers take their
    /// own `Fs` and no operation serializes another.
    pub fn sftp(&self) -> &Sftp {
        &self.sftp
    }

    /// What this server can do beyond bare v3.
    ///
    /// ❗ The one place a capability is read from. ❌ Never reach for a
    /// `Sftp::support_*` predicate at a call site: a fallback nobody can drive
    /// without a server that lacks the extension is a fallback nobody tests.
    pub fn extensions(&self) -> ServerExtensions {
        self.extensions
    }
}

/// A server whose key nobody has approved yet, and what to say about it.
///
/// Carries serde and `specta::Type` because this is the value the approval flow
/// hands the frontend and gets back: `approve_sftp_host_key` re-verifies the
/// fingerprint against what the server presents now, which is what stops an
/// approval being replayed against a different key.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyPrompt {
    /// The server, as addressed.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// The SSH key-type name it presented.
    pub algorithm: String,
    /// The OpenSSH `SHA256:…` fingerprint a human compares.
    pub fingerprint: String,
    /// Whether this is first contact or a key that CHANGED.
    pub kind: HostKeyPromptKind,
}

/// Which of the two host-key moments a prompt is for.
///
/// ❗ Two variants rather than one flag because they must never share a path: a
/// first-seen key is routine, and a changed one is the shape a
/// man-in-the-middle takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum HostKeyPromptKind {
    /// Nothing is stored for this host under this algorithm.
    Unknown,
    /// Something IS stored, and the server presented a different key.
    Changed,
}

/// What a dial produced.
pub enum DialOutcome {
    /// A working session, and the rung it was built on.
    Connected {
        /// The live session.
        connection: SshConnection,
        /// Which credential proved us, which is what decides whether a dropped
        /// session may rebuild itself (`crate::auth::reconnect_policy`).
        rung: AuthRungUsed,
    },
    /// The server's key needs a human. ❗ No session is held across the prompt:
    /// the dial is abandoned and a fresh one runs after approval.
    NeedsHostKeyApproval(HostKeyPrompt),
}

/// Opens an SSH session to `params`' server and an SFTP channel on it.
///
/// Cancelling `cancel` ends the attempt where it stands, whichever phase it is
/// in; § "Cancelling a connect" at the top of this module has the lever each one
/// uses. ❗ A cancelled dial leaves nothing behind: no session, no approval, no
/// stored secret.
///
/// ❌ Never drop this future instead of cancelling the token: a drop inside the
/// SFTP hello detaches the engine and leaves the server's session open for the
/// life of the process (`crates/cmdr-sftp/DETAILS.md` § "2. An abandoned
/// `Sftp::new`"). [`crate::volume::connect_sftp_volume`] is what guarantees
/// nothing does, by
/// running the whole dial inside a task.
pub async fn dial(
    params: SftpConnectionParams,
    host: VolumeHost,
    offered_secret: Option<String>,
    cancel: CancellationToken,
) -> Result<DialOutcome, SftpConnectError> {
    let mut session = match open_session(&params, &host, &cancel).await? {
        Opened::NeedsApproval(prompt) => return Ok(DialOutcome::NeedsHostKeyApproval(prompt)),
        Opened::Session(session) => session,
    };

    let rung = within(
        &cancel,
        handshake_deadline(),
        authenticate(&mut session, &params, &host, offered_secret),
    )
    .await??;

    let connection = hello(session, &host, &cancel).await?;
    Ok(DialOutcome::Connected { connection, rung })
}

/// Runs one step of a connect under a deadline and the user's cancel.
///
/// ❗ Either ending DROPS `work`, which is exactly why the key exchange, the auth
/// ladder, and the two channel requests are cancelable at all: `russh` unwinds a
/// dropped future cleanly. [`await_hello`] races a JOIN HANDLE instead of coming
/// through here, because the engine has to be stopped in a particular order; it
/// stops just as promptly.
async fn within<T>(
    cancel: &CancellationToken,
    deadline: tokio::time::Instant,
    work: impl Future<Output = T>,
) -> Result<T, SftpConnectError> {
    tokio::select! {
        // Biased so an already-cancelled token wins without polling `work` at
        // all: a cancel that lands before a step starts must cost no packet.
        biased;
        () = cancel.cancelled() => Err(SftpConnectError::Cancelled),
        done = tokio::time::timeout_at(deadline, work) => done.map_err(|_elapsed| SftpConnectError::TimedOut),
    }
}

/// The deadline one handshake phase gets, counted from now.
fn handshake_deadline() -> tokio::time::Instant {
    tokio::time::Instant::now() + HANDSHAKE_TIMEOUT
}

/// What the key exchange came to.
enum Opened {
    /// A session, past the host-key check and ready to authenticate.
    Session(client::Handle<TrustHandler>),
    /// The server's key needs a human, so there is no session to hold at all.
    NeedsApproval(HostKeyPrompt),
}

/// The TCP connect and the key exchange, including the host-key decision.
async fn open_session(
    params: &SftpConnectionParams,
    host: &VolumeHost,
    cancel: &CancellationToken,
) -> Result<Opened, SftpConnectError> {
    let known_hosts = KnownHostsFile::read_default();
    let pinned = trust::algorithms_to_pin(host.host_keys(), &known_hosts, &params.host, params.port);
    let config = Arc::new(build_config(&pinned));

    let seen = Arc::new(Mutex::new(None));
    let handler = TrustHandler {
        host: params.host.clone(),
        port: params.port,
        volume_host: host.clone(),
        known_hosts,
        seen: Arc::clone(&seen),
    };

    match within(
        cancel,
        handshake_deadline(),
        client::connect(config, (params.host.as_str(), params.port), handler),
    )
    .await?
    {
        Ok(session) => Ok(Opened::Session(session)),
        // A refused key is the likeliest reason a connect fails once the TCP
        // socket is up, and it's the one with a typed answer. `seen` is what
        // the handler left behind; anything else is a transport problem.
        Err(e) => match seen.lock_ignore_poison().take() {
            Some((key, HostKeyDecision::Revoked)) => Err(SftpConnectError::HostKeyRevoked {
                algorithm: key.algorithm,
                fingerprint: key.fingerprint,
            }),
            Some((key, HostKeyDecision::Unknown)) => {
                Ok(Opened::NeedsApproval(prompt(params, key, HostKeyPromptKind::Unknown)))
            }
            Some((key, HostKeyDecision::Changed)) => {
                Ok(Opened::NeedsApproval(prompt(params, key, HostKeyPromptKind::Changed)))
            }
            _ => Err(SftpConnectError::Unreachable(e.to_string())),
        },
    }
}

/// Opens the channel, asks for the `sftp` subsystem, and waits out the server's
/// hello.
///
/// ❗ **One deadline covers all three**, rather than a budget each: a server that
/// stalls picks whichever of them it likes, and three windows here would make the
/// worst case a user sits through 50 s instead of 30 s.
///
/// The first two are ordinary `russh` futures and go under the token like the
/// handshake phases do. The hello is the one the token can't simply drop: the
/// engine runs in a task of its own, so [`await_hello`] stops it by hand.
async fn hello(
    session: client::Handle<TrustHandler>,
    host: &VolumeHost,
    cancel: &CancellationToken,
) -> Result<SshConnection, SftpConnectError> {
    let deadline = tokio::time::Instant::now() + SUBSYSTEM_TIMEOUT;
    let starting = start_engine(&session, host, cancel, deadline).await?;
    await_hello(session, cancel, deadline, starting).await
}

/// Opens the channel, asks for `sftp`, and puts `Sftp::new` on a task of its own.
///
/// Both requests are ordinary `russh` futures, so they go under the token like
/// the handshake phases do. ❗ Split from [`await_hello`] so a cell can run this
/// half on a live token and hand the other half a cancelled one: the wait in
/// [`await_hello`] measures 1.3 ms against `sftp-fixture-openssh` (instrumented
/// dial, 2026-08-23), which no amount of timing reaches reliably.
async fn start_engine(
    session: &client::Handle<TrustHandler>,
    host: &VolumeHost,
    cancel: &CancellationToken,
    deadline: tokio::time::Instant,
) -> Result<StartingEngine, SftpConnectError> {
    let channel = open_channel(session, cancel, deadline).await?;
    within(cancel, deadline, channel.request_subsystem(true, "sftp"))
        .await?
        .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
    Ok(spawn_engine(host, channel))
}

/// The engine's own task, still waiting on the server's hello.
type StartingEngine = JoinHandle<Result<Sftp, openssh_sftp_client::Error>>;

/// One SSH channel, under the token and the phase deadline.
async fn open_channel(
    session: &client::Handle<TrustHandler>,
    cancel: &CancellationToken,
    deadline: tokio::time::Instant,
) -> Result<Channel<client::Msg>, SftpConnectError> {
    within(cancel, deadline, session.channel_open_session())
        .await?
        .map_err(|e| SftpConnectError::Transport(e.to_string()))
}

/// Puts `Sftp::new` on a task of its own over `channel`.
///
/// ❗ The ONE place the engine is built, so the test-only peer in
/// `dial_cancelling_inside_the_hello` can never drift from what a real dial
/// hands the engine.
fn spawn_engine(host: &VolumeHost, channel: Channel<client::Msg>) -> StartingEngine {
    let (reader, writer) = tokio::io::split(channel.into_stream());
    host.runtime().spawn(Sftp::new(writer, reader, SftpOptions::new()))
}

/// Waits out the server's hello, and stops the engine by hand if it never comes.
///
/// ❗ The engine runs in a task, so a cancel here can't drop a future the way the
/// earlier phases do; [`stop_engine`] is the lever instead, and it is the same
/// one for a window that ran out. Both endings leave the server nothing to hold.
async fn await_hello(
    session: client::Handle<TrustHandler>,
    cancel: &CancellationToken,
    deadline: tokio::time::Instant,
    mut starting: StartingEngine,
) -> Result<SshConnection, SftpConnectError> {
    let waited = tokio::select! {
        biased;
        () = cancel.cancelled() => Err(SftpConnectError::Cancelled),
        // ❗ `&mut`, so the handle survives either ending and can still be
        // aborted. Dropping it would DETACH the task instead, and an engine
        // parked on a hello that never comes holds the socket for the life of
        // the process rather than giving up on its own.
        joined = tokio::time::timeout_at(deadline, &mut starting) => joined.map_err(|_elapsed| SftpConnectError::TimedOut),
    };
    let joined = match waited {
        Ok(joined) => joined,
        // Cancelled, or the window ran out with no hello: either way nobody is
        // waiting for this engine any more.
        Err(ended) => {
            stop_engine(starting, session).await;
            return Err(ended);
        }
    };

    match joined {
        // The engine's own task DIED rather than ended, which on 0.15.8 should
        // not happen whatever we do to the future: the regression tell for
        // `DETAILS.md` § "2. An abandoned `Sftp::new`".
        Err(died) => Err(SftpConnectError::Transport(died.to_string())),
        Ok(Err(e)) => Err(SftpConnectError::Transport(e.to_string())),
        Ok(Ok(sftp)) => {
            let extensions = ServerExtensions::probe(&sftp);
            // PII-free: protocol constants only. What a server can do decides
            // which path a copy and a rename take, so it's the first thing worth
            // knowing when one of them behaves differently than it does against a
            // stock OpenSSH.
            log::debug!(target: "volume", "sftp server extensions: {:?}", extensions.advertised());
            Ok(SshConnection {
                sftp,
                extensions,
                _ssh: session,
            })
        }
    }
}

/// Which peer a cell puts on the other end of the SFTP channel.
#[cfg(test)]
pub(crate) enum HelloPeer {
    /// The server's real `sftp` subsystem, which answers in about a
    /// millisecond: the production path, and a hello window nothing can aim at.
    Subsystem,
    /// A command that swallows `SSH_FXP_INIT` and answers nothing, so the hello
    /// window stays open until the cell closes it. Carries the marker its entry
    /// in the server's process table is found by, which is how a cell watches
    /// the server-side session go.
    Stalling(String),
}

/// Stops an engine nobody is waiting for any more, and closes the transport
/// under it.
///
/// The order is [`SshConnection`]'s own: the aborted task is awaited out first,
/// so the engine has let its end of the channel go before the session goes.
///
/// ❗ **`disconnect`, ❌ never a bare `drop(session)`.** `Sftp::new` spawns tasks
/// of its own that hold the channel, and each of them holds a sender the session
/// task lives on, so dropping the handle alone leaves a hello nobody answered
/// holding its socket for the life of the process. Disconnecting ends the session
/// loop, which closes the transport and errors those tasks out with it. ❗ Only
/// this path needs it: a session that reached [`SshConnection`] is shut down by
/// dropping the ENGINE, whose own drop orders its tasks to stop.
async fn stop_engine(starting: StartingEngine, session: client::Handle<TrustHandler>) {
    starting.abort();
    let _aborted = starting.await;
    let _closing = session.disconnect(Disconnect::ByApplication, "", "").await;
    drop(session);
}

/// Dials as far as the SFTP hello, signals `reached_hello`, and waits it out
/// under the cell's own `cancel`.
///
/// ❗ The one phase a cell can't reach by timing against a real subsystem: the
/// window is about a millisecond wide. [`HelloPeer::Stalling`] holds it open
/// instead, and everything from the signal on is [`await_hello`]'s own code, so
/// a cell driving this drives production.
///
/// `cfg(test)` rather than the crate's `testing` feature because this is
/// `pub(crate)`: nothing outside the crate could call it whatever the gate said.
#[cfg(test)]
pub(crate) async fn dial_cancelling_inside_the_hello(
    params: SftpConnectionParams,
    host: VolumeHost,
    peer: HelloPeer,
    cancel: CancellationToken,
    reached_hello: tokio::sync::oneshot::Sender<()>,
) -> Result<SshConnection, SftpConnectError> {
    let live = CancellationToken::new();
    let Opened::Session(mut session) = open_session(&params, &host, &live).await? else {
        return Err(SftpConnectError::Transport(
            "this helper needs a server whose key is already approved".to_string(),
        ));
    };
    within(
        &live,
        handshake_deadline(),
        authenticate(&mut session, &params, &host, None),
    )
    .await??;

    // Everything up to here on a live token, so the engine really is waiting on
    // the server's hello when the cancel lands.
    let deadline = tokio::time::Instant::now() + SUBSYSTEM_TIMEOUT;
    let starting = match peer {
        HelloPeer::Subsystem => start_engine(&session, &host, &live, deadline).await?,
        HelloPeer::Stalling(marker) => {
            let channel = open_channel(&session, &live, deadline).await?;
            // `cat` reads the engine's `SSH_FXP_INIT` and writes nothing back;
            // the `:` after it is what carries `marker` into the process table
            // and ends the moment the session's pipes close.
            within(
                &live,
                deadline,
                channel.exec(true, format!("cat >/dev/null; : {marker}")),
            )
            .await?
            .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
            spawn_engine(&host, channel)
        }
    };

    let _ = reached_hello.send(());
    await_hello(session, &cancel, deadline, starting).await
}

fn prompt(params: &SftpConnectionParams, key: PresentedHostKey, kind: HostKeyPromptKind) -> HostKeyPrompt {
    HostKeyPrompt {
        host: params.host.clone(),
        port: params.port,
        algorithm: key.algorithm,
        fingerprint: key.fingerprint,
        kind,
    }
}

/// Asks `(server, port)` which host key it presents, and stops there.
///
/// ❗ The key exchange runs and the connection is then refused, so this never
/// authenticates: no password leaves the machine, no attempt is spent, and a
/// server that would lock an account after three tries sees nothing at all. It
/// is also why the engine's cancellation hazard doesn't reach here — `Sftp::new`
/// is never constructed, so there is no spawned task to panic.
///
/// The negotiation is pinned exactly as [`dial`] pins it, so the algorithm a
/// probe sees is the algorithm a dial would see. Without that, a two-key server
/// could answer one probe with ed25519 and the next dial with rsa.
pub(crate) async fn presented_host_key(
    server: &str,
    port: u16,
    host: &VolumeHost,
) -> Result<PresentedHostKey, SftpConnectError> {
    let known_hosts = KnownHostsFile::read_default();
    let pinned = trust::algorithms_to_pin(host.host_keys(), &known_hosts, server, port);
    let config = Arc::new(build_config(&pinned));

    let seen = Arc::new(Mutex::new(None));
    let handler = ProbeHandler {
        seen: Arc::clone(&seen),
    };

    let dialed = tokio::time::timeout(HANDSHAKE_TIMEOUT, client::connect(config, (server, port), handler)).await;
    // The refusal is the expected ending, so what the handler left behind
    // outranks whatever error russh reported for it.
    if let Some(key) = seen.lock_ignore_poison().take() {
        return Ok(key);
    }
    match dialed {
        Err(_elapsed) => Err(SftpConnectError::TimedOut),
        // The handler always refuses, so a session here would mean russh reached
        // authentication without ever calling `check_server_key`.
        Ok(Ok(_session)) => Err(SftpConnectError::Transport(
            "the server's key exchange completed without presenting a host key".to_string(),
        )),
        Ok(Err(e)) => Err(SftpConnectError::Unreachable(e.to_string())),
    }
}

/// Records `key` as trusted for `(host, port)`, so the next dial is silent.
///
/// The approval flow's second half. ❗ The caller re-verifies that this is still
/// the key the server presents before calling; recording a fingerprint a user
/// approved minutes ago against whatever answers now is how an approval gets
/// replayed onto a different key. [`crate::volume::approve_host_key`] is that
/// caller, and the only one production has.
pub fn approve(host: &VolumeHost, server: &str, port: u16, algorithm: &str, fingerprint: &str) {
    trust::record_approval(host.host_keys(), server, port, algorithm, fingerprint);
}

/// The client config, with the channel window raised and key negotiation pinned.
fn build_config(pinned: &[String]) -> client::Config {
    let mut config = client::Config {
        window_size: CHANNEL_WINDOW_BYTES,
        ..client::Config::default()
    };
    if !pinned.is_empty() {
        // Filtered out of the DEFAULT order rather than rebuilt from the pinned
        // list, so the preference order stays russh's (modern first) and an
        // algorithm name we can't parse simply doesn't narrow anything.
        let wanted: Vec<Algorithm> = config
            .preferred
            .key
            .iter()
            .filter(|algorithm| pinned.iter().any(|name| name == algorithm.as_str()))
            .cloned()
            .collect();
        if !wanted.is_empty() {
            config.preferred.key = Cow::Owned(wanted);
        }
    }
    config
}

/// What one rung of the ladder came to.
///
/// ❗ Three outcomes rather than a `bool`, because the difference between the
/// first two is what the user is shown: a rung with nothing behind it means
/// "sign in", and a rung the server turned down means "that credential is
/// wrong". Collapsing them tells someone who has never entered a password that
/// their password is wrong.
enum Attempt {
    /// Nothing to offer: no agent running, no readable key file, no stored
    /// secret. The server was never asked.
    NotOffered,
    /// Offered and turned down.
    Refused,
    /// Offered and accepted.
    Accepted(AuthRungUsed),
}

/// Walks the ladder, stopping at the first rung the server accepts.
/// `offered` is a secret the USER just typed, for an attended reconnect. It
/// stands in for the store's answer for this dial and dies with it, which is what
/// lets a passphrase-protected key come back without the passphrase ever being
/// written anywhere (`crate::volume::reconnect`).
async fn authenticate(
    session: &mut client::Handle<TrustHandler>,
    params: &SftpConnectionParams,
    host: &VolumeHost,
    offered: Option<String>,
) -> Result<AuthRungUsed, SftpConnectError> {
    let mut secret: Option<StoredCredentials> = offered.map(|secret| StoredCredentials {
        username: params.username.clone(),
        secret,
    });
    let mut anything_offered = false;

    for rung in ladder(params) {
        let attempt = match rung {
            AuthRung::Agent => try_agent(session, params).await?,
            AuthRung::KeyFile(path) => {
                let passphrase = stored_secret(&mut secret, params, host).await;
                try_key_file(session, params, &path, passphrase.as_deref()).await?
            }
            AuthRung::Password => match stored_secret(&mut secret, params, host).await {
                Some(password) => try_password(session, params, &password).await?,
                None => Attempt::NotOffered,
            },
            AuthRung::KeyboardInteractive => match stored_secret(&mut secret, params, host).await {
                Some(password) => try_keyboard_interactive(session, params, &password).await?,
                None => Attempt::NotOffered,
            },
        };
        match attempt {
            Attempt::Accepted(used) => return Ok(used),
            Attempt::Refused => anything_offered = true,
            Attempt::NotOffered => {}
        }
    }

    // Nothing was ever offered: every rung had no credential behind it, so this
    // is a sign-in the user hasn't done rather than one the server turned down.
    if anything_offered {
        Err(SftpConnectError::AuthenticationRejected)
    } else {
        Err(SftpConnectError::NeedsCredentials)
    }
}

/// The stored secret for this account, read at most once per dial.
///
/// ❗ The store may block on a Keychain prompt, so it goes to a blocking task.
/// ❌ Never held past the session it builds: it lives in this function's caller
/// and dies with the dial.
async fn stored_secret(
    cache: &mut Option<StoredCredentials>,
    params: &SftpConnectionParams,
    host: &VolumeHost,
) -> Option<String> {
    if cache.is_none() {
        let host = host.clone();
        let service = params.credential_service();
        let scope = params.username.clone();
        *cache = tokio::task::spawn_blocking(move || host.credentials().credentials(&service, Some(&scope)))
            .await
            .ok()
            .flatten();
    }
    cache.as_ref().map(|stored| stored.secret.clone())
}

async fn try_agent(
    session: &mut client::Handle<TrustHandler>,
    params: &SftpConnectionParams,
) -> Result<Attempt, SftpConnectError> {
    let Ok(mut agent) = russh::keys::agent::client::AgentClient::connect_env().await else {
        // No agent running is not a failure; it's a rung that isn't there.
        return Ok(Attempt::NotOffered);
    };
    let Ok(identities) = agent.request_identities().await else {
        return Ok(Attempt::NotOffered);
    };
    let mut offered = false;
    for identity in identities {
        // ❌ Certificates aren't offered: validating one needs the CA half of
        // host trust, which this backend deliberately doesn't do.
        let AgentIdentity::PublicKey { key, .. } = identity else {
            continue;
        };
        let hash_alg = rsa_hash_alg(key.algorithm());
        offered = true;
        let result = session
            .authenticate_publickey_with(params.username.clone(), key, hash_alg, &mut agent)
            .await;
        if matches!(result, Ok(AuthResult::Success)) {
            return Ok(Attempt::Accepted(AuthRungUsed::Agent));
        }
    }
    // An agent holding no identities is the same as no agent: a vanished socket
    // and a removed key both look like this, and neither is a rejection.
    Ok(if offered { Attempt::Refused } else { Attempt::NotOffered })
}

async fn try_key_file(
    session: &mut client::Handle<TrustHandler>,
    params: &SftpConnectionParams,
    path: &std::path::Path,
    passphrase: Option<&str>,
) -> Result<Attempt, SftpConnectError> {
    // Tried unlocked first so an unencrypted key never reaches for a secret it
    // doesn't need, which keeps its reconnect policy honest.
    let (key, passphrase_protected) = match russh::keys::load_secret_key(path, None) {
        Ok(key) => (key, false),
        // An unreadable or wrongly-unlocked key file is a rung with nothing
        // behind it, not a server saying no.
        Err(_) => match passphrase.and_then(|p| russh::keys::load_secret_key(path, Some(p)).ok()) {
            Some(key) => (key, true),
            None => return Ok(Attempt::NotOffered),
        },
    };
    let hash_alg = rsa_hash_alg(key.algorithm());
    let result = session
        .authenticate_publickey(
            params.username.clone(),
            PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
        )
        .await
        .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
    Ok(match result {
        AuthResult::Success => Attempt::Accepted(AuthRungUsed::KeyFile { passphrase_protected }),
        AuthResult::Failure { .. } => Attempt::Refused,
    })
}

async fn try_password(
    session: &mut client::Handle<TrustHandler>,
    params: &SftpConnectionParams,
    password: &str,
) -> Result<Attempt, SftpConnectError> {
    let result = session
        .authenticate_password(params.username.clone(), password)
        .await
        .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
    Ok(match result {
        AuthResult::Success => Attempt::Accepted(AuthRungUsed::Password),
        AuthResult::Failure { .. } => Attempt::Refused,
    })
}

/// The non-interactive half of keyboard-interactive: a server that asks exactly
/// one hidden question is asking for the password, which is the common
/// `PasswordAuthentication no` + `KbdInteractiveAuthentication yes` shape.
///
/// Anything longer is real 2FA and needs a human, so it stops here rather than
/// guessing and burning an attempt.
async fn try_keyboard_interactive(
    session: &mut client::Handle<TrustHandler>,
    params: &SftpConnectionParams,
    password: &str,
) -> Result<Attempt, SftpConnectError> {
    let mut response = session
        .authenticate_keyboard_interactive_start(params.username.clone(), None)
        .await
        .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
    loop {
        match response {
            KeyboardInteractiveAuthResponse::Success => {
                return Ok(Attempt::Accepted(AuthRungUsed::KeyboardInteractive));
            }
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(Attempt::Refused),
            KeyboardInteractiveAuthResponse::InfoRequest { ref prompts, .. } => {
                let answers = match prompts.len() {
                    0 => Vec::new(),
                    1 => vec![password.to_string()],
                    // Real 2FA. Guessing burns an attempt and can lock an
                    // account, so this stops and waits for a human.
                    _ => return Ok(Attempt::NotOffered),
                };
                response = session
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
            }
        }
    }
}

/// RSA keys need an explicit modern hash; every other type ignores the argument.
///
/// SHA-512 rather than the crate's `None` default, which maps to the legacy
/// SHA-1 `ssh-rsa` that OpenSSH has refused since 8.8.
fn rsa_hash_alg(algorithm: Algorithm) -> Option<HashAlg> {
    algorithm.is_rsa().then_some(HashAlg::Sha512)
}

/// The `russh` handler, which exists for exactly one callback: deciding whether
/// the key the server presented is one we trust.
struct TrustHandler {
    host: String,
    port: u16,
    volume_host: VolumeHost,
    known_hosts: KnownHostsFile,
    /// What the server presented and what we made of it, so the dial can turn a
    /// refusal into a typed prompt instead of a transport error.
    seen: Arc<Mutex<Option<(PresentedHostKey, HostKeyDecision)>>>,
}

impl Handler for TrustHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        let key = presented(server_public_key);
        let decision = trust::decide(
            self.volume_host.host_keys(),
            &self.known_hosts,
            &self.host,
            self.port,
            &key,
        );
        log::debug!(target: "volume", "sftp host key for {}:{} is {decision:?}", self.host, self.port);
        *self.seen.lock_ignore_poison() = Some((key, decision));
        Ok(decision == HostKeyDecision::Trusted)
    }
}

/// The handler for a probe: read the key, refuse the connection, go no further.
///
/// ❌ It never consults the trust store. Answering `true` for a key that happens
/// to be trusted would carry the probe into authentication, which is the one
/// thing a "what key does this server hold?" question must not cost.
struct ProbeHandler {
    seen: Arc<Mutex<Option<PresentedHostKey>>>,
}

impl Handler for ProbeHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        *self.seen.lock_ignore_poison() = Some(presented(server_public_key));
        Ok(false)
    }
}

/// The three forms a trust decision needs, out of one `ssh_key::PublicKey`.
fn presented(key: &PublicKey) -> PresentedHostKey {
    // `to_openssh` writes `<keytype> <base64>[ comment]`, and the base64 field is
    // exactly what `known_hosts` stores, so comparing it needs no key parsing
    // further down.
    let openssh = key.to_openssh().unwrap_or_default();
    let blob = openssh.split_whitespace().nth(1).unwrap_or_default();
    PresentedHostKey::new(
        key.algorithm().as_str(),
        blob,
        key.fingerprint(HashAlg::Sha256).to_string(),
    )
}

#[cfg(test)]
#[path = "transport_test.rs"]
mod transport_test;
