//! Deciding whether the server on the other end is the one we met last time.
//!
//! SSH has no certificate authority in the web sense. Trust is
//! trust-on-first-use, and the whole security of a connection rests on
//! recognizing the SAME key next time — so this module is where a
//! man-in-the-middle either gets caught or doesn't.
//!
//! It is a pure function over three inputs (Cmdr's own store, the user's
//! `known_hosts`, and the key the server presented) and holds no SSH types, so
//! every cell of the decision table is a unit test with string literals in it.
//! `transport.rs` is the only module that turns a real `ssh_key::PublicKey` into
//! the [`PresentedHostKey`] this takes.
//!
//! The order of consultation, strongest signal first, and why:
//!
//! 1. **`@revoked` in `known_hosts`** — the user (or their admin) was told this
//!    exact key is compromised. Nothing outranks that, and it must never reach
//!    the one-click approval path.
//! 2. **Cmdr's own store** — an approval a human gave in this app, which is
//!    Cmdr's equivalent of `known_hosts` and authoritative for it.
//! 3. **`known_hosts`** — the fallback, so a server the user's terminal already
//!    reaches doesn't ask again.

use cmdr_fs::volume::host::host_keys::{HostKeyVerdict, HostKeys};

use crate::known_hosts::{KnownHostsFile, KnownHostsVerdict};

/// The host key a server presented, in the three forms a trust decision needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentedHostKey {
    /// The SSH key-type name (`ssh-ed25519`, `ssh-rsa`), which is half the store
    /// key: a server may hold several types and present any of them.
    pub algorithm: String,
    /// The base64 key blob, exactly as `known_hosts` spells it. Compared
    /// verbatim, so no key parsing has to happen down here.
    pub blob: String,
    /// The OpenSSH `SHA256:…` fingerprint: what Cmdr's store holds, what a
    /// prompt shows, and what a human checks against `ssh-keygen -lf`.
    pub fingerprint: String,
}

impl PresentedHostKey {
    /// Assembles one from its three parts.
    pub fn new(algorithm: impl Into<String>, blob: impl Into<String>, fingerprint: impl Into<String>) -> Self {
        Self {
            algorithm: algorithm.into(),
            blob: blob.into(),
            fingerprint: fingerprint.into(),
        }
    }
}

/// What to do about the key a server just presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyDecision {
    /// Recognized. Carry on with the connection.
    Trusted,
    /// First contact under this algorithm. A human may approve it.
    Unknown,
    /// We hold a different key for this host under this algorithm. Possibly a
    /// man-in-the-middle, so ❌ never the same one-click path `Unknown` takes.
    Changed,
    /// The key is explicitly revoked. Not approvable at all.
    Revoked,
}

/// The trust decision for `key` presented by `(host, port)`.
pub fn decide(
    trusted: &dyn HostKeys,
    known_hosts: &KnownHostsFile,
    host: &str,
    port: u16,
    key: &PresentedHostKey,
) -> HostKeyDecision {
    let from_file = known_hosts.lookup(host, port, &key.algorithm, &key.blob);
    if from_file == KnownHostsVerdict::Revoked {
        return HostKeyDecision::Revoked;
    }
    match trusted.verdict(host, port, &key.algorithm, &key.fingerprint) {
        HostKeyVerdict::Matches => HostKeyDecision::Trusted,
        HostKeyVerdict::Changed => HostKeyDecision::Changed,
        HostKeyVerdict::Unknown => match from_file {
            KnownHostsVerdict::Matches => HostKeyDecision::Trusted,
            KnownHostsVerdict::Changed => HostKeyDecision::Changed,
            // Already returned above; repeated here so adding a variant to the
            // file's verdict is a compile error rather than a silent `Unknown`.
            KnownHostsVerdict::Revoked => HostKeyDecision::Revoked,
            KnownHostsVerdict::Unknown => HostKeyDecision::Unknown,
        },
    }
}

/// Remembers `key` as the trusted key for `(host, port)`.
///
/// Called only once a human approved it. ❌ Writes to Cmdr's own store and never
/// to `~/.ssh/known_hosts`: that file belongs to `ssh`.
pub fn record_approval(trusted: &dyn HostKeys, host: &str, port: u16, key: &PresentedHostKey) {
    trusted.record(host, port, &key.algorithm, &key.fingerprint);
}

/// The host-key algorithms the transport must pin its negotiation to, or empty
/// on first contact.
///
/// ❗ This is the half that makes [`HostKeyDecision::Changed`] mean something.
/// Keying trust by `(host, port, algorithm)` alone would let an attacker offering
/// a type we hold no entry for land on the UNKNOWN path and collect a one-click
/// approval. Pinning to what's already trusted means a healthy server presents
/// the key we stored, and anything else is a real change. This is what OpenSSH
/// does. Both halves, or neither.
pub fn algorithms_to_pin(
    trusted: &dyn HostKeys,
    known_hosts: &KnownHostsFile,
    host: &str,
    port: u16,
) -> Vec<String> {
    let mut algorithms = trusted.trusted_algorithms(host, port);
    algorithms.extend(known_hosts.algorithms_for(host, port));
    algorithms.sort();
    algorithms.dedup();
    algorithms
}

#[cfg(test)]
#[path = "trust_test.rs"]
mod trust_test;
