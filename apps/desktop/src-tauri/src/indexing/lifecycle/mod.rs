//! Orchestration: how a per-volume index is born, lives, transitions, and dies.
//!
//! - [`state`]: the `INDEX_REGISTRY` + `IndexPhase` machine + `IndexVolumeKind`
//!   + the `IndexManager`/`ReadPool` bootstrap. The authority for WHICH volumes
//!   are indexed and each volume's lifecycle.
//! - [`manager`]: `IndexManager`, the per-volume coordinator + the LOCAL scan
//!   dispatch. [`network_scan`]: its SMB/MTP `Volume`-trait scan path (a sibling
//!   `impl IndexManager` block). [`scan_completion`]: the post-scan handler.
//! - [`freshness`]: the Fresh/Stale/Scanning transition table.
//! - [`failure`]: the fatal-storage-error Failed state.
//! - [`lifecycle_bus`]: the neutral registration / dirs-changed event bus.

pub(crate) mod failure;
pub mod freshness;
pub(crate) mod lifecycle_bus;
pub(crate) mod manager;
pub(crate) mod network_scan;
pub(crate) mod scan_completion;
pub(crate) mod state;
