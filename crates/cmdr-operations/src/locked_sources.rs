//! Locked items in what a move takes away, and the person's consent to unlock
//! them.
//!
//! A move has to rename or delete the user's ORIGINAL, and Finder's Locked
//! (`uchg`/`uappnd`) or an ACL denying `delete` refuses both. A copy never needs
//! that (it only reads the original), so only a move asks. It asks ONCE, before
//! any byte moves, so a cross-drive move can never end with everything copied and
//! the originals refusing to go (#408):
//!
//! - **Some Locked**: it emits `write-locked-sources` and parks on
//!   `state.locked_sources` (`locked_sources_slot.rs`), holding its lanes like a
//!   conflict prompt. [`Operations::answer_locked_sources`] resumes it with a
//!   [`LockedSourcesAnswer`]; a cancel ends it through the ordinary cancel path,
//!   which abandons the slot.
//! - **Move anyway**: the engine clears an item's blockers right before the act
//!   that needs it ([`LockedPlan::unlock`]) and puts them back where the item
//!   lands ([`Unlocked::landed_at`]) or, when it stays, where it is (`Drop`). A
//!   copy already lands Locked, so a cross-drive move only clears the original
//!   right before deleting it.
//! - **Skip**: each Locked item stays where it is, with everything under it, and
//!   the completion counts it ([`LockedKept`]).
//! - **System-locked** (`schg`/`sappnd`): only root can clear those, so no answer
//!   could move them. Never asked about; always skipped and counted.
//!
//! ❌ Clearing never happens inside `LocalPosixVolume::delete` or `rename`, nor
//! anywhere the person didn't say yes: F8 and F2 run through those, and must
//! never silently unlock a user's file. Only a [`LockedPlan`] built from an
//! answer clears anything.
//!
//! A same-drive move unlocks, renames, and locks again. A crash between the clear
//! and the re-lock leaves the item unlocked at whichever path it's at; the same
//! accepted gap as a copy's landing (`DETAILS.md` § "Moving Locked items").

use cmdr_fs::log_private::LogPrivate;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::metadata::{LandedMetadata, RemovalLock, Unblock, apply_landed, clear_blockers};
use cmdr_fs::volume::{Volume, root_anchored};
use tokio::sync::oneshot;

use super::Operations;
use super::event_sinks::OperationEventSink;
use super::state::WriteOperationState;
use super::types::{
    CancelRollback, LockedKept, LockedSourcesAnswer, LockedSourcesAnswerOutcome, WriteCancelledEvent,
    WriteLockedSourcesEvent, WriteOperationType,
};
use super::validation::is_real_directory;
use cmdr_operation_log::types::OperationId;

/// How many Locked items the question names. The count beside it is the whole
/// truth; the names only help the person recognize them.
const LOCKED_SAMPLE_LEN: usize = 5;

impl Operations {
    /// Resumes a move parked on "these items are Locked" with the person's
    /// answer. Cancelling is the ordinary `cancel_operation`.
    pub fn answer_locked_sources(
        &self,
        operation_id: &OperationId,
        answer: LockedSourcesAnswer,
    ) -> LockedSourcesAnswerOutcome {
        let Some(state) = self.registry().get(operation_id) else {
            log::info!(target: "locked_sources", "answer for op={operation_id}: no such operation");
            return LockedSourcesAnswerOutcome::UnknownOperation;
        };
        if state.locked_sources.answer(answer) {
            log::info!(target: "locked_sources", "op={operation_id}: answered {answer:?}");
            LockedSourcesAnswerOutcome::Answered
        } else {
            log::info!(target: "locked_sources", "answer for op={operation_id}: it isn't asking");
            LockedSourcesAnswerOutcome::NotAsking
        }
    }

    /// The Locked-items question `operation_id` is parked on, if any. For a
    /// surface that arrives after the broadcast.
    pub fn pending_locked_sources(&self, operation_id: &OperationId) -> Option<WriteLockedSourcesEvent> {
        self.registry().get(operation_id)?.locked_sources.pending()
    }
}

/// What a look over a move's items found, in the order it found them.
// DEFAULT-OK: an empty scan is "nothing Locked", the answer for nearly every move.
#[derive(Debug, Default)]
pub(crate) struct LockedScan {
    /// The user's own Locked items: the move asks about these.
    locked: Vec<PathBuf>,
    /// Items the system locked: skipped without asking.
    system_locked: Vec<PathBuf>,
}

impl LockedScan {
    /// Classifies each of `paths` (no walk, no duplicates expected).
    pub(crate) fn of_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Self {
        let mut scan = Self::default();
        for path in paths {
            scan.note(path);
        }
        scan
    }

    /// Classifies every item under each of `roots`, the roots included, never
    /// through a symlink. For a move whose copy walks the trees through a
    /// `Volume`, where no local walk result is at hand to read. `stop` is asked
    /// per folder; a `true` ends the walk early, and the cancel it stands for
    /// stops the move right after.
    pub(crate) fn of_trees(roots: &[PathBuf], stop: &dyn Fn() -> bool) -> Self {
        let mut scan = Self::default();
        for root in roots {
            scan.note(root);
            let mut pending = vec![root.clone()];
            while let Some(dir) = pending.pop() {
                if stop() {
                    return scan;
                }
                // `symlink_metadata`, so a link to a folder is a leaf here as it
                // is to the move.
                if !is_real_directory(&dir) {
                    continue;
                }
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                for entry in entries.flatten() {
                    let path = entry.path();
                    scan.note(&path);
                    if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                        pending.push(path);
                    }
                }
            }
        }
        scan
    }

    /// What a same-drive move renames: each source, and where a source folder
    /// MERGES into a folder of the same name, each child the merge renames on its
    /// own, level by level, the way `merge_move_directory` walks them. A folder
    /// that lands by one rename takes its contents along untouched, so nothing
    /// inside it is asked about.
    pub(crate) fn of_renames(sources: &[PathBuf], destination: &Path, destination_name: Option<&str>) -> Self {
        let mut scan = Self::default();
        for source in sources {
            scan.note(source);
            let Some(name) = source.file_name() else { continue };
            let leaf = destination_name.map(Path::new).unwrap_or_else(|| Path::new(name));
            scan.note_merge(source, &destination.join(leaf));
        }
        scan
    }

    fn note_merge(&mut self, source_dir: &Path, dest_dir: &Path) {
        if !is_real_directory(source_dir) || !is_real_directory(dest_dir) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(source_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let child = entry.path();
            self.note(&child);
            self.note_merge(&child, &dest_dir.join(entry.file_name()));
        }
    }

    fn note(&mut self, path: &Path) {
        match classify(path) {
            RemovalLock::Free => {}
            RemovalLock::Locked => self.locked.push(path.to_path_buf()),
            RemovalLock::SystemLocked => self.system_locked.push(path.to_path_buf()),
        }
    }
}

/// What a move does about its Locked items, settled before it touches any.
// DEFAULT-OK: the empty plan unlocks and skips nothing, which is right for a move
// with nothing Locked and for every staged item a move made itself.
#[derive(Debug, Default)]
pub(crate) struct LockedPlan {
    /// What the person agreed to unlock. Cleared only right before the act that
    /// needs it.
    unlock: HashSet<PathBuf>,
    /// What stays where it is, with everything under it.
    skip: HashSet<PathBuf>,
    kept: Option<LockedKept>,
    /// The local root a volume's paths are anchored to (`root_anchored`), so a
    /// volume engine can ask about the paths it lists. `None`: paths are already
    /// absolute local ones.
    root: Option<PathBuf>,
}

impl LockedPlan {
    /// The plan of a move with nothing Locked in it.
    pub(crate) fn none() -> Self {
        Self::default()
    }

    /// This plan, asked with a local volume's paths (`Volume::local_path`).
    pub(crate) fn anchored_at(mut self, root: PathBuf) -> Self {
        self.root = Some(root);
        self
    }

    fn key<'a>(&self, path: &'a Path) -> std::borrow::Cow<'a, Path> {
        match &self.root {
            Some(root) => std::borrow::Cow::Owned(root_anchored(root, path)),
            None => std::borrow::Cow::Borrowed(path),
        }
    }

    /// Whether `path` itself stays where it is.
    pub(crate) fn skips(&self, path: &Path) -> bool {
        !self.skip.is_empty() && self.skip.contains(self.key(path).as_ref())
    }

    /// Whether `path` or a folder above it stays where it is.
    pub(crate) fn skips_under(&self, path: &Path) -> bool {
        !self.skip.is_empty() && self.key(path).ancestors().any(|ancestor| self.skip.contains(ancestor))
    }

    /// Whether anything stays.
    pub(crate) fn skips_any(&self) -> bool {
        !self.skip.is_empty()
    }

    /// The items that stay, each with everything under it.
    pub(crate) fn skipped(&self) -> impl Iterator<Item = &Path> {
        self.skip.iter().map(PathBuf::as_path)
    }

    /// Whether the person agreed to unlock `path` for this move.
    pub(crate) fn unlocks(&self, path: &Path) -> bool {
        !self.unlock.is_empty() && self.unlock.contains(self.key(path).as_ref())
    }

    /// Clears `path`'s blockers when the person agreed to unlock it, right
    /// before the rename or delete that needs it; a no-op [`Unlocked`]
    /// otherwise. The answer says where they go back on.
    pub(crate) fn unlock(&self, path: &Path) -> Unlocked {
        if !self.unlocks(path) {
            return Unlocked::nothing();
        }
        let local = self.key(path).into_owned();
        let cleared = match clear_blockers(&local) {
            Unblock::Cleared(cleared) => Some(cleared),
            Unblock::NothingToClear => None,
            Unblock::Refused => {
                log::warn!(target: "locked_sources", "couldn't unlock {}, the move meets its refusal", LogPrivate::path(&local));
                None
            }
        };
        Unlocked { path: local, cleared }
    }

    /// What the completion reports about what stayed.
    pub(crate) fn kept(&self) -> Option<LockedKept> {
        self.kept
    }
}

/// One item a [`LockedPlan`] unlocked for the act about to take it. ❗ Its
/// blockers go back on: [`landed_at`](Self::landed_at) where it landed, and on
/// `Drop` where it was, so an act that fails or a stop on the way leaves the
/// item exactly as Locked as it found it.
#[must_use = "an unlocked item has to be locked again where it ends up"]
pub(crate) struct Unlocked {
    path: PathBuf,
    cleared: Option<LandedMetadata>,
}

impl Unlocked {
    fn nothing() -> Self {
        Self {
            path: PathBuf::new(),
            cleared: None,
        }
    }

    /// The item now lives at `landed`: lock it there.
    pub(crate) fn landed_at(mut self, landed: &Path) {
        if let Some(cleared) = self.cleared.take() {
            relock(landed, &cleared);
        }
    }

    /// The item is gone from its place and lands nowhere this move tracks: a
    /// cross-drive original whose copy already landed Locked, or a merged folder
    /// whose destination keeps its own.
    pub(crate) fn gone(mut self) {
        self.cleared = None;
    }
}

impl Drop for Unlocked {
    fn drop(&mut self) {
        if let Some(cleared) = self.cleared.take() {
            relock(&self.path, &cleared);
        }
    }
}

fn relock(path: &Path, cleared: &LandedMetadata) {
    let report = apply_landed(path, cleared);
    if report.any_refused() {
        log::warn!(target: "locked_sources", "{} couldn't be locked again: {report:?}", LogPrivate::path(path));
    }
}

/// The plan for `scan`, asking the person first when anything in it is
/// Locked, or `None` when the move is over (cancelled while asking; its
/// `write-cancelled` is already out). Local filesystem; blocks the calling
/// thread while the person decides, so call it only from the move's blocking
/// task.
pub(crate) fn plan_locked_local(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    state: &WriteOperationState,
    scan: LockedScan,
) -> Option<LockedPlan> {
    match ask(events, operation_id, operation_type, state, &scan) {
        Asked::Nothing => Some(settle(scan, None)),
        Asked::Waiting(answer) => {
            let answer = answer.blocking_recv().ok();
            settle_answer(events, operation_id, operation_type, state, scan, answer)
        }
    }
}

/// [`plan_locked_local`] for a move running on the async runtime.
pub(crate) async fn plan_locked_async(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    state: &WriteOperationState,
    scan: LockedScan,
) -> Option<LockedPlan> {
    match ask(events, operation_id, operation_type, state, &scan) {
        Asked::Nothing => Some(settle(scan, None)),
        Asked::Waiting(answer) => {
            let answer = answer.await.ok();
            settle_answer(events, operation_id, operation_type, state, scan, answer)
        }
    }
}

/// [`plan_locked_async`] for a move whose copy reads through `source_volume`.
/// Only a LOCAL source can wear Locked: its trees are walked on the disk itself,
/// off the runtime, since the copy's own walk lists through the `Volume` and
/// carries no flags or ACLs. Any other source plans nothing and asks nobody.
pub(crate) async fn plan_locked_on_volume(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    source_volume: &Arc<dyn Volume>,
    sources: &[PathBuf],
) -> Option<LockedPlan> {
    let Some(root) = source_volume.local_path() else {
        return Some(LockedPlan::none());
    };
    let roots: Vec<PathBuf> = sources.iter().map(|source| root_anchored(&root, source)).collect();
    let walking = Arc::clone(state);
    let scan = state
        .scope()
        .host()
        .runtime()
        .spawn_blocking(move || LockedScan::of_trees(&roots, &|| walking.intent().is_cancelled()))
        .await
        .unwrap_or_default();
    plan_locked_async(events, operation_id, WriteOperationType::Move, state, scan)
        .await
        .map(|plan| plan.anchored_at(root))
}

enum Asked {
    /// Nothing Locked to ask about (system-locked items never are).
    Nothing,
    Waiting(oneshot::Receiver<LockedSourcesAnswer>),
}

fn ask(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    state: &WriteOperationState,
    scan: &LockedScan,
) -> Asked {
    if !scan.system_locked.is_empty() {
        log::info!(
            target: "locked_sources",
            "op={operation_id}: {} system-locked item(s) stay where they are",
            scan.system_locked.len()
        );
    }
    if scan.locked.is_empty() {
        return Asked::Nothing;
    }
    log::info!(target: "locked_sources", "op={operation_id}: {} Locked item(s); asking", scan.locked.len());
    let question = WriteLockedSourcesEvent {
        operation_id: operation_id.to_string(),
        operation_type,
        locked_count: scan.locked.len(),
        locked_sample: scan
            .locked
            .iter()
            .take(LOCKED_SAMPLE_LEN)
            .map(|path| path.display().to_string())
            .collect(),
    };
    let answer = state.locked_sources.arm(question.clone());
    // A cancel that landed before the arm abandoned an empty slot, so look again
    // now that the question is in place; one landing after it drops the sender
    // itself.
    if state.intent().is_cancelled() {
        state.locked_sources.abandon();
    }
    // Parked on a person from here, announced while the slot is armed: a surface
    // may answer from inside the emit.
    state.announce_human_wait(events);
    events.emit_locked_sources(question);
    Asked::Waiting(answer)
}

fn settle_answer(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    state: &WriteOperationState,
    scan: LockedScan,
    answer: Option<LockedSourcesAnswer>,
) -> Option<LockedPlan> {
    state.announce_human_wait(events);
    let Some(answer) = answer else {
        log::info!(target: "locked_sources", "op={operation_id} was cancelled while asking about its Locked items");
        events.emit_cancelled(WriteCancelledEvent {
            operation_id: operation_id.to_string(),
            operation_type,
            files_processed: 0,
            rollback: CancelRollback::none(),
        });
        return None;
    };
    Some(settle(scan, Some(answer)))
}

fn settle(scan: LockedScan, answer: Option<LockedSourcesAnswer>) -> LockedPlan {
    let skipped = if answer == Some(LockedSourcesAnswer::Skip) {
        scan.locked.len()
    } else {
        0
    };
    let kept = (skipped > 0 || !scan.system_locked.is_empty()).then_some(LockedKept {
        skipped: skipped as u32,
        system_locked: scan.system_locked.len() as u32,
    });
    let (unlock, mut skip): (HashSet<PathBuf>, HashSet<PathBuf>) = match answer {
        Some(LockedSourcesAnswer::MoveAnyway) => (scan.locked.into_iter().collect(), HashSet::new()),
        Some(LockedSourcesAnswer::Skip) => (HashSet::new(), scan.locked.into_iter().collect()),
        None => (HashSet::new(), HashSet::new()),
    };
    skip.extend(scan.system_locked);
    LockedPlan {
        unlock,
        skip,
        kept,
        root: None,
    }
}

/// [`cmdr_fs::metadata::removal_lock`], plus a test's stand-in for `schg`, which
/// a non-root test can't set.
fn classify(path: &Path) -> RemovalLock {
    #[cfg(all(test, target_os = "macos"))]
    if test_seam::is_system_locked(path) {
        return RemovalLock::SystemLocked;
    }
    cmdr_fs::metadata::removal_lock(path)
}

/// Lets a test call an item system-locked. A static because the volume engine
/// classifies on a blocking-pool thread; each test names paths under its own
/// temp folder, so parallel tests never see each other's.
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod test_seam {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use cmdr_fs::ignore_poison::IgnorePoison;

    static SYSTEM_LOCKED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

    /// From now on, `path` reads as system-locked to every move.
    pub(crate) fn pretend_system_locked(path: &Path) {
        SYSTEM_LOCKED.lock_ignore_poison().push(path.to_path_buf());
    }

    pub(super) fn is_system_locked(path: &Path) -> bool {
        SYSTEM_LOCKED.lock_ignore_poison().iter().any(|locked| locked == path)
    }
}

#[cfg(test)]
#[path = "locked_sources_tests.rs"]
mod tests;
