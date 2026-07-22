//! Local filesystem change detection and processing: watch the boot disk and
//! keep the index live between full scans.
//!
//! - [`watcher`]: the drive-level watcher (macOS FSEvents via
//!   `cmdr-fsevent-stream`, Linux inotify via `notify`), with event-ID replay.
//! - [`event_loop`]: turns the watcher's stream into index writes — live
//!   processing, cold-start journal replay, post-replay verification, and
//!   removal-storm coalescing.
//! - [`churn_monitor`]: an off-by-default per-subtree churn rollup that hooks
//!   `process_live_batch` (env `CMDR_CHURN_SPIKE`).

pub(crate) mod churn_monitor;
pub(crate) mod event_loop;
pub(crate) mod watcher;
