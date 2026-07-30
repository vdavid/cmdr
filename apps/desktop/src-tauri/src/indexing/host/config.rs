//! What the product tells the index to do.
//!
//! ❌ **The index never reads settings, and never resolves the data dir for
//! itself.** Policy belongs to the product: the app owns the settings store, the
//! defaults, the migrations, and the UI that changes them, and hands the index a
//! finished [`IndexConfig`]. That kills a whole class of test setup pain (a test
//! sets a value instead of writing a settings file) and makes "controlled from
//! Cmdr's UI" one concept instead of a scatter of setters.
//!
//! The exception, deliberately: `CMDR_*` environment knobs. Those are developer
//! diagnostics rather than product policy, so they stay `std::env::var` reads
//! inside the index and are documented as such where they're read.
//!
//! ## Applying it
//!
//! [`IndexConfig`] is an INPUT value, not a stored snapshot. [`set_config`] pushes
//! the media half straight into the gate atomics and the network-enrichment config
//! — which stay the internal storage the hot paths read — and keeps only the data
//! dir, which has no other home.
//!
//! ❌ Don't add a stored copy of a value the gate already owns. The media-policy
//! IPC setters live-apply single fields as the user moves a slider, so a second
//! copy here would go stale the moment one of them ran, and "what is the index
//! configured to do" would have two answers.

use std::path::PathBuf;
use std::sync::RwLock;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::media_index::gate::IndexScope;
use crate::media_index::network::config::NetworkEnrichConfig;

/// Everything the index needs from the product.
#[derive(Clone, Debug, Default)]
pub(crate) struct IndexConfig {
    /// Where every index database lives. One directory for the drive index, the
    /// media index, and the importance index, resolved once by the app so dev,
    /// production, and each worktree stay separated.
    pub(crate) data_dir: PathBuf,
    /// The media index's user-controlled policy.
    pub(crate) media: MediaConfig,
}

/// The media index's share of [`IndexConfig`]: what the user turned on, how wide,
/// how hard, and which folders they opted in or out.
#[derive(Clone, Debug)]
pub(crate) struct MediaConfig {
    /// The master toggle. Off by default; everything below is inert while it's off.
    pub(crate) enabled: bool,
    /// How much of a volume is eligible for enrichment.
    pub(crate) scope: IndexScope,
    /// The importance score a folder must reach for its images to enrich under the
    /// by-importance scope.
    pub(crate) importance_threshold: f64,
    /// Concurrent enrichment workers. Clamped to `1..=CPU count` on the way in, so a
    /// hand-edited settings file can't over-provision.
    pub(crate) parallelism: usize,
    /// Whether the CLIP write path and semantic search are on. On unless explicitly
    /// turned off; inert anyway with no model installed.
    pub(crate) semantic_search_enabled: bool,
    /// Per-volume and per-folder opt-ins, overrides, and exclusions for network
    /// enrichment.
    pub(crate) network: NetworkEnrichConfig,
}

impl Default for MediaConfig {
    /// Everything off, at the index's own defaults. What a test binary or a tool
    /// sees when the app hasn't configured anything.
    fn default() -> Self {
        Self {
            enabled: false,
            scope: crate::media_index::gate::DEFAULT_SCOPE,
            importance_threshold: crate::media_index::gate::DEFAULT_IMPORTANCE_THRESHOLD,
            parallelism: crate::media_index::gate::DEFAULT_PARALLELISM,
            semantic_search_enabled: true,
            network: NetworkEnrichConfig::default(),
        }
    }
}

/// Where index databases live. The one piece of [`IndexConfig`] that isn't stored
/// somewhere else already. `RwLock` rather than `OnceLock` so tests can swap it.
static DATA_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Hand the index its configuration, applying the media half to the gate.
///
/// Call at startup, and again whenever a setting the index acts on changes. It's a
/// whole-value replace, not a patch: a partially-applied config is a state nobody
/// can reason about.
pub(crate) fn set_config(config: IndexConfig) {
    use crate::media_index::{gate, network};

    gate::set_enabled(config.media.enabled);
    gate::set_scope(config.media.scope);
    gate::set_importance_threshold(config.media.importance_threshold);
    gate::set_parallelism(config.media.parallelism);
    gate::set_semantic_search_enabled(config.media.semantic_search_enabled);
    network::config::set_config(config.media.network);

    *DATA_DIR.write_ignore_poison() = Some(config.data_dir);
}

/// Where index databases live, or an error naming what's missing.
///
/// Returns `Err` rather than a bogus path when nothing was configured, because
/// writing an index DB to a relative path would scatter databases through the
/// working directory instead of failing where it's noticeable.
pub(crate) fn data_dir() -> Result<PathBuf, DataDirUnset> {
    DATA_DIR.read_ignore_poison().clone().ok_or(DataDirUnset)
}

/// No data directory has been configured, so nothing can be opened on disk.
#[derive(Debug)]
pub(crate) struct DataDirUnset;

impl std::fmt::Display for DataDirUnset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no index data directory configured")
    }
}

/// Point the index at `dir` for the duration of one test, restoring the previous
/// config when the guard drops.
///
/// The slot is process-wide, so hold [`test_lock`] first.
#[cfg(test)]
#[must_use = "the config is restored when the guard drops"]
pub(crate) fn install_data_dir_for_test(dir: impl AsRef<std::path::Path>) -> TestConfigGuard {
    let previous = DATA_DIR.write_ignore_poison().replace(dir.as_ref().to_path_buf());
    TestConfigGuard { previous }
}

/// Restores the previous config on drop, including on a panic.
#[cfg(test)]
pub(crate) struct TestConfigGuard {
    previous: Option<PathBuf>,
}

#[cfg(test)]
impl Drop for TestConfigGuard {
    fn drop(&mut self) {
        *DATA_DIR.write_ignore_poison() = self.previous.take();
    }
}

/// Serializes tests that swap the config. Take it BEFORE [`install_data_dir_for_test`].
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With nothing configured, asking for a data dir fails loudly instead of
    /// handing back `""` and scattering index DBs through the working directory.
    #[test]
    fn an_unset_data_dir_is_an_error_not_an_empty_path() {
        let _serialized = test_lock();
        assert!(data_dir().is_err());
    }

    /// The round trip the whole seam exists for: a value set through the config is
    /// the value the index reads back, with nothing consulting a settings file.
    #[test]
    fn a_configured_value_is_what_the_index_reads() {
        let _serialized = test_lock();
        let _installed = install_data_dir_for_test("/tmp/cmdr-config-round-trip");
        assert_eq!(
            data_dir().expect("configured"),
            PathBuf::from("/tmp/cmdr-config-round-trip")
        );
    }
}
