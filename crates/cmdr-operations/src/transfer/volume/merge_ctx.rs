//! The vocabulary every merge walker and write site shares: the operation's one
//! file-copy window ([`FileWindow`]), the context a merge walk resolves deep
//! clashes through ([`MergeCtx`], [`MergeProbe`]), and the per-source rollback
//! ledger ([`CreatedPaths`]).
//!
//! Kept out of `strategy.rs` and `stream_pipe.rs`, whose job is one source's
//! bytes: these types are what `merge.rs`, both copy drivers, both moves,
//! `naming.rs`, and `sequential_extract.rs` import.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::super::super::conflict::ApplyToAll;
use super::super::super::event_sinks::OperationEventSink;
use super::super::super::ledger::WrittenFile;
use super::super::super::locked_sources::LockedPlan;
use super::super::super::state::WriteOperationState;
use super::super::super::types::VolumeCopyConfig;
use super::super::transfer_probe::{OperationProbe, TaskRow};
use super::displaced_destination::DisplacedLedger;
use super::link_carry::LinkCarry;
use super::preflight::SourceHint;
use super::source_sweep::{SourceLedger, SourceStamp};
use cmdr_fs::ignore_poison::IgnorePoison;

/// How many FILE byte-copies this operation may have in flight at once, across
/// every merge walker at every level AND the concurrent driver's top-level file
/// tasks.
///
/// **ONE per operation.** ❌ Never one per directory level and ❌ never one per
/// top-level source: the concurrent driver already fans out `W` ways over
/// sources, so a `W`-wide window inside each walker would put `W²` files on one
/// connection — 100 at the shipped default of 10, far past the point where
/// throughput starts falling (measured 4-8 useful, degrading past it:
/// `docs/notes/transfer-subtree-concurrency-bench-2026-08-13.md`).
///
/// A width of 1 makes it [`FileWindow::is_serial`], which is what keeps MTP
/// (`max_concurrent_ops() == 1`) walking a subtree strictly one file at a time,
/// with no overlap of any kind — not even a directory create against a live
/// write. That backend's cap is a single USB bulk transport, not a guard-rail.
#[derive(Clone)]
pub(super) struct FileWindow(Option<Arc<tokio::sync::Semaphore>>);

impl FileWindow {
    /// `width` is [`super::copy::transfer_concurrency`]'s answer for the pair —
    /// the same number the top-level driver sizes its window with, ❌ never a
    /// second private constant.
    pub(super) fn new(width: usize) -> Self {
        Self((width > 1).then(|| Arc::new(tokio::sync::Semaphore::new(width))))
    }

    /// A window that never overlaps anything: every leaf runs inline, exactly as
    /// the walk did before it had a window at all. What `merge: None` callers
    /// (the archive scratch pull) and MTP both get.
    pub(super) fn serial() -> Self {
        Self(None)
    }

    pub(super) fn is_serial(&self) -> bool {
        self.0.is_none()
    }

    /// Reserves one of the operation's slots for a leaf about to stream. The
    /// permit rides INSIDE the leaf's future, so the slot comes back the moment
    /// that file finishes, fails, or is dropped.
    ///
    /// ❌ Never hold one across a recursive descent: a walker that held a permit
    /// while waiting for its children to take permits would deadlock the whole
    /// operation at width 1.
    pub(super) async fn reserve(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        match &self.0 {
            // `acquire_owned` errors only on a closed semaphore, and nothing
            // closes this one; a `None` here would merely widen the window, so
            // it can't turn into a hang either way.
            Some(permits) => Arc::clone(permits).acquire_owned().await.ok(),
            None => None,
        }
    }
}

/// Context threaded into the recursive merge walk so each pre-existing level can
/// resolve its clashing children through the same conflict machinery the
/// top-level copy uses (Stop-wait, the apply-to-all latch, conditional reduce,
/// type mismatches), without widening `copy_directory_streaming`'s already-long
/// argument list per item.
///
/// `None` means "no conflict resolution" — the caller is a path that streams a
/// directory into a brand-new destination where nothing can clash (the
/// cross-volume move's copy phase, or a plain non-merging copy). In that case
/// every `create_directory` either succeeds fresh or — if the dest happens to
/// already hold a same-named dir — the walk still merges structurally, but
/// per-child file clashes overwrite blindly (today's behavior for that path).
/// The volume copy/move pipelines pass `Some(_)` so deep clashes honor the
/// user's file policy.
pub(super) struct MergeCtx<'a> {
    pub events: &'a dyn OperationEventSink,
    pub operation_id: &'a str,
    pub config: &'a VolumeCopyConfig,
    /// The operation's shared state — carries the cancel `intent`, the
    /// `conflict_slot` a Stop-mode prompt is answered through, and the
    /// `conflict_dispatch_lock` the resolver uses to serialize the human across
    /// concurrent merges.
    pub state: &'a Arc<WriteOperationState>,
    /// Op-wide apply-to-all latch, shared between the top-level dispatch and
    /// every deep merge level so a "…all" choice applies everywhere. Held only
    /// briefly per resolve (copy out → run the async resolver on the stack local
    /// → store back), mirroring the serial top-level path; the `Cancelled`-safe
    /// serialization of the human is the `conflict_dispatch_lock`'s job, not
    /// this cell's.
    pub apply_to_all: &'a Mutex<ApplyToAll>,
    /// Per-source-path hints from the preflight scan. Deep merge children aren't
    /// top-level sources, so they never have a hint — the resolver falls back to
    /// trait calls for them (the size/mtime annotations come from `get_metadata`
    /// on the Stop path only, bounded by the user's click time).
    pub source_hints: &'a HashMap<PathBuf, SourceHint>,
    /// The operation-wide file-copy window, shared by every walker. See
    /// [`FileWindow`] for why there is exactly one of these per operation.
    pub window: FileWindow,
    /// The operation's ledger of what cross-type Overwrites set aside. A deep
    /// clash hands its aside here, and the operation settles them all when it
    /// ends (`displaced_destination.rs::DisplacedLedger`).
    pub displaced: &'a DisplacedLedger,
    /// The operation's live in-flight table plus the row of the source whose
    /// subtree this walk is, so each leaf a walker overlaps gets its OWN row
    /// (and its own stall-abort token), numbered under that source. `None` in
    /// the tests that register no probe. A leaf runs inside its row's
    /// `CURRENT_TASK_PROBE` scope, which is what keeps the invariant every
    /// `TaskProbe` field assumes — one row, one write attempt — true once a
    /// subtree streams several files at once.
    pub probe: Option<MergeProbe>,
    /// A MOVE's answer about Locked items: a child it skips stays out of the
    /// copy whole, like a conflict Skip, and the sweep keeps it. `None` on a
    /// copy, which never touches an original (`locked_sources.rs`).
    pub locked: Option<&'a LockedPlan>,
    /// Which symlinks this operation lands as symlinks. `None` on a move, which
    /// follows them (`link_carry.rs`).
    pub links: Option<&'a LinkCarry>,
}

/// The in-flight table as a merge walk sees it: the operation's probe, plus the
/// row of the TOP-LEVEL source this walk descends from.
///
/// The two travel together because a leaf row can't be opened without both: the
/// table to open it in, and the source to number it under. Handing a walker the
/// probe alone is what let a walker and its own first leaf both render as `#0`.
#[derive(Clone)]
pub(super) struct MergeProbe {
    pub operation: Arc<OperationProbe>,
    /// The walker's own row. Every leaf of the subtree hangs off it.
    pub source_row: TaskRow,
}

/// Records exactly what a single `copy_single_path` call wrote to the
/// destination, so rollback can remove only what this operation created — never
/// dest-only files that pre-existed a merged destination directory.
///
/// A directory source merges into an existing dest directory ("Overwrite means
/// merge for dirs"), so recording the top-level dest directory and recursively
/// deleting it on rollback would destroy the user's untouched files. Instead we
/// record:
/// - `files`: every destination FILE the copy streamed, in write order, each with
///   the byte count it was written with. Rollback deletes these individually; the
///   size is what lets both the in-flight reversal and the operation log
///   recognize a leaf INSIDE a copied folder (see [`WrittenFile`]).
/// - `dirs`: every destination DIRECTORY this copy newly created (i.e. the
///   `create_directory` call returned `Ok`, not `AlreadyExists`), in
///   creation order (shallowest first). Rollback removes these with a
///   non-recursive delete (empty-only on real backends), deepest first, so a
///   directory that still holds a pre-existing sibling survives.
// DEFAULT-OK: an empty ledger is an operation that has created nothing yet, which is the
// one state where rollback correctly has nothing to undo.
#[derive(Default)]
pub(super) struct CreatedPaths {
    pub files: Mutex<Vec<WrittenFile>>,
    pub dirs: Mutex<Vec<PathBuf>>,
    // Children a DEEP merge resolved to Skip (a conflict the user/policy
    // declined). Invisible to the top-level driver, so tallied here; the
    // move-out op reads the count to keep a not-fully-extracted source in the
    // archive (see `skipped_file_count`).
    pub skipped_files: std::sync::atomic::AtomicUsize,
    pub skipped_bytes: std::sync::atomic::AtomicU64,
    // The SOURCE path of each skipped child. A skipped child never landed at
    // the destination, so the source copy is the only one: a MOVE sweeps its
    // source folder while preserving exactly these (see
    // `skipped_source_paths`).
    pub skipped_sources: Mutex<Vec<PathBuf>>,
    // Deep-merge children that REPLACED an existing dest file. The operation-log
    // capture reads this per source (`overwrote_count`) so a copy / move whose
    // subtree overwrote anything finalizes `not_rollbackable` — deleting the
    // copies can't bring the overwritten originals back.
    pub overwrote_files: std::sync::atomic::AtomicUsize,
    // What a MOVE's copy walk carried out of the SOURCE: every file it copied,
    // with what the listing said about it, and every folder it listed. The
    // move's source sweep removes exactly this and nothing else
    // (`source_sweep.rs`). `None` on a copy, which never sweeps.
    pub source_ledger: Option<Mutex<SourceLedger>>,
}

impl CreatedPaths {
    /// A ledger that also keeps the [`SourceLedger`] a move's source sweep reads.
    /// `mac_only` is what the walk leaves out, which the sweep then removes with
    /// a folder that holds nothing else.
    pub(super) fn recording_sources(mac_only: cmdr_fs::mac_only_files::MacOnlyFiles) -> Self {
        Self {
            source_ledger: Some(Mutex::new(SourceLedger::leaving_out(mac_only))),
            ..Self::default()
        }
    }

    /// Notes a source folder the copy walk listed. A no-op on a copy.
    pub(super) fn record_walked_source_dir(&self, dir: &Path) {
        if let Some(ledger) = &self.source_ledger {
            ledger.lock_ignore_poison().record_walked_dir(dir.to_path_buf());
        }
    }

    /// Notes a source file the copy walk is carrying, stamped from the listing
    /// that found it. The walk reads that listing BEFORE the copy, so a save
    /// during the copy counts as a change too. A no-op on a copy.
    pub(super) fn record_carried_source(&self, file: &Path, listed: &cmdr_fs::entry::FileEntry) {
        if let Some(ledger) = &self.source_ledger {
            ledger
                .lock_ignore_poison()
                .record_carried_file(file.to_path_buf(), SourceStamp::of(listed));
        }
    }

    /// Takes back a carried source file that never landed (a refused name the
    /// person skipped), so nothing about it reads as carried. The sweep spares
    /// a skipped path whatever the ledger says; this keeps the ledger true as
    /// well. A no-op on a copy.
    pub(super) fn forget_carried_source(&self, file: &Path) {
        if let Some(ledger) = &self.source_ledger {
            ledger.lock_ignore_poison().forget_carried_file(file);
        }
    }

    /// The source ledger a move's sweep reads, handed over whole. Empty on a
    /// copy.
    pub(super) fn take_source_ledger(&self) -> SourceLedger {
        self.source_ledger
            .as_ref()
            .map(|ledger| std::mem::take(&mut *ledger.lock_ignore_poison()))
            .unwrap_or_default()
    }
    /// Notes a file the copy landed at `path`, with the landed modified time
    /// its write reported (`LandedModified::Unknown` when it reported none).
    pub(super) fn record_file(&self, path: PathBuf, size: u64, landed: cmdr_fs::volume::LandedModified) {
        self.files
            .lock_ignore_poison()
            .push(WrittenFile::volume(path, size).landed_at(landed));
    }

    /// Notes a symlink the copy landed at `path`, holding `target`.
    pub(super) fn record_link(&self, path: PathBuf, target: PathBuf) {
        self.files
            .lock_ignore_poison()
            .push(WrittenFile::volume_link(path, target));
    }

    pub(super) fn record_dir(&self, path: PathBuf) {
        self.dirs.lock_ignore_poison().push(path);
    }

    /// Tally one child a deep merge skipped (conflict resolved to Skip), and
    /// remember its SOURCE path so a move's source sweep can spare it.
    pub(super) fn record_skip(&self, source: PathBuf, size: u64) {
        self.record_skipped_subtree(source, 1, size);
    }

    /// [`record_skip`](Self::record_skip) for a whole subtree left out at once
    /// (a folder whose own name the destination refused): `files` leaves of
    /// `bytes` under `source`, which a move's sweep then spares whole.
    pub(super) fn record_skipped_subtree(&self, source: PathBuf, files: usize, bytes: u64) {
        use std::sync::atomic::Ordering;
        self.skipped_files.fetch_add(files, Ordering::Relaxed);
        self.skipped_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.skipped_sources.lock_ignore_poison().push(source);
    }

    /// Tally one child a deep merge overwrote (replaced an existing dest file).
    pub(super) fn record_overwrite(&self) {
        self.overwrote_files.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether this copy overwrote any existing dest file in its subtree — the
    /// operation-log capture uses it (with the top-level file→file overwrite) to
    /// mark the op `not_rollbackable`.
    pub(super) fn any_overwrote(&self) -> bool {
        self.overwrote_files.load(std::sync::atomic::Ordering::Relaxed) > 0
    }

    /// How many children this copy skipped (deep merge Skips). `0` means the
    /// whole subtree landed; the move-out op keys its per-source archive delete
    /// on this.
    pub(super) fn skipped_file_count(&self) -> usize {
        self.skipped_files.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Total byte size of the skipped children, for folding into the op-wide
    /// skipped-bytes tally.
    pub(super) fn skipped_byte_count(&self) -> u64 {
        self.skipped_bytes.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The source paths a deep merge skipped. A MOVE passes these to
    /// `remove_tree` so its source sweep spares the
    /// children that never landed at the destination — deleting them would
    /// destroy the user's only copy.
    pub(super) fn skipped_source_paths(&self) -> std::collections::HashSet<PathBuf> {
        self.skipped_sources.lock_ignore_poison().iter().cloned().collect()
    }
}
