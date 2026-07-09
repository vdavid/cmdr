//! The archive-edit operation: runs a zip mutation (`ArchiveMutator`) as a real
//! managed write op, so it inherits the queue, lane admission, pause/resume,
//! cancel, progress/ETA, busy-volumes eject guard, and the `write-settled`
//! contract every other transfer/delete gets.
//!
//! A zip edit is NOT a metadata syscall — it's an O(archive) temp+rename rewrite
//! — so it flows through `manager::spawn_managed` (a progress bar, the parent
//! drive's lane) like copy/delete, NOT the instant path rename/mkdir take for a
//! plain filesystem. The driver is net-new but mirrors the volume-delete branch's
//! shape: a deferred async start owns the op end to end (settle guard, the
//! mutator run on the blocking pool, the terminal event, `on_settled`).
//!
//! ## Module map
//!
//! - [`routing`]: the shared detection/path primitives every route builds on —
//!   the archive-boundary path helpers, the zip-only write guard, the
//!   duplicate-existence oracle, and the instant-op sink builder.
//! - [`engine`]: the single apply chokepoint (`run_managed_edit`, LOCAL vs
//!   REMOTE dispatch), the `PlanError` cancel-vs-fault split, the mutator
//!   control-seam `MutatorHooks`, error mapping, and the post-commit source
//!   deletion.
//! - [`conflicts`]: how a copy/move-into collision resolves (pre-resolved policy
//!   or interactive Stop-mode prompt).
//! - [`copy_into`]: the copy/move INTO a zip flow — route, changeset planning,
//!   and its managed-op driver.
//! - [`move_out`]: the MOVE whose source is inside a zip (extract, then a batch
//!   `{ delete }` on a fully clean extract — all-or-nothing).
//! - [`driver`]: the generic changeset driver (`archive_edit_start`) plus the
//!   thin in-archive delete route that feeds it.
//!
//! ## What crosses the seam
//!
//! The caller hands an [`ArchiveEditRequest`]: the archive path, its parent drive
//! id (source of the lane + the eject-busy id), a resolved `Changeset`, a queue
//! summary, and — for an into-archive MOVE only — the local sources to delete
//! AFTER the edit durably commits (the move invariant: never lose both copies).
//! Conflicts are resolved into the changeset before it reaches here, so the
//! mutator stays deterministic.

mod compress;
mod conflicts;
mod copy_into;
mod driver;
mod engine;
mod move_out;
mod routing;

pub(crate) use compress::compress_start;
pub(crate) use copy_into::route_archive_copy_into;
pub(crate) use driver::{ArchiveEditRequest, archive_edit_start, route_archive_delete};
pub(crate) use move_out::route_archive_move_out;
pub(crate) use routing::{
    archive_inner_exists, ensure_zip_writable, global_tauri_sink, join_inner_path, normalize_inner_path,
};

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod compress_tests;
#[cfg(test)]
mod copy_into_interactive_tests;
#[cfg(test)]
mod copy_into_remote_tests;
#[cfg(test)]
mod copy_into_tests;
#[cfg(test)]
mod driver_tests;
#[cfg(test)]
mod move_out_tests;
#[cfg(test)]
mod routing_tests;
