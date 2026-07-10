//! Retention enforcement (M4): run the writer's `Prune` on startup and on a
//! periodic timer, driven by the age/size limits from settings (D9/D10).
//!
//! The prune MECHANISM (whole-op pruning, dir GC, size-budget loop, vacuum) lives
//! in [`writer`](super::writer); this module owns the POLICY: when to prune and
//! with which limits. Limits are read fresh each tick
//! ([`crate::settings::load_operation_log_retention_limits`]), so an M6 settings
//! change takes effect on the next tick without a restart. Defaults hold before
//! M6 lands the UI: age = forever, size = 3 GB.

use std::time::Duration;

use tauri::AppHandle;

use super::now_secs;
use super::writer::{OperationLogWriter, PruneRequest};

/// How often retention runs after the startup pass. Retention is cheap when the DB
/// is under budget (the size loop is a no-op) and off the hot path, so a slow
/// cadence is plenty; a heavy-churn user's next tick still catches up.
const RETENTION_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Spawn the retention loop: prune once at startup, then every
/// [`RETENTION_INTERVAL`]. Non-fatal — a failed prune logs and the next tick
/// retries.
pub fn spawn(app: &AppHandle, writer: OperationLogWriter) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            prune_now(&app, &writer);
            tokio::time::sleep(RETENTION_INTERVAL).await;
        }
    });
}

/// Run one retention pass with the current settings-driven limits.
fn prune_now(app: &AppHandle, writer: &OperationLogWriter) {
    let limits = crate::settings::load_operation_log_retention_limits(app);
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
