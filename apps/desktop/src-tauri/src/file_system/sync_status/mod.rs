//! Cloud sync status (Dropbox, iCloud Drive, Google Drive, …) for the pane's
//! per-row badge, on macOS.
//!
//! The status of one file comes from `probe.rs`. Everything else here exists
//! because that probe ends in a **synchronous XPC call into a File Provider daemon
//! that can block forever**, while the caller is a file pane that re-asks for every
//! visible path several times a second.
//!
//! - `pool.rs`: a long-lived, hard-capped set of 8 MB-stack OS threads. The probe
//!   never runs anywhere else (never rayon, never tokio's blocking pool).
//! - `cache.rs`: per-directory, TTL'd answers, so an unchanged folder is free.
//! - `service.rs`: one batch in flight at a time, cancellable, with a deadline that
//!   bounds the caller's wait and never the work.
//!
//! Design rationale and the incident that produced it: `DETAILS.md`.

mod bench;
mod cache;
mod pool;
mod probe;
mod service;

use cache::Ttls;
use pool::PoolConfig;
use serde::{Deserialize, Serialize};
use service::Service;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

/// Sync status for a file in a cloud-synced folder (Dropbox, iCloud, etc.).
// DEFAULT-OK: the zero value is `Unknown`, which is the ABSENCE of a claim rather than
// one — the variant exists to carry exactly that.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    Synced,
    /// Stub file, content in cloud only.
    OnlineOnly,
    Uploading,
    Downloading,
    /// Not a cloud file or status cannot be determined.
    #[default]
    Unknown,
}

/// Sizing of the thread pool the probe runs on.
///
/// `target_workers` is deliberately far below `available_parallelism()`: the work
/// is XPC latency, not CPU, and the pane asks about a visible range (tens of
/// paths), not a whole folder. `max_workers` is the number of threads this feature
/// can ever cost, including ones lost inside a provider that stopped answering.
const POOL: PoolConfig = PoolConfig {
    name: "cmdr-sync-status",
    target_workers: 4,
    max_workers: 12,
    wedged_after: Duration::from_secs(30),
};

/// Roughly a hundred directories' worth of files. Overflow evicts whole
/// directories, least recently used first.
const CACHE_CAPACITY: usize = 4096;

const TTLS: Ttls = Ttls {
    stable: Duration::from_secs(60),
    transitional: Duration::from_secs(2),
};

static SERVICE: LazyLock<Service> =
    LazyLock::new(|| Service::new(Arc::new(probe::sync_status_for), POOL, CACHE_CAPACITY, TTLS));

/// Sync status for many paths, waiting at most `deadline`.
///
/// The returned bool is "we ran out of time". Timing out abandons the *wait*, not
/// the work: the batch keeps running on the bounded pool and caches what it learns,
/// so a caller that asks again gets those answers without touching the provider.
pub async fn statuses_within(paths: Vec<String>, deadline: Duration) -> (HashMap<String, SyncStatus>, bool) {
    SERVICE.statuses_within(paths, deadline).await
}

/// Sync status for one path, for callers with no async context (the native context
/// menu). Bounded by `deadline`; falls back to [`SyncStatus::Unknown`] rather than
/// blocking the caller on an unresponsive provider.
pub fn status_within_blocking(path: &str, deadline: Duration) -> SyncStatus {
    SERVICE.status_within_blocking(path, deadline)
}

/// Forgets the cached statuses of the files directly inside `dir`. Called from the
/// listing change notification, so a badge follows what the filesystem did.
pub fn invalidate_dir(dir: &Path) {
    SERVICE.invalidate_dir(dir);
}

/// Forgets one file's cached status. For actions that change it without reliably
/// producing an FS event, like evicting a download.
pub fn invalidate_path(path: &Path) {
    SERVICE.invalidate_path(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_status_serializes_to_the_names_the_frontend_expects() {
        assert_eq!(serde_json::to_string(&SyncStatus::Synced).unwrap(), "\"synced\"");
        assert_eq!(
            serde_json::to_string(&SyncStatus::OnlineOnly).unwrap(),
            "\"online_only\""
        );
        assert_eq!(serde_json::to_string(&SyncStatus::Uploading).unwrap(), "\"uploading\"");
        assert_eq!(
            serde_json::to_string(&SyncStatus::Downloading).unwrap(),
            "\"downloading\""
        );
    }
}
