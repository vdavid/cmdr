//! Which TLS certificates a person told this machine to trust although macOS
//! doesn't, for an HTTP backend talking to a NAS with a self-signed one.
//!
//! The same trust-on-first-use shape as `host_keys`, for the same reason: a
//! home NAS has no certificate authority behind it, so the only thing that can
//! say "this is the box I met last time" is recognizing the SAME certificate
//! next time. The store outlives any one volume and is the user's to inspect
//! and clear, so it's the app's, and it arrives through here.
//!
//! ## One pin per server, compared exactly
//!
//! Keyed by `(host, port)`, one SHA-256 fingerprint each. A backend asks
//! [`TrustedCertificates::pinned`] every time it builds a client (a connect AND
//! every reconnect), and trusts a certificate the OS refused only when its
//! fingerprint IS the pin. ❗ A pin never widens trust beyond that one
//! certificate: a different one from the same server is the "it changed"
//! alarm, never a quiet yes.
//!
//! ❗ **No `record` here, unlike `HostKeys`.** The app writes the pin itself,
//! with the fingerprint a person read off the prompt. Recording exactly what was
//! shown can't be replayed against a certificate nobody saw, since nothing but
//! that fingerprint is ever trusted, so there's no server re-check to do from a
//! backend first.

/// The TLS certificates this machine trusts beyond the OS's own store.
///
/// Cmdr answers this from a durably-written store in its data directory; a test
/// or a tool trusts nothing (`NoTrustedCertificates`).
pub trait TrustedCertificates: Send + Sync {
    /// The SHA-256 fingerprint pinned for `(host, port)`, spelled the way
    /// `cmdr_fs::volume::tls::CertificatePrompt::fingerprint` is (uppercase hex
    /// pairs joined by colons), or `None` when nothing is pinned.
    fn pinned(&self, host: &str, port: u16) -> Option<String>;
}

/// Nothing is pinned.
///
/// ❌ Deliberately NOT "trust everything": a detached host is what a bench, a
/// tool, and half the tests run under, and a double that accepted any
/// certificate is how a man-in-the-middle regression ships green. A test that
/// needs a pin uses `InMemoryCertificates`.
pub(super) struct NoTrustedCertificates;

impl TrustedCertificates for NoTrustedCertificates {
    fn pinned(&self, _host: &str, _port: u16) -> Option<String> {
        None
    }
}

#[cfg(any(test, feature = "testing"))]
pub use in_memory::InMemoryCertificates;

#[cfg(any(test, feature = "testing"))]
mod in_memory {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::TrustedCertificates;
    use crate::ignore_poison::IgnorePoison;

    /// A [`TrustedCertificates`] store in a `HashMap`, so an approval flow can
    /// be driven end to end: pin, then dial again.
    #[derive(Default)]
    pub struct InMemoryCertificates {
        pins: Mutex<HashMap<(String, u16), String>>,
    }

    impl InMemoryCertificates {
        /// An empty store: every server is first contact.
        pub fn new() -> Self {
            Self::default()
        }

        /// Pins `fingerprint` for `(host, port)`, as if a person had approved it.
        pub fn pin(&self, host: &str, port: u16, fingerprint: &str) {
            self.pins
                .lock_ignore_poison()
                .insert((host.to_string(), port), fingerprint.to_string());
        }
    }

    impl TrustedCertificates for InMemoryCertificates {
        fn pinned(&self, host: &str, port: u16) -> Option<String> {
            self.pins.lock_ignore_poison().get(&(host.to_string(), port)).cloned()
        }
    }
}
