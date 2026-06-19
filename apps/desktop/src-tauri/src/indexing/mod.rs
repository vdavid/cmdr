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
mod manager;
mod partial_agg;
mod state;
pub mod store;
pub mod writer;

mod memory_watchdog;
mod metadata;
mod pending_sizes;
mod reconciler;
pub(crate) mod scanner;
mod verifier;
pub(crate) mod watcher;

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

pub(crate) use enrichment::{ReadPool, get_read_pool};
pub use enrichment::{enrich_entries_with_index, enrich_entries_with_index_on_volume};
pub(crate) use events::DEBUG_STATS;
pub use events::*;

pub(crate) use state::ROOT_VOLUME_ID;
pub use state::{
    clear_index, force_scan, get_debug_status, get_dir_stats, get_dir_stats_batch, get_status, init, is_active,
    should_auto_start, should_auto_start_indexing, start_indexing, stop_indexing, stop_scan, trigger_verification,
};
