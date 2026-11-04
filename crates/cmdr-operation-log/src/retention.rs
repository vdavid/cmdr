//! Retention enforcement: run the writer's `Prune` on startup and on a
//! periodic timer, driven by the age/size limits the host supplies.
//!
//! The prune MECHANISM (whole-op pruning, dir GC, size-budget loop, vacuum) lives
//! in [`writer`](super::writer); this module owns the POLICY: when to prune and
//! with which limits. Limits are read fresh each tick through the host's
//! [`RetentionLimitsSource`], so a retention settings change takes effect on the
//! next tick without a restart.

use std::sync::Arc;
use std::time::Duration;

use super::now_secs;
use super::writer::{OperationLogWriter, PruneRequest};

/// How often retention runs after the startup pass. Retention is cheap when the DB
/// is under budget (the size loop is a no-op) and off the hot path, so a slow
/// cadence is plenty; a heavy-churn user's next tick still catches up.
const RETENTION_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// The limits one retention pass prunes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionLimits {
    /// Prune finished ops older than this many seconds. `None` keeps them forever.
    pub max_age_secs: Option<u64>,
    /// Prune the oldest ops until the DB fits this many bytes. `None` is unlimited.
    pub max_size_bytes: Option<u64>,
}

/// Where each tick reads its limits from. The host reads its settings here, live,
/// so a change applies on the next tick.
pub type RetentionLimitsSource = Arc<dyn Fn() -> RetentionLimits + Send + Sync>;

/// Spawn the retention loop on `runtime`: prune once at startup, then every
/// [`RETENTION_INTERVAL`]. Non-fatal: a failed prune logs and the next tick
/// retries.
pub(super) fn spawn(runtime: &cmdr_fs::rt::Runtime, writer: OperationLogWriter, limits: RetentionLimitsSource) {
    runtime.spawn(async move {
        loop {
            prune_now(&writer, limits());
            crate::rt::time::sleep(RETENTION_INTERVAL).await;
        }
    });
}

/// Run one retention pass with `limits`.
fn prune_now(writer: &OperationLogWriter, limits: RetentionLimits) {
    let request = PruneRequest {
        max_age_secs: limits.max_age_secs,
        max_size_bytes: limits.max_size_bytes,
        now_secs: now_secs().max(0) as u64,
        // Age-only pruning still reclaims a bounded slice; a size budget reclaims
        // fully. Either way freed pages return to the OS over time.
        vacuum: true,
    };
    if let Err(e) = writer.prune(request) {
        log::warn!(target: "operation_log", "retention prune not enqueued: {e}");
    }
}
