//! Which SSH host keys this machine already trusts, for a backend that has to
//! decide whether the server on the other end is the one it met last time.
//!
//! SSH has no certificate authority in the web sense: trust is
//! trust-on-first-use, and the whole security of the protocol rests on
//! recognizing the SAME key next time. A backend can't own that store — it
//! outlives any one volume, it's the user's to inspect and clear, and writing it
//! durably is the app's business — so it arrives through here.
//!
//! ## The lookup is keyed by algorithm, and the pin is the other half
//!
//! A healthy server may hold several host keys (an ed25519 and an rsa, say) and
//! present whichever the negotiation lands on. So a store keyed by host alone
//! reports a *changed key* on a perfectly healthy server, which trains people to
//! click through the one alarm that matters.
//!
//! Keying by `(host, port, algorithm)` fixes that half. ❗ On its own it opens a
//! worse hole: an attacker who offers ed25519 where we hold an rsa entry lands on
//! the UNKNOWN path and collects a one-click approval. That's what
//! [`HostKeys::trusted_algorithms`] is for — a backend pins its negotiation to the
//! algorithms already trusted for that host, so a healthy server presents the key
//! we stored and any mismatch is a real change. Both halves, or neither.
//!
//! ## Fingerprints, not keys
//!
//! The seam speaks in the OpenSSH fingerprint string (`SHA256:…`) rather than a
//! parsed key type, so no SSH crate reaches this far down and the value the store
//! holds is the same one a human compares against `ssh-keygen -lf`.

/// What the trusted-host store knows about a key a server just presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyVerdict {
    /// We hold a key for this host, port, and algorithm, and it's this one.
    Matches,
    /// We hold a key for this host, port, and algorithm, and it is a DIFFERENT
    /// one. Possible man-in-the-middle: ❌ never let this take the same
    /// one-click path a first-seen key takes.
    Changed,
    /// Nothing is stored for this host, port, and algorithm.
    Unknown,
}

/// The SSH host keys this machine trusts.
///
/// Cmdr answers this from a durably-written store in its data directory; a test
/// or a tool trusts nothing (`NoHostKeys`).
pub trait HostKeys: Send + Sync {
    /// What the store knows about `fingerprint` for `(host, port, algorithm)`.
    ///
    /// `algorithm` is the SSH key-type name (`ssh-ed25519`, `rsa-sha2-512`), and
    /// `fingerprint` is the OpenSSH `SHA256:…` form.
    fn verdict(&self, host: &str, port: u16, algorithm: &str, fingerprint: &str) -> HostKeyVerdict;

    /// Every key algorithm already trusted for `(host, port)`.
    ///
    /// A backend pins its key-exchange preferences to exactly these, so a server
    /// can't move itself onto the unknown path by offering a type we hold no
    /// entry for. An empty answer means first contact, where there's nothing to
    /// pin to and every algorithm is fair.
    fn trusted_algorithms(&self, host: &str, port: u16) -> Vec<String>;

    /// Remembers `fingerprint` as the key for `(host, port, algorithm)`,
    /// replacing whatever was there.
    ///
    /// Called only after a human approved it. A store that can't write logs and
    /// carries on: the session in hand still works, and the only thing lost is
    /// "silent next time".
    fn record(&self, host: &str, port: u16, algorithm: &str, fingerprint: &str);
}

/// Nothing is trusted, and nothing is remembered.
///
/// ❌ Deliberately NOT "trust everything": a detached host is what a bench, a
/// tool, and half the tests run under, and a double that accepted any key is how
/// a man-in-the-middle regression ships green. A test that needs approvals to
/// stick uses `InMemoryHostKeys`, which actually remembers.
pub(super) struct NoHostKeys;

impl HostKeys for NoHostKeys {
    fn verdict(&self, _host: &str, _port: u16, _algorithm: &str, _fingerprint: &str) -> HostKeyVerdict {
        HostKeyVerdict::Unknown
    }

    fn trusted_algorithms(&self, _host: &str, _port: u16) -> Vec<String> {
        Vec::new()
    }

    fn record(&self, _host: &str, _port: u16, _algorithm: &str, _fingerprint: &str) {}
}

#[cfg(any(test, feature = "testing"))]
pub use in_memory::InMemoryHostKeys;

#[cfg(any(test, feature = "testing"))]
mod in_memory {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::{HostKeyVerdict, HostKeys};
    use crate::ignore_poison::IgnorePoison;

    /// A [`HostKeys`] store in a `HashMap` that actually
    /// remembers, so an approval flow can be driven end to end.
    ///
    /// The detached host answers trust-nothing, which is right for it and wrong
    /// for a fixture: a no-op `record` leaves an approve-then-reconnect harness
    /// looping forever on "unknown → approve → still unknown".
    #[derive(Default)]
    pub struct InMemoryHostKeys {
        entries: Mutex<HashMap<(String, u16, String), String>>,
    }

    impl InMemoryHostKeys {
        /// An empty store: every host is first contact.
        pub fn new() -> Self {
            Self::default()
        }

        /// Pre-seeds one entry, as if the user had approved this key earlier.
        #[must_use]
        pub fn with_entry(self, host: &str, port: u16, algorithm: &str, fingerprint: &str) -> Self {
            self.record(host, port, algorithm, fingerprint);
            self
        }
    }

    impl HostKeys for InMemoryHostKeys {
        fn verdict(&self, host: &str, port: u16, algorithm: &str, fingerprint: &str) -> HostKeyVerdict {
            match self
                .entries
                .lock_ignore_poison()
                .get(&(host.to_string(), port, algorithm.to_string()))
            {
                Some(stored) if stored == fingerprint => HostKeyVerdict::Matches,
                Some(_) => HostKeyVerdict::Changed,
                None => HostKeyVerdict::Unknown,
            }
        }

        fn trusted_algorithms(&self, host: &str, port: u16) -> Vec<String> {
            let mut algorithms: Vec<String> = self
                .entries
                .lock_ignore_poison()
                .keys()
                .filter(|(h, p, _)| h == host && *p == port)
                .map(|(_, _, algorithm)| algorithm.clone())
                .collect();
            // Sorted so a pinned preference list is deterministic; a `HashMap`
            // iteration order would reshuffle the algorithms offered per run.
            algorithms.sort();
            algorithms
        }

        fn record(&self, host: &str, port: u16, algorithm: &str, fingerprint: &str) {
            self.entries.lock_ignore_poison().insert(
                (host.to_string(), port, algorithm.to_string()),
                fingerprint.to_string(),
            );
        }
    }
}
