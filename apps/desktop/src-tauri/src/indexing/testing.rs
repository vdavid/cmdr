//! Everything the index exposes for tests, in one place, behind one feature.
//!
//! ❌ **Not part of the API.** Nothing here is a promise, none of it is
//! documented for callers, and a consumer that reaches for it in production code
//! is doing something the real API should be growing a method for instead.
//!
//! It exists because a test outside the index still has to drive the index: hand
//! it fake volumes, record what it reports, reserve a registry slot, or run a
//! scan against a real share. `#[cfg(test)]` can't serve that — it's set only
//! while a crate compiles its OWN test target, so the moment the index is a
//! dependency, every `cfg(test)` item silently vanishes from its consumers'
//! test builds. A feature is the only gate that crosses the boundary.
//!
//! Turn it on through a **dev-dependency**, never a normal one; that's what keeps
//! it out of shipped builds.

// The public surface a test drives the index through. Grouped by what a test is
// trying to do, because that's how someone arrives here.

/// Swapping the index's host: fake volumes, a recording sink, a controllable
/// work-priority policy, and a temp data directory. Each installs under a guard
/// that restores the previous value on drop; hold [`crate::indexing::handle::test_lock`]
/// first, because the slots are process-wide.
pub mod host {
    pub use crate::indexing::host::config::{TestConfigGuard, install_data_dir_for_test};
    pub use crate::indexing::host::events::{TestSinkGuard, install_for_test as install_event_sink_for_test};
    pub use crate::indexing::host::policy::FakeHostPolicy;
    pub use crate::indexing::host::volumes::{FakeVolumeProvider, TestProviderGuard, install_for_test};
}

/// Watching what the index reports without an app to report to.
pub mod events {
    pub use crate::indexing::events::RecordingSink;
}

/// Reaching the walk, the writer, and the store directly, for a test that
/// exercises a real backend against the index's scanner (the live SMB coverage)
/// or installs a synthetic index database (the benchmarks).
pub mod scan {
    pub use crate::indexing::aggregator::compute_all_aggregates_reported;
    pub use crate::indexing::network_scanner::scan_pace::ScanPacer;
    pub use crate::indexing::network_scanner::scan_volume_via_trait;
    pub use crate::indexing::read::enrichment::{
        enrich_via_parent_id_on, test_install_root_read_pool, test_read_pool_lock, test_uninstall_root_read_pool,
    };
    pub use crate::indexing::scanner::ScanProgress;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub use crate::indexing::transports::smb::watch::{index_relative_path, resolve_and_send_for_test};
    pub use crate::indexing::writer::IndexWriter;
}

/// Claiming a registry slot without running a scan, so a test can assert on what
/// happens to a volume that is mid-initialization.
pub use crate::indexing::lifecycle::state::reserve_initializing_index_for_test;
