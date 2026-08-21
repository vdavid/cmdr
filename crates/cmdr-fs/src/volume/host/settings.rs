//! The live, user-tunable knobs a backend reads while it runs.
//!
//! ❌ **Connection parameters don't belong here.** A server address, a port, a
//! bucket, a region, a passive-mode flag: those are fixed for a volume's whole
//! life, so they travel through the backend's constructor, typed exactly as that
//! backend wants them. This seam is only for values the user can change WHILE a
//! volume is mounted, which is why they're read per dispatch rather than
//! captured once.
//!
//! It's also deliberately not a settings file reader. The host resolves stored
//! settings, applies its own defaults and clamps, and answers a question. ❌ No
//! backend reads a config file, an environment variable, or a preferences store.

/// The settings namespace a backend reads under.
///
/// One short lowercase word per backend, the crate name without its `cmdr-`
/// prefix: `"smb"`, `"ftp"`, `"s3"`. A backend declares it once as a constant
/// and passes it to every lookup. It's a namespace, not a classification: ❌
/// nothing branches on it, on either side of the seam.
pub type BackendName = &'static str;

/// What the user has tuned for a backend.
///
/// Cmdr answers this from stored settings; a test or a tool gets the built-in
/// defaults (`DefaultBackendSettings`).
pub trait BackendSettings: Send + Sync {
    /// How many operations this backend may have in flight against one volume.
    ///
    /// Read on every batch dispatch, so a change to the setting takes effect on
    /// the next batch without remounting. That's the reason it's a seam call and
    /// not a constructor argument.
    ///
    /// The right number is a property of the server, not of the network: some
    /// servers serialize every request on one connection and go several times
    /// faster with a handful of parallel ones, while others fall over. Hosts
    /// clamp to something sane, and a backend may clamp further to what its
    /// protocol can actually sustain.
    fn max_concurrent_operations(&self, backend: BackendName) -> usize;
}

/// The built-in defaults, for a host that has no stored settings to offer.
///
/// Conservative on purpose: a bench or a tool should behave like a cautious
/// default install, not like whatever the last person tuned their NAS to.
pub(super) struct DefaultBackendSettings;

/// What `DefaultBackendSettings` answers. Enough parallelism to beat a
/// strictly-serial dispatch, low enough that no server is stressed by it.
const DEFAULT_MAX_CONCURRENT_OPERATIONS: usize = 4;

impl BackendSettings for DefaultBackendSettings {
    fn max_concurrent_operations(&self, _backend: BackendName) -> usize {
        DEFAULT_MAX_CONCURRENT_OPERATIONS
    }
}
