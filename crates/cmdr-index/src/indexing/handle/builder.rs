//! Building the one [`Index`] a process has.
//!
//! Everything the index needs from its host arrives here, once, and the handle
//! that comes back is the only way to reach the index's API. Nothing the host
//! answers is read from a global by the caller: it's supplied.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::runtime::Handle;

use super::Index;
use crate::indexing::events::{EventSink, NoopEventSink};
use crate::indexing::host::config::IndexConfig;
use crate::indexing::host::policy::{AlwaysClear, HostPolicy};
use crate::indexing::host::volumes::{NoVolumes, VolumeProvider};

/// Why [`IndexBuilder::build`] couldn't hand back a handle.
#[derive(Debug)]
pub enum IndexBuildError {
    /// This process already has an index. The subsystems below the handle carry
    /// process-wide state, so a second `build` would hand back something that
    /// silently shared the first one's registry, databases, and threads rather
    /// than being an independent index.
    ///
    /// Honest for a single-instance system, and this variant disappears the day
    /// that state moves inside the handle.
    AlreadyBuilt,
}

impl std::fmt::Display for IndexBuildError {
    /// Diagnostic text for logs; the app renders its own words.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyBuilt => f.write_str("this process already has an index"),
        }
    }
}

impl std::error::Error for IndexBuildError {}

/// Claimed by the first successful [`IndexBuilder::build`], so a second one can
/// report [`IndexBuildError::AlreadyBuilt`] instead of pretending.
static BUILT: AtomicBool = AtomicBool::new(false);

/// Collects what the host answers for, then hands back the [`Index`].
///
/// Every seam has a default that degrades rather than panicking (nothing
/// mounted, nothing competing for resources, events dropped), so a test or a
/// tool can build a handle and supply only what it cares about. The shipped app
/// supplies all of them.
#[derive(Default)]
pub struct IndexBuilder {
    data_dir: Option<PathBuf>,
    config: Option<IndexConfig>,
    volumes: Option<Arc<dyn VolumeProvider>>,
    events: Option<Arc<dyn EventSink>>,
    policy: Option<Arc<dyn HostPolicy>>,
    runtime: Option<Handle>,
    indexing_enabled: Option<Option<bool>>,
}

impl IndexBuilder {
    /// Where every index database lives. One directory for the drive index, the
    /// media index, and the importance index.
    #[must_use]
    pub fn data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(dir.into());
        self
    }

    /// The full configuration, when the host has media policy to hand over too.
    /// Supersedes [`data_dir`](Self::data_dir).
    #[must_use]
    pub fn config(mut self, config: IndexConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Which volumes exist, where they're mounted, and what kind of storage they
    /// are. Without one, the index behaves as if nothing were mounted.
    #[must_use]
    pub fn volumes(mut self, volumes: Arc<dyn VolumeProvider>) -> Self {
        self.volumes = Some(volumes);
        self
    }

    /// Where the index reports what it's doing. Without one, events are dropped.
    #[must_use]
    pub fn events(mut self, events: Arc<dyn EventSink>) -> Self {
        self.events = Some(events);
        self
    }

    /// Whether background work may run right now. Without one, the index assumes
    /// nothing is competing with it.
    #[must_use]
    pub fn host(mut self, policy: Arc<dyn HostPolicy>) -> Self {
        self.policy = Some(policy);
        self
    }

    /// The tokio runtime background work spawns onto. Sharing the host's runtime
    /// is what keeps one thread pool, and with it the priority story that lets
    /// indexing run inside an interactive app. Without one, the index lazily
    /// builds a multi-threaded runtime of its own.
    #[must_use]
    pub fn runtime(mut self, runtime: Handle) -> Self {
        self.runtime = Some(runtime);
        self
    }

    /// The user's master drive-indexing switch, as stored. `None` means "never
    /// chosen", which resolves to on. Off means no volume indexes, whatever its
    /// own per-drive setting says.
    #[must_use]
    pub fn indexing_enabled(mut self, enabled: Option<bool>) -> Self {
        self.indexing_enabled = Some(enabled);
        self
    }

    /// Build this process's index.
    ///
    /// Installs everything supplied, prepares the read path, and hands back the
    /// handle. Call once; a second call reports
    /// [`AlreadyBuilt`](IndexBuildError::AlreadyBuilt).
    pub fn build(self) -> Result<Index, IndexBuildError> {
        if BUILT.swap(true, Ordering::SeqCst) {
            return Err(IndexBuildError::AlreadyBuilt);
        }
        let index = self.install();
        crate::indexing::lifecycle::state::init();
        Ok(index)
    }

    /// Push every supplied seam into the process-wide slots the subsystems still
    /// read, and materialize the handle's own fields.
    ///
    /// The two halves exist because the handle is mid-migration: call sites that
    /// have been moved read the handle's fields, and everything below them still
    /// resolves a slot. Installing both keeps the two consistent, and the slot
    /// half disappears as the internals get threaded.
    fn install(self) -> Index {
        use crate::indexing::host;

        if let Some(runtime) = self.runtime.clone()
            && host::runtime::set_runtime(runtime).is_err()
        {
            log::warn!(target: "indexing", "index runtime was already set; keeping the first one");
        }
        if let Some(events) = self.events.clone()
            && host::events::set_event_sink(events).is_err()
        {
            log::warn!(target: "indexing", "index event sink was already set; keeping the first one");
        }
        if let Some(volumes) = self.volumes.clone()
            && host::volumes::set_volume_provider(volumes).is_err()
        {
            log::warn!(target: "indexing", "index volume provider was already set; keeping the first one");
        }
        if let Some(policy) = self.policy.clone()
            && host::policy::set_host_policy(policy).is_err()
        {
            log::warn!(target: "indexing", "index host policy was already set; keeping the first one");
        }

        let config = self.config.clone().unwrap_or_else(|| IndexConfig {
            data_dir: self.data_dir.clone().unwrap_or_default(),
            ..IndexConfig::default()
        });
        if !config.data_dir.as_os_str().is_empty() {
            host::config::set_config(config.clone());
        }
        if let Some(enabled) = self.indexing_enabled {
            crate::indexing::lifecycle::master::set_master_enabled(
                crate::indexing::lifecycle::state::should_auto_start(enabled),
            );
        }

        Index {
            events: self.events.unwrap_or_else(NoopEventSink::shared),
            volumes: self.volumes.unwrap_or_else(|| Arc::new(NoVolumes)),
            policy: self.policy.unwrap_or_else(|| Arc::new(AlwaysClear)),
            data_dir: config.data_dir,
        }
    }

    /// Build a handle for one test WITHOUT claiming the process's index, and
    /// restore every slot this touched when the guard drops.
    ///
    /// The claim is deliberately not taken: a test's handle is a handle to the
    /// same process-wide index, not a second one, and taking it would make the
    /// next test in the binary fail to build.
    ///
    /// Hold [`test_lock`](super::test_lock) first — the slots are process-wide.
    #[cfg(any(test, feature = "testing"))]
    #[must_use = "the seams are restored when the guard drops"]
    pub fn install_for_test(self) -> (Index, TestInstallGuard) {
        use crate::indexing::host;

        let guard = TestInstallGuard {
            volumes: self.volumes.clone().map(host::volumes::install_for_test),
            config: self
                .data_dir
                .clone()
                .or_else(|| self.config.as_ref().map(|c| c.data_dir.clone()))
                .map(host::config::install_data_dir_for_test),
            events: self.events.clone().map(host::events::install_for_test),
            // The master switch is a process-wide atomic that `install` below is
            // about to write. Captured HERE, before that write, so a test that
            // turns drive indexing off doesn't leave it off for whichever test
            // runs next in the same binary.
            master: self.indexing_enabled.map(|enabled| {
                crate::indexing::lifecycle::master::install_for_test(
                    crate::indexing::lifecycle::state::should_auto_start(enabled),
                )
            }),
        };
        let mut builder = self;
        // The runtime slot is a one-shot and a test binary's fallback is already
        // the right one, so a test handle never installs one.
        builder.runtime = None;
        (builder.install(), guard)
    }

    /// Release the process's index claim for one test, restoring it on drop, so a
    /// test can exercise [`build`](Self::build) itself.
    #[cfg(test)]
    #[must_use = "the claim is restored when the guard drops"]
    pub(crate) fn release_build_claim_for_test() -> BuildClaimGuard {
        BuildClaimGuard {
            previous: BUILT.swap(false, Ordering::SeqCst),
        }
    }
}

/// Restores every seam a test handle installed, including on a panic.
///
/// Holding it is the whole job: each field is another guard whose `Drop` puts a
/// slot back.
#[cfg(any(test, feature = "testing"))]
#[allow(dead_code, reason = "each field restores a seam on drop; none is ever read")]
pub struct TestInstallGuard {
    volumes: Option<crate::indexing::host::volumes::TestProviderGuard>,
    config: Option<crate::indexing::host::config::TestConfigGuard>,
    events: Option<crate::indexing::host::events::TestSinkGuard>,
    master: Option<crate::indexing::lifecycle::master::MasterSwitchGuard>,
}

/// Restores the process's index claim on drop.
#[cfg(test)]
pub(crate) struct BuildClaimGuard {
    previous: bool,
}

#[cfg(test)]
impl Drop for BuildClaimGuard {
    fn drop(&mut self) {
        BUILT.store(self.previous, Ordering::SeqCst);
    }
}
