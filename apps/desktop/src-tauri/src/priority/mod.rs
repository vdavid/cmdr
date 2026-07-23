//! Who gets the volume: the per-volume priority signals background work yields to.
//!
//! One transport-generic mechanism (keyed by volume id, nothing SMB- or MTP-specific)
//! with three levels, highest first:
//!
//! 1. **User-interactive work** (browsing, opening files) — [`foreground`]'s activity
//!    timestamps, stamped by the hot listing IPC. Everything below yields to it.
//! 2. **File transfers** (user-initiated write operations: copy, move, delete,
//!    drag-out) — [`transfers`]' per-volume gauge, fed by the write-operation
//!    lifecycle. Transfers yield to (1) via the `Volume` foreground-yield methods
//!    (`file_system/volume/backends/smb/foreground_yield.rs`) and trump all indexing.
//! 3. **Indexing** (drive indexing AND image indexing/enrichment) — lowest. Never
//!    signals; only reads (1) and (2) and stands aside.
//!
//! This module owns the SIGNALS and the pure decisions over them; each background
//! consumer composes them with its own scope and yield shape, at its natural
//! between-units boundary:
//!
//! - **Drive indexing** (`indexing/network_scanner/scan_pace.rs`): per-volume
//!   foreground + per-volume transfers ⇒ drops to a one-listing budget (throttle,
//!   never a stop — forward progress stays structural).
//! - **Image enrichment** (`media_index/scheduler`): app-wide foreground (heavy
//!   ML with no deadline) + per-volume transfers ⇒ pauses the pass and resumes
//!   when clear (`PauseReason::NotIdle` → `PassOutcome::RetryWhenIdle`).
//! - **SMB transfers** (`CheckpointStream`'s auto-yield): per-volume foreground ⇒
//!   parks between chunks. MTP transfers answer the same question from their own
//!   per-device gate (a PTP session has an explicit holder; see
//!   `foreground_yield.rs`'s module docs) — the signal here is time-based because
//!   SMB frames just interleave.
//!
//! Writers are hot paths (one atomic store / small map write); readers poll at
//! their loop boundaries. No scheduler, no queues: signals in, decisions out.

pub mod foreground;
pub mod transfers;
