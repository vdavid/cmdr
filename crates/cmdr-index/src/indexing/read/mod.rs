//! The read side of indexing: serve recursive sizes and status back to the app.
//! Everything here reads via the per-volume `ReadPool` (lock-free thread-local
//! connections), NEVER the lifecycle registry lock.
//!
//! - [`enrichment`]: `ReadPool` + `enrich_entries_with_index[_on_volume]`, the
//!   hot path that populates a listing's recursive sizes.
//! - [`queries`]: the IPC read surface (status + dir-stats); no registry
//!   mutation.
//! - [`expected_totals`]: index-derived write-op progress-bar denominators.
//! - [`pending_sizes`]: the per-directory "size updating" hourglass marked-set.
//! - [`handles`]: the volume-keyed tables the two handles above live in, and the
//!   leaf-lock discipline that keeps this side underneath lifecycle.

pub(crate) mod enrichment;
pub mod expected_totals;
mod handles;
pub(crate) mod pending_sizes;
pub(crate) mod queries;
