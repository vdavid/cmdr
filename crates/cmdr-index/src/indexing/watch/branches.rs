//! The branches a search walk covered, and the rule the live loop reads them by.
//!
//! A volume the user turned indexing on for is walked whole and watched whole:
//! every event its stream carries belongs to ground the index answers for. A
//! volume a SEARCH walked is the other shape — a few branches are covered and
//! everything else is untouched — and the same unfiltered loop over it would do
//! two wrong things: write rows under directories nothing ever listed, and
//! escalate an event in unwalked ground into a subtree rescan nobody asked for
//! (`../reconcile/reconciler/escalation.rs`). So a branch-watched volume admits
//! an event only when it lands inside ground a walk covered.
//!
//! ## Three states, and why the middle one exists
//!
//! - Inside a branch whose walk has finished ⇒ **process**. The index answers for
//!   that ground, so a change to it is a change to the index.
//! - Inside a branch a walk is covering RIGHT NOW ⇒ **buffer**, and replay when
//!   the walk ends. Processing it live would let the live loop and the parallel
//!   walker write the same names through one writer: the walker allocates fresh
//!   ids, `INSERT OR IGNORE` drops whichever row loses, and everything the walk
//!   attributed to the dropped id is orphaned (`../../lifecycle/cover/live.rs`
//!   names the same hazard between two walks). Discarding instead would drift the
//!   branch's aggregates with no signal, which is the failure nobody would notice.
//!   This is the per-branch shape of the scan-completion handshake, which buffers
//!   a whole volume's events for the same reason.
//! - Anywhere else ⇒ **discard**. Not our ground.
//!
//! ## What is deliberately not a plain prefix test
//!
//! A coalesced `MustScanSubDirs` arrives at a SHALLOWER path than the branch it
//! is telling us about ("a lot changed under here"). Dropping it because that
//! path sits above every branch would silently lose every change inside the
//! covered ground, so an ancestor-of-a-branch sweep is RE-ANCHORED onto each
//! branch under it.
//!
//! ## Where the set lives
//!
//! In memory per volume (`for_volume`), and on the volume's own index database
//! as [`COVERED_BRANCHES_KEY`] — index-relative, so a drive that comes back at a
//! different mount point still finds its branches. "Clear index" deletes the
//! database, so the branch set goes with the coverage it describes, and a full
//! scan drops it because a whole-watched volume has no use for branches.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use super::watcher::FsChangeEvent;
use crate::indexing::IndexPathSpace;
use crate::indexing::paths::path_prefix::is_strict_descendant;
use crate::indexing::store::IndexStore;
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::ignore_poison::IgnorePoison;

/// Meta key for the persisted branch set: index-relative paths, one per line.
pub(crate) const COVERED_BRANCHES_KEY: &str = "walk_covered_branches";

/// How many events one branch holds while its walk covers it.
///
/// Generous, because losing the buffer costs a re-list of the branch, and tight
/// enough that a pathological churn storm during a long walk can't grow without
/// bound (~300 B an event, so ~30 MB at the cap). Past it the branch stops
/// collecting and asks for a re-list instead, which is the honest recovery: the
/// buffer is no longer a complete record of what changed.
const BRANCH_BUFFER_CAP: usize = 100_000;

/// Every branch-watched volume's set, keyed by volume id. A volume with no
/// search-walked branches holds no entry.
static WATCHES: OnceLock<Mutex<HashMap<String, Arc<BranchWatch>>>> = OnceLock::new();

fn watches() -> &'static Mutex<HashMap<String, Arc<BranchWatch>>> {
    WATCHES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// This volume's set as it stands in this session, empty if nothing has walked
/// it yet.
///
/// Every live loop takes one, including a scanned volume's: a walk over a hole in
/// an indexed drive needs the same buffering as a walk on an unindexed one, or
/// the loop and the walker write the same names through one writer.
pub(crate) fn live_for(volume_id: &str) -> Arc<BranchWatch> {
    let mut all = watches().lock_ignore_poison();
    Arc::clone(
        all.entry(volume_id.to_string())
            .or_insert_with(|| Arc::new(BranchWatch::with_branches(Vec::new()))),
    )
}

/// This volume's set, restored from its own database.
///
/// Called once, when a volume whose index a search built comes back up — the
/// first moment anything can read that coverage, since an unregistered volume
/// answers neither sizes nor coverage questions. `conn` is a read connection on
/// the volume's own database.
pub(crate) fn resumed_for(volume_id: &str, space: &IndexPathSpace, conn: &rusqlite::Connection) -> Arc<BranchWatch> {
    let persisted = load_branches(space, conn);
    if !persisted.is_empty() {
        log::info!(
            "Branch watch: '{volume_id}' comes back with {} walk-covered branch(es)",
            persisted.len()
        );
    }
    // Restored INTO whatever this session already holds rather than replacing it.
    // The two can both be non-empty — a walk that registered a branch a moment ago
    // is not on the database yet — and a resume that dropped either half would
    // leave ground the volume claims to watch with nothing watching it.
    let watch = live_for(volume_id);
    watch.restore(persisted);
    watch
}

/// Drop a volume's set from memory. Its database keeps the persisted copy unless
/// the caller also cleared that (a cleared index deletes the file outright).
pub(crate) fn forget(volume_id: &str) {
    watches().lock_ignore_poison().remove(volume_id);
}

/// Retire the branch concept for a volume that's about to be walked WHOLE: the
/// scan covers everything and its watcher watches everything, so a branch set
/// would only be a stale second answer to a question the volume no longer asks.
pub(crate) fn clear(volume_id: &str, writer: &IndexWriter) {
    forget(volume_id);
    let _ = writer.send(WriteMessage::DeleteMeta(COVERED_BRANCHES_KEY.to_string()));
}

/// Whether this volume's database remembers any ground a walk covered on it.
///
/// The one durable difference between a volume the phase machine was part way
/// through and a first BULK scan somebody interrupted: `start_scan` clears the set
/// before it walks, so an interrupted bulk build has none while a phased (or
/// search-walked) volume does. That is what `launch_route` discriminates on, and
/// it is asked at launch, before [`resumed_for`] has loaded anything into memory —
/// so it reads the database rather than the in-memory set.
pub(crate) fn any_persisted(conn: &rusqlite::Connection) -> bool {
    IndexStore::get_meta(conn, COVERED_BRANCHES_KEY)
        .ok()
        .flatten()
        .is_some_and(|stored| stored.lines().any(|line| !line.trim().is_empty()))
}

/// Read the persisted branches back as absolute paths for this volume's space.
fn load_branches(space: &IndexPathSpace, conn: &rusqlite::Connection) -> Vec<Branch> {
    let Ok(Some(stored)) = IndexStore::get_meta(conn, COVERED_BRANCHES_KEY) else {
        return Vec::new();
    };
    let volume_root = space.volume_root_string();
    stored
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|relative| Branch::new(join_volume_relative(&volume_root, relative)))
        .collect()
}

/// Rebuild an absolute path from the volume root and an index-relative one. The
/// boot disk's index-relative paths are already absolute, so its root (`/`) has
/// to not double up the separator.
fn join_volume_relative(volume_root: &str, relative: &str) -> String {
    let trimmed_root = volume_root.trim_end_matches('/');
    let trimmed_relative = relative.trim_start_matches('/');
    format!("{trimmed_root}/{trimmed_relative}")
}

/// One patch of ground a search walk covered on this volume.
struct Branch {
    /// The absolute path, in this volume's space.
    path: String,
    /// How many walks are covering it right now. Above zero its events buffer;
    /// a count rather than a flag because two searches can walk overlapping
    /// frontiers and the second must not un-buffer the first.
    walks: usize,
    /// Events held while a walk covers this branch, replayed when the last one
    /// ends.
    buffered: Vec<FsChangeEvent>,
    /// The buffer hit its cap, so what it holds is no longer a complete record
    /// of what changed. The branch asks for a re-list instead of a replay.
    overflowed: bool,
}

impl Branch {
    fn new(path: String) -> Self {
        Self {
            path,
            walks: 0,
            buffered: Vec::new(),
            overflowed: false,
        }
    }

    /// Whether `path` is this branch or sits under it.
    fn contains(&self, path: &str) -> bool {
        path == self.path || is_strict_descendant(path, &self.path)
    }
}

/// How much of a volume its live loop answers for.
///
/// A volume that was scanned whole is watched whole; a volume a search walked is
/// watched exactly as far as the walk went. BOTH carry the branch set, because
/// the buffer-while-a-walk-runs rule is the same on either: what differs is only
/// what happens to an event no walk is covering — served on a scanned volume,
/// dropped on a walked one.
#[derive(Clone)]
pub(crate) enum WatchScope {
    /// Every event the stream carries lands in ground the index answers for.
    WholeVolume(Arc<BranchWatch>),
    /// Only what a search walk covered.
    Branches(Arc<BranchWatch>),
}

impl WatchScope {
    /// What the loop should do with one event, whose path is already absolute in
    /// this volume's space.
    pub(crate) fn admit(&self, event: FsChangeEvent) -> Admission {
        match self {
            Self::WholeVolume(watch) => watch.admit(event, Reach::WholeVolume),
            Self::Branches(watch) => watch.admit(event, Reach::CoveredBranches),
        }
    }

    /// The branch set. Both arms have one; the variant says what it means.
    pub(crate) fn branches(&self) -> &Arc<BranchWatch> {
        match self {
            Self::WholeVolume(watch) | Self::Branches(watch) => watch,
        }
    }

    /// Whether a reconcile may walk `anchor` right now.
    ///
    /// Two refusals, both about not walking ground somebody else owns:
    ///
    /// - ground a cover walk is covering this moment. The reconcile and the
    ///   parallel walker would write the same names through one writer, and the
    ///   walker's fresh ids lose to `INSERT OR IGNORE`, orphaning whatever it
    ///   attributed to the dropped one. The walk is covering it anyway.
    /// - on a branch-watched volume, anything outside the covered branches: the
    ///   walk owns coverage growth, and a watcher that indexed unwalked ground
    ///   would be doing exactly the uninvited work both indexing switches exist to
    ///   stop. That ground stays frontier, so the next search over it walks.
    pub(crate) fn may_walk(&self, anchor: &Path) -> bool {
        let watch = self.branches();
        if watch.is_being_walked(anchor) {
            return false;
        }
        match self {
            Self::WholeVolume(_) => true,
            Self::Branches(watch) => watch.covers(anchor),
        }
    }
}

/// How far a loop's admission rule reaches.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Everything, except what a walk is covering right now.
    WholeVolume,
    /// Only ground a search walk covered.
    CoveredBranches,
}

/// What a finished walk leaves behind on the volume's branch set.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AfterWalk {
    /// Ground this volume's loop now keeps current branch by branch. What a
    /// search-built index's walk leaves.
    Watch,
    /// Nothing to remember: the volume's loop already answers for every path,
    /// so a branch would only be a second, staler description of it. What a walk
    /// over a hole in a SCANNED volume leaves — it was only ever registered to
    /// buffer events for the duration.
    Forget,
}

/// What the live loop should do with one event.
#[derive(Debug)]
pub(crate) enum Admission {
    /// Feed these into the batch. Usually the event itself; a coalesced
    /// `MustScanSubDirs` above the branches becomes one event per branch under
    /// it.
    Process(Vec<FsChangeEvent>),
    /// Held until the walk covering its branch ends.
    Buffered,
    /// Outside every covered branch.
    Discarded,
}

/// Events a finished walk released, and the branches whose buffer overflowed
/// instead.
#[derive(Default)]
pub(crate) struct Promoted {
    /// Buffered events to fold into the next batch.
    pub(crate) events: Vec<FsChangeEvent>,
    /// Branches to re-list, because their buffer stopped being a complete record.
    pub(crate) relist: Vec<String>,
}

/// One volume's branch set, shared between the walks that grow it and the live
/// loop that reads it.
pub(crate) struct BranchWatch {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Ancestor-minimal among SETTLED entries, and no more than that: a walk over
    /// a pocket inside an already-covered branch needs its own entry to buffer
    /// against, and deepest-match wins so it gets one. Every collapse is the same
    /// absorption rule firing, on insert and when a walk ends.
    branches: Vec<Branch>,
    /// Released buffers waiting for the loop's next flush tick.
    promoted: Promoted,
    /// The highest event id this volume's stream has carried, including the
    /// events we discarded. Journal position is about the STREAM, not about what
    /// we chose to act on, so an unrelated drive-wide event still advances it —
    /// otherwise a quiet branch's stored position ages until the next launch's
    /// replay gap is too wide to bother with.
    max_event_id_seen: u64,
}

impl BranchWatch {
    fn with_branches(branches: Vec<Branch>) -> Self {
        Self {
            state: Mutex::new(State {
                branches,
                ..State::default()
            }),
        }
    }

    /// Add branches read back from the database, keeping whatever this session
    /// already knows about.
    fn restore(&self, restored: Vec<Branch>) {
        let mut state = self.state.lock_ignore_poison();
        for branch in restored {
            if !state.branches.iter().any(|held| held.contains(&branch.path)) {
                state.insert(&branch.path);
            }
        }
    }

    /// A walk is about to cover `paths`. Their events buffer from this moment,
    /// which is BEFORE the walk reads anything — the window this closes is the
    /// one where a change lands in ground the walk has already passed.
    ///
    /// Reports the branches that are new to this volume, which is what a
    /// per-branch watcher backend (inotify) has to register.
    pub(crate) fn begin_covering(&self, paths: &[String]) -> Vec<String> {
        let mut state = self.state.lock_ignore_poison();
        let mut added = Vec::new();
        for path in paths {
            if state.insert(path) {
                added.push(path.clone());
            }
            if let Some(branch) = state.branches.iter_mut().find(|b| b.path == *path) {
                branch.walks += 1;
            }
        }
        added
    }

    /// A walk over `paths` ended, however it ended. Their buffers are released,
    /// and `after` says whether the ground is now the loop's to keep current or
    /// something it already answered for.
    ///
    /// [`AfterWalk::Watch`] either way for a cancelled walk: it still marked every
    /// directory it read, so that ground is exactly as covered as a completed
    /// walk's and needs the same watching. What it didn't reach stays frontier and
    /// the next search asks for it again.
    pub(crate) fn finish_covering(&self, paths: &[String], after: AfterWalk) {
        let mut state = self.state.lock_ignore_poison();
        for path in paths {
            let Some(index) = state.branches.iter().position(|b| b.path == *path) else {
                continue;
            };
            let branch = &mut state.branches[index];
            branch.walks = branch.walks.saturating_sub(1);
            if branch.walks > 0 {
                continue;
            }
            let released = std::mem::take(&mut branch.buffered);
            let overflowed = std::mem::replace(&mut branch.overflowed, false);
            if overflowed {
                state.promoted.relist.push(path.clone());
            } else {
                state.promoted.events.extend(released);
            }
            // The set is the shortest description of the ground this volume
            // watches branch by branch. So a branch goes when the volume's loop
            // already answers for it (a scanned volume) or a settled branch around
            // it does, and one that stays absorbs whatever settled underneath it
            // while it was live.
            let covered_by_a_settled_branch = state
                .branches
                .iter()
                .any(|other| other.path != *path && other.walks == 0 && other.contains(path));
            if after == AfterWalk::Forget || covered_by_a_settled_branch {
                state.branches.remove(index);
            } else {
                state.absorb_settled_under(path);
            }
        }
        state.branches.sort_by(|a, b| a.path.cmp(&b.path));
    }

    /// Collapse the set to `root`: one entry covering everything walked under it,
    /// written down.
    ///
    /// The set is scanned once per event on the live hot path
    /// (`deepest_containing`), so a caller that knows a whole subtree is covered
    /// can say so and stop paying for the branches inside it. Absorption does the
    /// rest: an entry a walk is covering right now stays, and is absorbed by the
    /// same rule when it finishes.
    ///
    /// ⚠️ It mutates THIS `BranchWatch` in place, and that is the point: the live
    /// loop and its reconciler each captured an `Arc` of it at
    /// `ensure_branch_watch`. ❌ Never build a collapse out of `branches::clear`
    /// plus a begin/finish pair — `clear` calls `forget`, so `live_for` mints a
    /// brand-new set nobody is reading, the running loop keeps filtering against
    /// the old entries for the rest of the session, and the database says
    /// something else. (`start_scan`'s `clear` is safe only because the loop is
    /// torn down and replaced in the same breath.)
    #[allow(
        dead_code,
        reason = "no caller in the crate: what collapses the set today is absorption as walks finish. This is the \
                  explicit form, for a caller that covers a whole subtree in pieces and can say so once it's done."
    )]
    pub(crate) fn collapse_to(&self, root: &str, space: &IndexPathSpace, writer: &IndexWriter) {
        self.state.lock_ignore_poison().insert(root);
        self.persist(space, writer);
    }

    /// What the live loop should do with one event, whose path is already
    /// absolute in this volume's space.
    fn admit(&self, event: FsChangeEvent, reach: Reach) -> Admission {
        let mut state = self.state.lock_ignore_poison();
        state.max_event_id_seen = state.max_event_id_seen.max(event.event_id);

        if let Some(index) = state.deepest_containing(&event.path) {
            return state.take(index, event, reach);
        }

        // Nothing holds this path itself. A coalesced sweep ABOVE the branches is
        // still about them, so it's re-anchored rather than dropped — on a
        // whole-watched volume too, where processing it as-is would send a
        // reconcile straight through ground a walk is covering.
        let under: Vec<usize> = (0..state.branches.len())
            .filter(|&i| is_strict_descendant(&state.branches[i].path, &event.path))
            .collect();
        if event.flags.must_scan_sub_dirs && !under.is_empty() {
            let mut process = Vec::new();
            for &index in &under {
                let anchored = FsChangeEvent {
                    path: state.branches[index].path.clone(),
                    event_id: event.event_id,
                    flags: event.flags.clone(),
                };
                if let Admission::Process(mut events) = state.take(index, anchored, reach) {
                    process.append(&mut events);
                }
            }
            // A whole-watched volume keeps the sweep it was handed as well: the
            // branches under it get their own re-anchored copies, and the rest of
            // the subtree is still this loop's to reconcile. If a walk is covering
            // one of those branches, the sweep is HELD against it rather than
            // dropped — reconciling it now would walk straight through the walk,
            // and dropping it would lose the rest of the subtree it speaks for.
            if reach == Reach::WholeVolume {
                match under.iter().find(|&&i| state.branches[i].walks > 0) {
                    Some(&walked) => {
                        state.take(walked, event, reach);
                    }
                    None => process.push(event),
                }
            }
            return if process.is_empty() {
                Admission::Buffered
            } else {
                Admission::Process(process)
            };
        }

        match reach {
            Reach::WholeVolume => Admission::Process(vec![event]),
            Reach::CoveredBranches => Admission::Discarded,
        }
    }

    /// Whether any of `paths`' branches is holding events for the walk covering it.
    ///
    /// What a walk that deferred its writer drain asks before releasing ground:
    /// those events are replayed the moment the branch is finished, and the loop
    /// resolves their paths through a read connection, so they need the walk's rows
    /// committed first.
    pub(crate) fn any_buffered(&self, paths: &[String]) -> bool {
        let state = self.state.lock_ignore_poison();
        state
            .branches
            .iter()
            .any(|branch| !branch.buffered.is_empty() && paths.iter().any(|path| branch.contains(path)))
    }

    /// Whether a walk is covering `path` (or the branch it sits in) right now.
    pub(crate) fn is_being_walked(&self, path: &Path) -> bool {
        let path = path.to_string_lossy();
        let state = self.state.lock_ignore_poison();
        state
            .branches
            .iter()
            .any(|branch| branch.walks > 0 && branch.contains(&path))
    }

    /// Take whatever finished walks released. The loop folds the events into its
    /// next batch and re-lists the branches whose buffer overflowed.
    pub(crate) fn take_promoted(&self) -> Promoted {
        std::mem::take(&mut self.state.lock_ignore_poison().promoted)
    }

    /// The journal position it's safe to persist: the highest event id the stream
    /// carried, unless something is still buffered, in which case advancing past
    /// it would let a restart skip the very events we're holding.
    pub(crate) fn safe_event_id(&self) -> Option<u64> {
        let state = self.state.lock_ignore_poison();
        if state.branches.iter().any(|b| !b.buffered.is_empty()) {
            return None;
        }
        (state.max_event_id_seen > 0).then_some(state.max_event_id_seen)
    }

    /// Whether a missing-parent escalation may walk `anchor`.
    ///
    /// The walk owns coverage growth; the watcher only keeps covered ground
    /// current. An anchor outside every branch would have the watcher indexing
    /// ground nobody asked for, on a drive whose owner may have indexing turned
    /// off entirely.
    pub(crate) fn covers(&self, anchor: &Path) -> bool {
        let path = anchor.to_string_lossy();
        let state = self.state.lock_ignore_poison();
        state.branches.iter().any(|b| b.contains(&path))
    }

    /// The covered branches, shallowest first. The persisted form and what tests
    /// assert against.
    pub(crate) fn branch_paths(&self) -> Vec<String> {
        self.state
            .lock_ignore_poison()
            .branches
            .iter()
            .map(|b| b.path.clone())
            .collect()
    }

    /// Write the set onto the volume's own database, so the next session finds
    /// the branches this one covered.
    pub(crate) fn persist(&self, space: &IndexPathSpace, writer: &IndexWriter) {
        let value = self
            .branch_paths()
            .iter()
            .filter_map(|path| space.index_relative(path))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = writer.send(WriteMessage::UpdateMeta {
            key: COVERED_BRANCHES_KEY.to_string(),
            value,
        });
    }
}

impl State {
    /// Add `path` to the set, retiring every settled branch it now covers, and
    /// report whether it was new. A path already held keeps its entry — only its
    /// walk count is the caller's business.
    ///
    /// Absorption is a property of the SET, so it holds however a branch arrives:
    /// a walk registering one, a resume restoring one, an explicit collapse.
    /// Watching `/a` covers `/a/b`, and keeping both makes every event pay a
    /// longer `deepest_containing` scan for an answer that can't differ.
    fn insert(&mut self, path: &str) -> bool {
        self.absorb_settled_under(path);
        if self.branches.iter().any(|held| held.path == path) {
            return false;
        }
        self.branches.push(Branch::new(path.to_string()));
        self.branches.sort_by(|a, b| a.path.cmp(&b.path));
        true
    }

    /// Drop every settled branch strictly under `path`, which now covers them.
    ///
    /// ❌ A branch a walk is covering RIGHT NOW is left alone: its buffer belongs
    /// to that walk, and dropping the entry would strand the events it holds. The
    /// same rule absorbs it when it finishes. Settled entries are safe to drop
    /// because a branch only buffers while `walks > 0`.
    fn absorb_settled_under(&mut self, path: &str) {
        self.branches
            .retain(|held| held.walks > 0 || !is_strict_descendant(&held.path, path));
    }

    /// The most specific branch holding `path`, so a pocket being walked inside a
    /// live branch buffers rather than processes.
    fn deepest_containing(&self, path: &str) -> Option<usize> {
        self.branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| branch.contains(path))
            .max_by_key(|(_, branch)| branch.path.len())
            .map(|(index, _)| index)
    }

    /// Buffer the event against a branch under walk, or hand it back to process.
    fn take(&mut self, index: usize, event: FsChangeEvent, reach: Reach) -> Admission {
        let branch = &mut self.branches[index];
        if branch.walks == 0 {
            // A `Forget`ted branch is gone from the set the moment its walk ends,
            // so reaching here with no walk means the ground is covered either way.
            let _ = reach;
            return Admission::Process(vec![event]);
        }
        if branch.buffered.len() >= BRANCH_BUFFER_CAP {
            if !branch.overflowed {
                log::warn!(
                    "Branch watch: {} filled its buffer while a walk covered it; it will be re-listed instead",
                    branch.path
                );
            }
            branch.overflowed = true;
            branch.buffered.clear();
            branch.buffered.shrink_to_fit();
            return Admission::Buffered;
        }
        branch.buffered.push(event);
        Admission::Buffered
    }
}

#[cfg(test)]
mod tests;
