//! Orchestration: how a per-volume index is born, lives, transitions, and dies.
//!
//! - [`state`]: the `INDEX_REGISTRY`, the `IndexPhase` machine, `IndexVolumeKind`,
//!   and the `IndexManager`/`ReadPool` bootstrap. The authority for WHICH volumes
//!   are indexed and each volume's lifecycle.
//! - [`manager`]: `IndexManager`, the per-volume coordinator + the LOCAL scan
//!   dispatch. [`network_scan`]: its SMB/MTP `Volume`-trait scan path (a sibling
//!   `impl IndexManager` block). [`scan_completion`]: the post-scan handler.
//! - [`freshness`]: the Fresh/Stale/Scanning transition table.
//! - [`failure`]: the fatal-storage-error Failed state.
//! - [`lifecycle_bus`]: the neutral registration / dirs-changed event bus.
//! - [`master`]: the master drive-indexing switch and how it overrides per-drive
//!   intent. Every start/resume path passes through its gate.
//! - [`progress_reporter`]: the 500 ms scan-progress + partial-aggregation pump
//!   both scan paths spawn alongside their scan. [`partial_agg`]: its pure
//!   send-decision and hot-path collection.

pub(crate) mod failure;
pub mod freshness;
pub(crate) mod lifecycle_bus;
pub(crate) mod manager;
pub mod master;
pub(crate) mod network_scan;
pub(crate) mod partial_agg;
pub(crate) mod progress_reporter;
pub(crate) mod scan_completion;
pub(crate) mod state;
