//! Reading the user's `~/.ssh/known_hosts`, as a fallback and never as a writer.
//!
//! Someone whose terminal already reaches a server shouldn't have to approve its
//! key a second time in Cmdr, so this file is consulted when Cmdr's own store has
//! nothing. ❌ Nothing here writes: `ssh` owns that file, and a file manager
//! appending to it is a surprise nobody asked for. Cmdr's approvals go to Cmdr's
//! own store.
//!
//! # Why this parser rather than russh's
//!
//! `russh::keys::known_hosts` splits every line as `hosts keytype blob` and
//! parses the third field as a key. A line carrying a `@revoked` or
//! `@cert-authority` marker shifts every field by one, so the parse fails and the
//! `?` takes the WHOLE lookup down with it — one certificate-using host in the
//! file and no host in it is readable (verified by reading
//! `russh-0.62.7/src/keys/known_hosts.rs`, 2026-08-22). Both markers are exactly
//! the cases that must not be misread, so the reader is ours.

use std::path::{Path, PathBuf};

use data_encoding::BASE64;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

/// What a `known_hosts` file has to say about a key a server presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownHostsVerdict {
    /// A plain entry for this host and algorithm holds exactly this key.
    Matches,
    /// A plain entry for this host and algorithm holds a DIFFERENT key.
    Changed,
    /// A `@revoked` entry names this exact key: it is known to be compromised.
    Revoked,
    /// The file says nothing about this host under this algorithm.
    Unknown,
}

/// The marker OpenSSH allows in front of a `known_hosts` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    /// `@cert-authority`: the blob is a CA that SIGNS host keys, not a host key.
    /// Cmdr doesn't do certificate host auth, so such an entry neither matches
    /// nor mismatches — but it has to be RECOGNIZED, or its blob reads as the
    /// host's own key and every first connection becomes a "key changed" alarm.
    CertAuthority,
    /// `@revoked`: this exact key is known to be compromised.
    Revoked,
}

/// One usable line of a `known_hosts` file.
#[derive(Debug, Clone)]
struct Entry {
    marker: Option<Marker>,
    /// The comma-separated host patterns, verbatim.
    patterns: String,
    /// The SSH key-type name (`ssh-ed25519`, `ssh-rsa`).
    algorithm: String,
    /// The base64 key blob, exactly as the file spells it.
    blob: String,
}

/// A parsed `known_hosts` file. An unreadable or absent file is an empty one:
/// the fallback silently has nothing to add, which is the same outcome as a file
/// that doesn't mention this host.
#[derive(Debug, Clone, Default)]
pub struct KnownHostsFile {
    entries: Vec<Entry>,
}

impl KnownHostsFile {
    /// Parses `text`, skipping every line it can't use.
    ///
    /// Skipping rather than failing is the point: one line this reader doesn't
    /// understand must not cost the user every other host in the file.
    pub fn parse(text: &str) -> Self {
        let mut entries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let Some(first) = fields.next() else { continue };
            let (marker, patterns) = match first {
                "@cert-authority" => (Some(Marker::CertAuthority), fields.next()),
                "@revoked" => (Some(Marker::Revoked), fields.next()),
                // An unrecognized marker is a line whose shape we can't trust, so
                // it's skipped rather than guessed at.
                _ if first.starts_with('@') => continue,
                _ => (None, Some(first)),
            };
            let (Some(patterns), Some(algorithm), Some(blob)) = (patterns, fields.next(), fields.next()) else {
                continue;
            };
            entries.push(Entry {
                marker,
                patterns: patterns.to_string(),
                algorithm: algorithm.to_string(),
                blob: blob.to_string(),
            });
        }
        Self { entries }
    }

    /// Reads the user's own `~/.ssh/known_hosts`, or an empty file when there
    /// isn't one.
    pub fn read_default() -> Self {
        match default_path() {
            Some(path) => Self::read_path(&path),
            None => Self::default(),
        }
    }

    /// Reads a `known_hosts` file at `path`, or an empty one when it can't be
    /// read.
    pub fn read_path(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|text| Self::parse(&text)).unwrap_or_default()
    }

    /// What this file says about `blob` presented for `(host, port)` under
    /// `algorithm`.
    pub fn lookup(&self, host: &str, port: u16, algorithm: &str, blob: &str) -> KnownHostsVerdict {
        let addressed = address_forms(host, port);
        let mut verdict = KnownHostsVerdict::Unknown;
        for entry in self.matching(&addressed) {
            match entry.marker {
                // Checked against the blob rather than the algorithm, because a
                // revocation names one specific key.
                Some(Marker::Revoked) if entry.blob == blob => return KnownHostsVerdict::Revoked,
                Some(_) => continue,
                None if entry.algorithm != algorithm => continue,
                None if entry.blob == blob => verdict = KnownHostsVerdict::Matches,
                // Keep looking: a second entry for the same host and algorithm
                // may hold the key we were shown, and OpenSSH treats any match as
                // a match. A `Matches` already found outranks this.
                None if verdict == KnownHostsVerdict::Unknown => verdict = KnownHostsVerdict::Changed,
                None => {}
            }
        }
        verdict
    }

    /// Every key algorithm this file holds a plain entry for at `(host, port)`,
    /// sorted.
    ///
    /// Feeds the negotiation pin, so a host trusted only through this file is
    /// pinned as tightly as one Cmdr's own store knows.
    pub fn algorithms_for(&self, host: &str, port: u16) -> Vec<String> {
        let addressed = address_forms(host, port);
        let mut algorithms: Vec<String> = self
            .matching(&addressed)
            .filter(|entry| entry.marker.is_none())
            .map(|entry| entry.algorithm.clone())
            .collect();
        algorithms.sort();
        algorithms.dedup();
        algorithms
    }

    fn matching<'a>(&'a self, addressed: &'a [String]) -> impl Iterator<Item = &'a Entry> {
        self.entries
            .iter()
            .filter(move |entry| addressed.iter().any(|form| matches_patterns(form, &entry.patterns)))
    }
}

/// How OpenSSH spells a host in `known_hosts`: bare at port 22, bracketed
/// otherwise. Both forms are tried, because a file written by hand (or by an
/// older `ssh`) may carry either.
fn address_forms(host: &str, port: u16) -> Vec<String> {
    if port == 22 {
        vec![host.to_string(), format!("[{host}]:{port}")]
    } else {
        vec![format!("[{host}]:{port}")]
    }
}

/// Whether `address` is named by a comma-separated pattern list, plain or hashed.
fn matches_patterns(address: &str, patterns: &str) -> bool {
    patterns.split(',').any(|pattern| matches_pattern(address, pattern))
}

fn matches_pattern(address: &str, pattern: &str) -> bool {
    if let Some(hashed) = pattern.strip_prefix("|1|") {
        return matches_hashed(address, hashed);
    }
    // ❌ No glob expansion. OpenSSH allows `*` and `?` in a pattern, and a
    // fallback reader that got the expansion subtly wrong would either trust a
    // host it shouldn't or alarm on one it should. An exact match is always safe:
    // the worst it costs is one extra approval.
    if pattern.contains('*') || pattern.contains('?') {
        return false;
    }
    address == pattern
}

/// `|1|<base64 salt>|<base64 HMAC-SHA1 of the address under that salt>`, which is
/// what `HashKnownHosts yes` writes (the Debian and Ubuntu default).
fn matches_hashed(address: &str, hashed: &str) -> bool {
    let Some((salt, digest)) = hashed.split_once('|') else {
        return false;
    };
    let (Ok(salt), Ok(digest)) = (BASE64.decode(salt.as_bytes()), BASE64.decode(digest.as_bytes())) else {
        return false;
    };
    let Ok(mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.chain_update(address.as_bytes()).verify_slice(&digest).is_ok()
}

fn default_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".ssh").join("known_hosts"))
}

#[cfg(test)]
#[path = "known_hosts_test.rs"]
mod known_hosts_test;
