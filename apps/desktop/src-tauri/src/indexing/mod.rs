//! Drive indexing module.
//!
//! Background-indexes local volumes into a per-volume SQLite database,
//! tracking every file and directory with recursive size aggregates.
//! Design history is in git (former `docs/specs/drive-indexing/`).
//!
//! `mod.rs` is a thin public-API facade. The state machine (the global
//! `INDEX_REGISTRY` mutex, `IndexPhase` enum, phase transitions, and the
//! `IndexManager` + `ReadPool` bootstrap) lives in [`lifecycle::state`].

// Area modules. Cross-area references use each module's real path
// (`indexing::lifecycle::state::…`, `indexing::paths::routing::…`); `mod.rs` re-exports only
// the curated public item surface below, never a module alias that would hide where code lives.
pub mod aggregator;
mod events;
pub(crate) mod lifecycle;
mod metadata;
pub(crate) mod network_scanner;
pub(crate) mod paths;
pub(crate) mod read;
pub(crate) mod reconcile;
pub(crate) mod resources;
pub(crate) mod scanner;
pub mod store;
pub(crate) mod transports;
pub(crate) mod watch;
pub mod writer;

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::stress_test_helpers;

pub(crate) use events::DEBUG_STATS;
pub use events::*;
pub(crate) use read::enrichment::{ReadPool, get_read_pool, get_read_pool_for};
pub use read::enrichment::{enrich_entries_with_index, enrich_entries_with_index_on_volume};
#[cfg(test)]
pub(crate) use read::enrichment::{test_install_root_read_pool, test_read_pool_lock, test_uninstall_root_read_pool};

pub(crate) use lifecycle::failure::IndexFailureSignal;
pub(crate) use lifecycle::state::ROOT_VOLUME_ID;
pub(crate) use lifecycle::state::get_freshness;
#[cfg(test)]
pub(crate) use lifecycle::state::reserve_initializing_index_for_test;
pub(crate) use lifecycle::state::{IndexVolumeKind, all_registered_volume_ids, ready_volumes_with_kind, volume_kind};
pub use lifecycle::state::{
    clear_index, disable_drive_index_persist_intent, force_scan, init, is_active, is_failed, should_auto_start,
    should_auto_start_indexing, start_indexing, stop_indexing, stop_scan, trigger_verification,
};
pub(crate) use paths::routing::{IndexPathSpace, index_read_path, volume_id_for_local_path};
pub use read::queries::{
    get_debug_status, get_dir_stats, get_dir_stats_batch, get_status, get_volume_index_status,
    get_volume_index_status_for_path, list_dir_children,
};
pub use resources::subsystem_stop::register_subsystem_stop_hook;
pub use store::IndexFailure;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use transports::smb::index::SmbIndexGateReason;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::smb::index::{
    on_smb_overflow, on_smb_watcher_died, resume_smb_index_if_enabled, start_indexing_for_smb,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::smb::watch::{apply_smb_change, discard_buffered_changes, replay_buffered_changes};

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use lifecycle::state::registered_mtp_volume_ids_for_device;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::local_external::index::{LocalExternalEnable, start_indexing_for_local_external};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::mtp::index::{on_mtp_watch_continuity_lost, start_indexing_for_mtp};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use transports::mtp::watch::{
    MtpUpsert, apply_mtp_added_or_changed, apply_mtp_removed, buffer_mtp_handle_if_scanning,
    discard_buffered_mtp_changes, replay_buffered_mtp_changes,
};
