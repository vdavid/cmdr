//! The SSH transport: the ONE module that names `russh`.
//!
//! ❗ Keep it that way. `russh` shipped eight breaking minors in eight months
//! (`docs/notes/sftp-crate-evaluation-2026-08-22.md`), and the whole point of
//! this boundary is that a bump is one file's problem. Everything above works in
//! [`SshConnection`], [`crate::trust::PresentedHostKey`], and
//! [`crate::auth::AuthRung`], none of which mention an SSH type.
//!
//! ## Two hazards this module is shaped around
//!
//! **A cancelled connect panics inside `openssh-sftp-client`.** `tasks.rs:215`
//! does `tx.send(extensions).unwrap()`, which panics in a spawned task if the
//! `Sftp::new` future was dropped before the server's hello arrived — that is,
//! on any timed-out or abandoned connect (`openssh-sftp-client` 0.15.7, read
//! 2026-08-22; upstream issue #153 covers the same `unwrap`). So `Sftp::new`
//! runs in a task of its OWN and the timeout is applied to the join handle:
//! timing out drops the HANDLE, the future still runs to completion, and its
//! result is discarded. ❌ Never wrap `Sftp::new` in `tokio::time::timeout`
//! directly.
//!
//! **`Sftp::close()` never returns over a `russh` channel.** It awaits a read
//! task that only ends at reader EOF, which a channel doesn't give until it's
//! closed. Dropping [`SshConnection`] is the clean shutdown: the engine's own
//! drop orders it and both tasks exit. ❌ There is no `close()` call in this
//! crate, and adding one hangs `disconnect_sftp_volume` forever.

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

use crate::auth::{AuthRung, AuthRungUsed, ladder};
use crate::errors::SftpConnectError;
use crate::known_hosts::KnownHostsFile;
use crate::trust::{self, HostKeyDecision, PresentedHostKey};
use crate::volume::SftpConnectionParams;

/// How long the TCP connect, key exchange, and authentication get together.
/// Generous enough for a satellite link, short enough that a black-holed address
/// doesn't hold a pane hostage.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// How long the SFTP subsystem gets to answer its hello once the SSH session is
/// up. A server with no `sftp-server` installed simply never answers.
const SUBSYSTEM_TIMEOUT: Duration = Duration::from_secs(15);

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
/// ❗ Run this to completion rather than dropping it mid-flight: see the hazards
/// at the top of this module. `crate::volume::connect_sftp_volume` is what
/// enforces that, by running it inside a task.
pub async fn dial(params: SftpConnectionParams, host: VolumeHost) -> Result<DialOutcome, SftpConnectError> {
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

    let dialed = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        client::connect(config, (params.host.as_str(), params.port), handler),
    )
    .await;

    let mut session = match dialed {
        Err(_elapsed) => return Err(SftpConnectError::TimedOut),
        Ok(Ok(session)) => session,
        Ok(Err(e)) => {
            // A refused key is the likeliest reason a connect fails once the TCP
            // socket is up, and it's the one with a typed answer. `seen` is what
            // the handler left behind; anything else is a transport problem.
            return match seen.lock_ignore_poison().take() {
                Some((key, HostKeyDecision::Revoked)) => Err(SftpConnectError::HostKeyRevoked {
                    algorithm: key.algorithm,
                    fingerprint: key.fingerprint,
                }),
                Some((key, HostKeyDecision::Unknown)) => Ok(DialOutcome::NeedsHostKeyApproval(prompt(
                    &params,
                    key,
                    HostKeyPromptKind::Unknown,
                ))),
                Some((key, HostKeyDecision::Changed)) => Ok(DialOutcome::NeedsHostKeyApproval(prompt(
                    &params,
                    key,
                    HostKeyPromptKind::Changed,
                ))),
                _ => Err(SftpConnectError::Unreachable(e.to_string())),
            };
        }
    };

    let rung = tokio::time::timeout(HANDSHAKE_TIMEOUT, authenticate(&mut session, &params, &host))
        .await
        .map_err(|_elapsed| SftpConnectError::TimedOut)??;

    let sftp = open_sftp_subsystem(&session, &host).await?;
    Ok(DialOutcome::Connected {
        connection: SshConnection { sftp, _ssh: session },
        rung,
    })
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

/// Records `key` as trusted for `(host, port)`, so the next dial is silent.
///
/// The approval flow's second half. ❗ The caller re-verifies that this is still
/// the key the server presents before calling; recording a fingerprint a user
/// approved minutes ago against whatever answers now is how an approval gets
/// replayed onto a different key.
pub fn approve(host: &VolumeHost, server: &str, port: u16, algorithm: &str, fingerprint: &str) {
    let key = PresentedHostKey::new(algorithm, String::new(), fingerprint);
    trust::record_approval(host.host_keys(), server, port, &key);
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

/// Walks the ladder, stopping at the first rung the server accepts.
async fn authenticate(
    session: &mut client::Handle<TrustHandler>,
    params: &SftpConnectionParams,
    host: &VolumeHost,
) -> Result<AuthRungUsed, SftpConnectError> {
    let mut secret: Option<StoredCredentials> = None;
    let mut anything_offered = false;

    for rung in ladder(params) {
        let attempted = match rung {
            AuthRung::Agent => try_agent(session, params).await?,
            AuthRung::KeyFile(path) => {
                let passphrase = stored_secret(&mut secret, params, host).await;
                try_key_file(session, params, &path, passphrase.as_deref()).await?
            }
            AuthRung::Password => match stored_secret(&mut secret, params, host).await {
                Some(password) => try_password(session, params, &password).await?,
                None => None,
            },
            AuthRung::KeyboardInteractive => match stored_secret(&mut secret, params, host).await {
                Some(password) => try_keyboard_interactive(session, params, &password).await?,
                None => None,
            },
        };
        if let Some(used) = attempted {
            return Ok(used);
        }
        anything_offered = true;
    }

    // Nothing was even offered: every rung that needs a secret had none, so this
    // is a sign-in problem rather than a rejection.
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
) -> Result<Option<AuthRungUsed>, SftpConnectError> {
    let Ok(mut agent) = russh::keys::agent::client::AgentClient::connect_env().await else {
        // No agent running is not a failure; it's a rung that isn't there.
        return Ok(None);
    };
    let Ok(identities) = agent.request_identities().await else {
        return Ok(None);
    };
    for identity in identities {
        // ❌ Certificates aren't offered: validating one needs the CA half of
        // host trust, which this backend deliberately doesn't do.
        let AgentIdentity::PublicKey { key, .. } = identity else {
            continue;
        };
        let hash_alg = rsa_hash_alg(key.algorithm());
        let result = session
            .authenticate_publickey_with(params.username.clone(), key, hash_alg, &mut agent)
            .await;
        if matches!(result, Ok(AuthResult::Success)) {
            return Ok(Some(AuthRungUsed::Agent));
        }
    }
    Ok(None)
}

async fn try_key_file(
    session: &mut client::Handle<TrustHandler>,
    params: &SftpConnectionParams,
    path: &std::path::Path,
    passphrase: Option<&str>,
) -> Result<Option<AuthRungUsed>, SftpConnectError> {
    // Tried unlocked first so an unencrypted key never reaches for a secret it
    // doesn't need, which keeps its reconnect policy honest.
    let (key, passphrase_protected) = match russh::keys::load_secret_key(path, None) {
        Ok(key) => (key, false),
        Err(_) => match passphrase.and_then(|p| russh::keys::load_secret_key(path, Some(p)).ok()) {
            Some(key) => (key, true),
            None => return Ok(None),
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
    Ok(matches!(result, AuthResult::Success).then_some(AuthRungUsed::KeyFile { passphrase_protected }))
}

async fn try_password(
    session: &mut client::Handle<TrustHandler>,
    params: &SftpConnectionParams,
    password: &str,
) -> Result<Option<AuthRungUsed>, SftpConnectError> {
    let result = session
        .authenticate_password(params.username.clone(), password)
        .await
        .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
    Ok(matches!(result, AuthResult::Success).then_some(AuthRungUsed::Password))
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
) -> Result<Option<AuthRungUsed>, SftpConnectError> {
    let mut response = session
        .authenticate_keyboard_interactive_start(params.username.clone(), None)
        .await
        .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
    loop {
        match response {
            KeyboardInteractiveAuthResponse::Success => {
                return Ok(Some(AuthRungUsed::KeyboardInteractive));
            }
            KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(None),
            KeyboardInteractiveAuthResponse::InfoRequest { ref prompts, .. } => {
                let answers = match prompts.len() {
                    0 => Vec::new(),
                    1 => vec![password.to_string()],
                    _ => return Ok(None),
                };
                response = session
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
            }
        }
    }
}

/// Opens the channel, asks for the `sftp` subsystem, and starts the engine.
async fn open_sftp_subsystem(
    session: &client::Handle<TrustHandler>,
    host: &VolumeHost,
) -> Result<Sftp, SftpConnectError> {
    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| SftpConnectError::Transport(e.to_string()))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| SftpConnectError::Transport(e.to_string()))?;

    let (reader, writer) = tokio::io::split(channel.into_stream());
    // ❗ The engine's hello wait runs in a task of its own, and the timeout is on
    // the JOIN HANDLE. Timing out drops the handle, never the future, so the
    // `unwrap` at `openssh-sftp-client`'s `tasks.rs:215` has a receiver to send
    // to whatever we do out here.
    let starting = host.runtime().spawn(Sftp::new(writer, reader, SftpOptions::new()));
    match tokio::time::timeout(SUBSYSTEM_TIMEOUT, starting).await {
        Err(_elapsed) => Err(SftpConnectError::TimedOut),
        Ok(Err(join)) => Err(SftpConnectError::Transport(join.to_string())),
        Ok(Ok(Err(e))) => Err(SftpConnectError::Transport(e.to_string())),
        Ok(Ok(Ok(sftp))) => Ok(sftp),
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
