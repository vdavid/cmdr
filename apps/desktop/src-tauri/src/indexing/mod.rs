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
mod enrichment;
mod event_loop;
mod events;
pub mod expected_totals;
pub mod firmlinks;
pub mod freshness;
pub(crate) mod lifecycle_bus;
mod local_reconcile;
mod manager;
mod network_scan;
mod partial_agg;
mod progress_reporter;
mod queries;
mod routing;
mod scan_completion;
mod state;
pub mod store;
pub mod subsystem_stop;
pub mod writer;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod local_external_index;
mod memory_watchdog;
mod metadata;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod mtp_index;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod mtp_watch;
mod path_prefix;
mod pending_sizes;
mod reconciler;
mod retention;
pub(crate) mod scanner;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_index;
mod smb_watch;
mod system_dirs;
mod verifier;
pub(crate) mod volume_scanner;
pub(crate) mod watcher;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
#[path = "smb_scan_integration_test.rs"]
mod smb_scan_integration_test;

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
// Reconcile rescan: perf guard (ignored bench) + correctness regression tests.
#[cfg(test)]
mod reconcile_bench;
#[cfg(test)]
mod reconcile_correctness;

pub(crate) use enrichment::{ReadPool, get_read_pool, get_read_pool_for};
pub use enrichment::{enrich_entries_with_index, enrich_entries_with_index_on_volume};
#[cfg(test)]
pub(crate) use enrichment::{test_install_root_read_pool, test_read_pool_lock, test_uninstall_root_read_pool};
pub(crate) use events::DEBUG_STATS;
pub use events::*;

pub use queries::{
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
    clear_index, disable_drive_index_persist_intent, force_scan, init, is_active, should_auto_start,
    should_auto_start_indexing, start_indexing, stop_indexing, stop_scan, trigger_verification,
};
pub use subsystem_stop::register_subsystem_stop_hook;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use smb_index::SmbIndexGateReason;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use smb_index::{on_smb_overflow, on_smb_watcher_died, resume_smb_index_if_enabled, start_indexing_for_smb};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use smb_watch::{apply_smb_change, discard_buffered_changes, replay_buffered_changes};

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use local_external_index::{LocalExternalEnable, start_indexing_for_local_external};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use mtp_index::{on_mtp_device_disconnected, start_indexing_for_mtp};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use mtp_watch::{
    MtpUpsert, apply_mtp_added_or_changed, apply_mtp_removed, buffer_mtp_handle_if_scanning,
    discard_buffered_mtp_changes, replay_buffered_mtp_changes,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use state::registered_mtp_volume_ids_for_device;
