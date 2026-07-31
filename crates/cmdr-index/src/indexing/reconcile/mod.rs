//! Keep the index matching disk after the initial scan. Three mechanisms, all
//! sharing the cost-budget / skip / honest-stale discipline:
//!
//! - [`reconciler`]: event-triggered reconcile (the live FSEvents path,
//!   `MustScanSubDirs` rescan, per-subtree throttle, depth-split routing).
//! - [`local_reconcile`]: the full LOCAL rescan-in-place (non-destructive,
//!   hang-tolerant `GuardedReader`, cost budget).
//! - [`verifier`]: per-navigation `read_dir` diff that corrects the directory
//!   the user is looking at.

pub(crate) mod local_reconcile;
pub(crate) mod reconciler;
pub(crate) mod verifier;

// Reconcile rescan: perf guard (ignored bench) + correctness regression tests.
#[cfg(test)]
mod reconcile_bench;
#[cfg(test)]
mod reconcile_correctness;
