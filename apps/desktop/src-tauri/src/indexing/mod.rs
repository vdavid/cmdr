//! Drive indexing module.
//!
//! Background-indexes local volumes into a per-volume SQLite database,
//! tracking every file and directory with recursive size aggregates.
//! Design history is in git (former `docs/specs/drive-indexing/`).
//!
//! `mod.rs` is a thin public-API facade. The state machine (the global
//! `INDEXING` mutex, `IndexPhase` enum, phase transitions, and the
//! `IndexManager` + `ReadPool` bootstrap) lives in [`state`].

pub mod aggregator;
mod events;
mod failure;
pub(crate) mod watch;
// Public API surface; real homes are watch/watcher.rs and watch/event_loop.rs (churn_monitor is reached via watch::).
pub(crate) use watch::{event_loop, watcher};
pub(crate) mod read;
// Public API surface; real homes are read/expected_totals.rs, read/enrichment.rs, read/pending_sizes.rs.
pub use read::expected_totals;
pub(crate) use read::{enrichment, pending_sizes};
pub(crate) mod paths;
// Public API surface; real homes are paths/firmlinks.rs and paths/routing.rs.
pub use paths::firmlinks;
pub(crate) use paths::routing;
pub mod freshness;
pub(crate) mod lifecycle_bus;
mod manager;
mod network_scan;
mod scan_completion;
mod state;
pub mod store;
pub(crate) mod resources;
pub mod writer;

mod metadata;
pub(crate) mod scanner;
pub(crate) mod network_scanner;
pub(crate) mod reconcile;
// Public API surface; real homes are reconcile/{reconciler,local_reconcile,verifier}.rs.
pub(crate) use reconcile::{local_reconcile, reconciler, verifier};
pub(crate) mod transports;

// Synthetic FAT32/exFAT disk-image fixtures for external-drive indexing tests.
// macOS-only (hdiutil); see the module and DETAILS § "Testing external drives".
#[cfg(all(test, target_os = "macos"))]
mod external_drive_fixture;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod stress_test_helpers;
#[cfg(test)]
mod stress_tests_concurrency;
#[cfg(test)]
mod stress_tests_lifecycle;
#[cfg(test)]
mod stress_tests_partial_aggregation;

pub(crate) use read::enrichment::{ReadPool, get_read_pool, get_read_pool_for};
pub use read::enrichment::{enrich_entries_with_index, enrich_entries_with_index_on_volume};
#[cfg(test)]
pub(crate) use read::enrichment::{test_install_root_read_pool, test_read_pool_lock, test_uninstall_root_read_pool};
pub(crate) use events::DEBUG_STATS;
pub use events::*;

pub(crate) use failure::IndexFailureSignal;
pub use read::queries::{
    get_debug_status, get_dir_stats, get_dir_stats_batch, get_status, get_volume_index_status,
    get_volume_index_status_for_path, list_dir_children,
};
pub(crate) use routing::{IndexPathSpace, index_read_path, volume_id_for_local_path};
pub(crate) use state::ROOT_VOLUME_ID;
pub(crate) use state::get_freshness;
#[cfg(test)]
pub(crate) use state::reserve_initializing_index_for_test;
pub(crate) use state::{IndexVolumeKind, all_registered_volume_ids, ready_volumes_with_kind, volume_kind};
pub use state::{
    clear_index, disable_drive_index_persist_intent, force_scan, init, is_active, is_failed, should_auto_start,
    should_auto_start_indexing, start_indexing, stop_indexing, stop_scan, trigger_verification,
};
pub use store::IndexFailure;
pub use resources::subsystem_stop::register_subsystem_stop_hook;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use transports::smb::index::SmbIndexGateReason;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::smb::index::{
    on_smb_overflow, on_smb_watcher_died, resume_smb_index_if_enabled, start_indexing_for_smb,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::smb::watch::{apply_smb_change, discard_buffered_changes, replay_buffered_changes};

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::local_external::index::{LocalExternalEnable, start_indexing_for_local_external};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::mtp::index::{on_mtp_watch_continuity_lost, start_indexing_for_mtp};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::mtp::watch::{
    MtpUpsert, apply_mtp_added_or_changed, apply_mtp_removed, buffer_mtp_handle_if_scanning,
    discard_buffered_mtp_changes, replay_buffered_mtp_changes,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use state::registered_mtp_volume_ids_for_device;
