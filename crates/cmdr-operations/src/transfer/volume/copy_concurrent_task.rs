//! One top-level source, copied end to end, as the concurrent driver's window
//! runs it.
//!
//! Everything the task needs arrives owned on [`CopyTask`], so the future is
//! `'static` and spawns on its own (the probe's task-local scope is set inside
//! it, in [`run_copy_task`]). The two result payloads carry back what the phase
//! runner's cleanup and rollback have to know, and
//! `CopyTaskFailure::cleanup_temp`, which says whether the failed path may be
//! cleaned at all, is the most data-loss-sensitive line in this directory: read
//! its doc before touching the arms that build it.
//!
//! Conflict resolution is NOT here. It runs on the driver before a task is built
//! (`copy_concurrent_source.rs`), so a Stop prompt holds back its source and
//! every later one instead of copying into a destination the user is still
//! being asked about. Pinned by
//! `copy_concurrent_tests.rs::a_top_level_conflict_prompt_holds_back_its_source_and_every_later_one`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::super::super::conflict::ApplyToAll;
use super::super::super::event_sinks::OperationEventSink;
use super::super::super::journal;
use super::super::super::ledger::WrittenFile;
use super::super::super::state::WriteOperationState;
use super::super::super::types::VolumeCopyConfig;
use super::super::transfer_driver::LeafProgressLedger;
use super::super::transfer_probe::{CURRENT_TASK_PROBE, TaskProbeHandle};
use super::displaced_destination::DisplacedLedger;
use super::merge_ctx::{CreatedPaths, FileWindow, MergeCtx, MergeProbe};
use super::preflight::{SourceFileFacts, SourceHint};
use super::strategy::{LandingName, Replaces, copy_single_path, failed_write_leaves_ours_at, staging_for};
use super::transfer_error::PathedVolumeError;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::Volume;

/// Success payload for one concurrent copy task.
///
/// `recorded_path` is the top-level landed path (the original's own name after
/// an overwrite): the entry the result handler drops from `in_flight_partials`,
/// and what it records for rollback for a top-level file source.
/// `created_files` / `created_dirs` carry the per-file destinations and newly-created subdirectories from a
/// DIRECTORY source's recursive copy, so rollback removes exactly what the op
/// wrote into a (possibly pre-existing, merged) dest directory and never the
/// directory root.
pub(super) struct CopyTaskSuccess {
    pub(super) recorded_path: PathBuf,
    pub(super) source_is_dir: bool,
    pub(super) bytes: u64,
    /// Whether this source replaced any existing dest file (a top-level file→file
    /// swap, or a deep-merge child overwrite). Feeds the operation-log
    /// eligibility: a copy that overwrote isn't rollbackable (the original is gone).
    pub(super) overwrote: bool,
    pub(super) created_files: Vec<WrittenFile>,
    pub(super) created_dirs: Vec<PathBuf>,
    /// The landed files' own modified times, read back here in the task (so the
    /// driver loop never awaits the destination) when the operation journals.
    pub(super) landed: journal::LandedTimes,
    /// The top-level source this task copied, and how many children a deep
    /// merge skipped in its subtree. `skipped_count == 0` means the whole
    /// subtree landed durably, so the out-of-zip move op may drop it from the
    /// archive; any deep skip keeps it.
    pub(super) source_path: PathBuf,
    pub(super) skipped_count: usize,
    pub(super) skipped_bytes: u64,
}

/// Failure payload for one concurrent copy task.
///
/// `failed_path` is the in-flight partial entry to remove from
/// `in_flight_partials` (the dest item path). ❌ It is NOT the path to report:
/// it names where the partial sits on the DESTINATION, which for a directory
/// source is the dest dir root. `failure` carries the item the walker actually
/// failed on (deep inside the subtree, not the top-level item the user
/// selected) and the side that refused, and is the only one of the two a user
/// can act on. Keep them separate. `cleanup_temp` says whether `failed_path`
/// can hold a partial of OURS (`failed_write_leaves_ours_at`): `false` for a
/// plain staged write, whose temp is the write's own to abandon, and ❗ for a
/// swap, whose `failed_path` is the user's original.
///
/// `source_is_dir` plus `created_files` / `created_dirs` carry the per-file
/// rollback ledger for a DIRECTORY source interrupted mid-stream. Without it the
/// post-loop cleanup/rollback would fall back to recursively deleting the dest
/// directory ROOT — which on a merge destroys pre-existing dest-only files. With
/// it, the partials are cleaned per-file and the newly-created dirs pruned
/// empty-only, so a merged dir holding a sentinel survives.
///
/// `source_path` and `overwrote` are here for the same reason the success
/// payload carries them: the children this task DID finish are committed files
/// at the destination, so they get journaled exactly like a completed source's.
/// An interrupted operation is still a ledger of what it wrote.
pub(super) struct CopyTaskFailure {
    pub(super) failed_path: PathBuf,
    /// What the user is told: the failing item, its side, and, for a finalize
    /// that deleted the original and then couldn't land the new bytes, where
    /// the user's only complete copy went (`new_data_at`).
    pub(super) failure: PathedVolumeError,
    pub(super) source_path: PathBuf,
    pub(super) cleanup_temp: bool,
    pub(super) source_is_dir: bool,
    pub(super) overwrote: bool,
    pub(super) created_files: Vec<WrittenFile>,
    pub(super) created_dirs: Vec<PathBuf>,
}

/// Everything one task owns for the life of one top-level source.
///
/// Owned, not borrowed: it is what makes [`run_copy_task`]'s future `'static`,
/// so it spawns into the driver's `JoinSet` with no lifetime and the phases around
/// it stay free to hold `&mut` on the window.
pub(super) struct CopyTask {
    pub(super) events: Arc<dyn OperationEventSink>,
    pub(super) operation_id: String,
    pub(super) state: Arc<WriteOperationState>,
    pub(super) source_volume: Arc<dyn Volume>,
    pub(super) dest_volume: Arc<dyn Volume>,
    /// Per-task merge context inputs: deep file clashes inside a directory
    /// source landing on a merged dest honor the file policy, sharing the
    /// op-wide apply-to-all latch with every other task and the top-level
    /// dispatch.
    pub(super) config: VolumeCopyConfig,
    pub(super) apply_to_all: Arc<std::sync::Mutex<ApplyToAll>>,
    pub(super) source_path: PathBuf,
    pub(super) source_is_dir: bool,
    /// What the preflight scan already learned about a FILE source. A top-level
    /// dispatch carries no mode here (the scan counts bytes), so a file landing
    /// on a local destination resolves one after its bytes cross.
    pub(super) source_facts: SourceFileFacts,
    /// Where this task's file lands: the original's own name under
    /// [`Replaces::Swap`].
    pub(super) dest_path: PathBuf,
    /// What the write does to a file at the name: under `Swap`, its landing
    /// replaces the original after a successful write.
    pub(super) replaces: Replaces,
    /// Whether conflict resolution PICKED `dest_path` (a `Rename` pick, an
    /// Overwrite that cleared it) rather than it being the plain
    /// `dest_root.join(name)` nothing has looked at. The landing needs it to
    /// tell its own placeholder from a file nobody answered for
    /// (`staged_write.rs::LandingName`).
    pub(super) dest_name_claimed: bool,
    /// The operation's one file-copy window, shared with every merge walker.
    pub(super) window: FileWindow,
    /// The operation's ledger of cross-type asides, which a deep clash inside
    /// this source adds to.
    pub(super) displaced: Arc<DisplacedLedger>,
    /// This source's rollback ledger: the files it streams and the dirs it newly
    /// creates. Shared with the driver, which keeps a handle so a task it
    /// ABANDONS at the cancel-drain deadline still hands over what it landed
    /// (`copy_concurrent.rs::ConcurrentDriver::finish`).
    pub(super) created: Arc<CreatedPaths>,
    /// The in-flight table plus this source's row, so every leaf of a directory
    /// source's subtree opens a row numbered under it.
    pub(super) merge_probe: Option<MergeProbe>,
    /// This task's in-flight-table row, held for the task's whole life.
    pub(super) task_probe: Option<TaskProbeHandle>,
    /// The operation's byte ledger. This task mints its own source view from
    /// it, so its leaves and every sibling task's add up instead of competing
    /// for one shared high-water slot.
    pub(super) leaf_ledger: Arc<LeafProgressLedger>,
    pub(super) last_progress: Arc<std::sync::Mutex<Instant>>,
}

/// Streams one top-level source item end to end.
///
/// The `Err` payload's `cleanup_temp` is the whole reason this returns a struct
/// rather than a `VolumeError`: see [`CopyTaskFailure`]. Boxed, since it carries
/// the per-file rollback ledger and would otherwise size every `Ok` too.
pub(super) async fn run_copy_task(task: CopyTask) -> Result<CopyTaskSuccess, Box<CopyTaskFailure>> {
    let CopyTask {
        events,
        operation_id,
        state,
        source_volume,
        dest_volume,
        config,
        apply_to_all,
        source_path,
        source_is_dir,
        source_facts,
        dest_path,
        replaces,
        dest_name_claimed,
        window,
        displaced,
        created,
        merge_probe,
        // Held for the task's whole life; dropping it (completion, abort, panic)
        // removes the row from the in-flight table.
        task_probe,
        leaf_ledger,
        last_progress,
    } = task;

    let probe_handle = task_probe.as_ref().map(TaskProbeHandle::probe);
    // This task's view of the operation's byte ledger. Every file it streams —
    // the top-level one, or each leaf of a directory source's subtree — mints
    // its own handle from this, so the bar shows the SUM of what is moving
    // rather than whichever file happens to be furthest along. The throttle
    // clock is the operation's, shared with every sibling task, so the event
    // rate the user sees is the operation's and not each task's.
    let source_progress = leaf_ledger.for_source(&source_path, Arc::clone(&last_progress));
    // Deep merge children are never top-level sources, so the resolver never
    // keys into per-source hints for them — an empty map is correct.
    let merge_hints: HashMap<PathBuf, SourceHint> = HashMap::new();
    let merge_ctx = MergeCtx {
        events: &*events,
        operation_id: &operation_id,
        config: &config,
        state: &state,
        apply_to_all: &apply_to_all,
        source_hints: &merge_hints,
        window: window.clone(),
        displaced: &displaced,
        probe: merge_probe,
        locked: None,
    };
    // A top-level FILE source IS a leaf, so it takes its slot from the same
    // op-wide window a directory's children take theirs from. Otherwise a batch
    // mixing files and folders would carry `W` file tasks PLUS the walkers' `W`
    // leaves — twice the width the user's setting asked for, on one connection.
    //
    // ❌ A DIRECTORY source takes none. A walker that held a slot while waiting
    // for its own children to get theirs would deadlock the operation outright at
    // width 1.
    let _leaf_permit = if source_is_dir { None } else { window.reserve().await };
    let staging = staging_for(
        &replaces,
        if dest_name_claimed {
            LandingName::ClaimedByTheCaller
        } else {
            LandingName::ExpectedFree
        },
    );
    let copy_fut = copy_single_path(
        &source_volume,
        &source_path,
        Some(source_is_dir),
        source_facts,
        &dest_volume,
        &dest_path,
        &state,
        &created,
        &source_progress,
        Some(&merge_ctx),
        staging,
    );
    // Bind this task's probe as a task-local for the whole copy, so
    // `stream_pipe_file` and `CheckpointStream` can record their phases without
    // threading a handle through every signature.
    let result = match probe_handle {
        Some(probe) => CURRENT_TASK_PROBE.scope(probe, copy_fut).await,
        None => copy_fut.await,
    };
    let created_files = std::mem::take(&mut *created.files.lock_ignore_poison());
    let created_dirs = std::mem::take(&mut *created.dirs.lock_ignore_poison());
    // Deep-merge skips in this source's subtree; `0` means the whole subtree
    // landed durably (the move op may drop it from the archive).
    let task_skipped_count = created.skipped_file_count();
    let task_skipped_bytes = created.skipped_byte_count();
    match result {
        // Landed at `dest_path`, which names the original for a top-level
        // overwrite (a swap's landing ran inside the write).
        Ok(bytes) => Ok(CopyTaskSuccess {
            landed: if state.journal_volumes.is_some() {
                journal::LandedTimes::read_back(&*dest_volume, &dest_path, source_is_dir, &created_files).await
            } else {
                journal::LandedTimes::default()
            },
            recorded_path: dest_path,
            source_is_dir,
            bytes,
            // Overwrote iff a top-level file→file Overwrite replaced a file (a
            // swap, or an in-place write on a whole-publishing destination), OR
            // a deep-merge child replaced an existing dest file. Feeds the
            // operation-log eligibility (a copy that overwrote can't roll back).
            overwrote: replaces.overwrites() || created.any_overwrote(),
            created_files,
            created_dirs,
            source_path,
            skipped_count: task_skipped_count,
            skipped_bytes: task_skipped_bytes,
        }),
        // A failure (incl. a mid-stream cancel, and a landing that couldn't
        // finish): clean `dest_path` (`cleanup_temp = true`) ❗ only when it can
        // hold OUR partial. A plain staged write's final name never does, and
        // cleaning it deleted whatever another writer put there; a swap's names
        // the user's original (`failed_write_leaves_ours_at`). A failed
        // top-level write replaced nothing it can be blamed for: a swap that got
        // past its delete told the journal itself (`staged_write.rs::land`). For
        // a DIRECTORY source, carry the per-file ledger so the result handler
        // records the individual partials instead of the dir root — the
        // post-loop must never recursively delete a merged dest dir and destroy
        // pre-existing dest-only files.
        Err(e) => Err(Box::new(CopyTaskFailure {
            failed_path: dest_path,
            failure: e,
            source_path,
            cleanup_temp: failed_write_leaves_ours_at(staging),
            source_is_dir,
            overwrote: created.any_overwrote(),
            created_files,
            created_dirs,
        })),
    }
}
