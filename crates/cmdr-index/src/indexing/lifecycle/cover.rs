//! Walking a coverage frontier: the write half of the coverage concept.
//!
//! `read/coverage.rs` answers what a scope still needs walked; this drives the
//! walk that fills it in, and hands the entries it finds to whoever asked while
//! it's still running. Every row it writes goes through the volume's normal
//! writer into the normal index (Decision 2), so the work survives the search
//! that paid for it and the next search over the same ground walks less.
//!
//! ## Two primitives, and which one runs
//!
//! A frontier node is virgin ground by definition, so the workload is a bulk add
//! and the PARALLEL walker wins it outright — measured on a real frontier in
//! `docs/notes/cover-walk-primitive-2026-08-05.md`. It runs by default.
//!
//! The serial reconcile is the repair path, for the one case the parallel walker
//! can't take: a frontier node that ISN'T virgin. Those exist (an FSEvents
//! verification pass writes children under a directory without marking that
//! directory listed), and the parallel walker allocates fresh ids for every name
//! it finds, so over pre-existing rows `INSERT OR IGNORE` would drop its rows
//! silently and orphan everything below them. `reconcile_subtree` compares by
//! name and writes only differences, which is exactly the shape that case needs.
//! ❌ Neither path ever deletes: covering is add-only work.

use std::path::Path;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;

use tokio_util::sync::CancellationToken;

use crate::indexing::IndexPathSpace;
use crate::indexing::network_scanner::VolumeScanError;
use crate::indexing::read::coverage::CoverageDimension;
use crate::indexing::scanner::{CoveredEntry, ScanError, ScanSummary, cover_subtree};
use crate::indexing::store::IndexStore;
use crate::indexing::volume::IndexVolumeKind;
use crate::indexing::writer::IndexWriter;
use cmdr_fs::pluralize::pluralize;

/// How many batches may sit between the walk and its consumer.
///
/// Bounded on purpose (Decision 3): a consumer that falls behind slows the walk
/// down rather than letting a queue grow to the size of the subtree. Small,
/// because each batch already holds up to 2 000 entries.
const BATCH_QUEUE_DEPTH: usize = 8;

/// What a walk over a frontier covered.
///
/// `cancelled` is the field that matters to a caller: it separates "the index now
/// answers for this scope" from "someone stopped us partway", and the two are
/// different terminal states in the UI. It is NOT a failure either way — a
/// cancelled walk still left every directory it read marked, so the next search
/// over the same ground starts from where this one stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverOutcome {
    /// Entries the walk discovered and wrote.
    pub entries_found: u64,
    /// Directories among them.
    pub dirs_found: u64,
    /// Frontier roots it finished. Anything it didn't reach stays frontier, and
    /// a fresh `coverage` call names it.
    pub roots_covered: usize,
    /// Whether it stopped early because someone cancelled it.
    pub cancelled: bool,
}

/// A running walk over a frontier.
///
/// Take batches off it until [`next_batch`](Self::next_batch) reports `None`,
/// then [`finish`](Self::finish) for the totals. Dropping it does NOT stop the
/// walk (Decision 11: a superseded query keeps its walk running, because walking
/// is coverage work and matching is query work) — the walk simply stops emitting
/// and runs to completion filling the index.
pub struct CoverWalk {
    batches: Receiver<Vec<CoveredEntry>>,
    cancel: CancellationToken,
    thread: JoinHandle<CoverOutcome>,
    deferred: Vec<String>,
}

impl CoverWalk {
    /// The next batch of entries, blocking until one arrives. `None` once the
    /// walk has ended, for whatever reason.
    pub fn next_batch(&self) -> Option<Vec<CoveredEntry>> {
        self.batches.recv().ok()
    }

    /// Frontier roots this walk is NOT covering, because another walk on the same
    /// volume already is.
    ///
    /// Their rows land in the same index either way, and a query re-run once the
    /// other walk gets there picks them up — so this is "you'll get these a bit
    /// later", never "these are lost". Normally empty; it fills when a refined
    /// query asks for ground its predecessor's walk is still covering, which
    /// Decision 11 keeps running.
    pub fn covered_by_another_walk(&self) -> &[String] {
        &self.deferred
    }

    /// Stop the walk. Returns immediately; the walk winds down behind it, and
    /// everything it already read stays marked.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Wait for the walk to end and report what it covered.
    ///
    /// Drops the batch channel first, so a caller that stopped reading batches
    /// doesn't deadlock against a walk parked on a full one. ❌ Call
    /// [`cancel`](Self::cancel) first if you want it to stop — on its own this
    /// waits for the whole frontier.
    pub fn finish(self) -> CoverOutcome {
        let CoverWalk { batches, thread, .. } = self;
        drop(batches);
        thread.join().unwrap_or(CoverOutcome {
            entries_found: 0,
            dirs_found: 0,
            roots_covered: 0,
            cancelled: true,
        })
    }
}

/// Everything one walk needs, resolved on the caller's thread so a bad request
/// fails before a thread is spawned.
pub(crate) struct CoverContext {
    pub volume_id: String,
    pub writer: IndexWriter,
    pub space: IndexPathSpace,
    /// Which half of [`Ground`] this volume's rows come from. The ONE thing the
    /// walk branches on per kind; everything downstream of a discovered entry is
    /// identical.
    pub kind: IndexVolumeKind,
}

/// Start walking `frontier` on the volume `context` describes.
///
/// The paths are the ones a [`coverage`](crate::Index::coverage) answer named,
/// each taken whole: nothing under a frontier node is covered, so there is no
/// pruning to do inside one. Ground another walk on this volume is already
/// covering is left to it and reported as
/// [`covered_by_another_walk`](CoverWalk::covered_by_another_walk).
pub(crate) fn start(
    context: CoverContext,
    frontier: Vec<String>,
    dimension: CoverageDimension,
    cancel: CancellationToken,
) -> CoverWalk {
    // Deliberately an irrefutable `let`: a second dimension has to become a
    // compile error here, not a silently-ignored parameter.
    let CoverageDimension::Listing = dimension;

    // Taken on the CALLER's thread, so the answer is already true by the time
    // this returns: a caller that starts two walks in a row can't have the second
    // one claim ground the first hasn't reached the registry with yet.
    let claim = Claim::take(&context.volume_id, frontier);
    let deferred = claim.deferred().to_vec();

    let (sender, batches) = sync_channel(BATCH_QUEUE_DEPTH);
    let walk_cancel = cancel.clone();
    let thread = std::thread::Builder::new()
        .name("index-cover".into())
        .spawn(move || {
            // Yield CPU to the UI, exactly as the full scan does: someone is
            // waiting on the results, but they're waiting on the UI more.
            cmdr_fs::thread_qos::set_current_thread_qos(cmdr_fs::thread_qos::QosClass::Utility);
            // The claim lives as long as the walk and no longer, so its ground
            // frees up on the completion path, the cancel path, and a panic alike.
            let outcome = walk_frontier(&context, claim.mine(), &sender, &walk_cancel);
            drop(claim);
            outcome
        })
        .unwrap_or_else(|e| {
            // A machine that can't spawn a thread has a bigger problem than this
            // walk; report nothing covered rather than pretending otherwise.
            log::warn!("Cover: couldn't spawn the walk thread: {e}");
            std::thread::spawn(|| CoverOutcome {
                entries_found: 0,
                dirs_found: 0,
                roots_covered: 0,
                cancelled: true,
            })
        });

    CoverWalk {
        batches,
        cancel,
        thread,
        deferred,
    }
}

/// Walk every frontier root in turn, on the walk thread.
///
/// The backend's scan session brackets the WHOLE frontier, not each root: over SMB
/// that's a pool of extra connections, and opening one per frontier root would pay
/// the setup repeatedly for the same walk. ❌ Nothing between the two calls may
/// return early — the pairing is what keeps a cancelled walk from leaving the pool
/// standing.
fn walk_frontier(
    context: &CoverContext,
    frontier: &[String],
    sender: &SyncSender<Vec<CoveredEntry>>,
    cancel: &CancellationToken,
) -> CoverOutcome {
    let Some(ground) = Ground::under(context) else {
        log::warn!("Cover: '{}' isn't reachable right now, so nothing is walked", context.volume_id);
        return CoverOutcome {
            entries_found: 0,
            dirs_found: 0,
            roots_covered: 0,
            cancelled: true,
        };
    };

    ground.open_session();
    let outcome = walk_roots(context, &ground, frontier, sender, cancel);
    ground.close_session();

    log::debug!(
        "Cover: {} over {}{}",
        pluralize(outcome.entries_found, "entry"),
        pluralize(outcome.roots_covered as u64, "frontier root"),
        if outcome.cancelled { " (cancelled)" } else { "" },
    );
    outcome
}

/// The frontier loop itself, one root at a time, whatever kind of ground it is.
fn walk_roots(
    context: &CoverContext,
    ground: &Ground,
    frontier: &[String],
    sender: &SyncSender<Vec<CoveredEntry>>,
    cancel: &CancellationToken,
) -> CoverOutcome {
    let mut outcome = CoverOutcome {
        entries_found: 0,
        dirs_found: 0,
        roots_covered: 0,
        cancelled: false,
    };

    for path in frontier {
        if cancel.is_cancelled() {
            outcome.cancelled = true;
            break;
        }
        let root = Path::new(path);
        // Ground the index has no row for can't be resolved to a scan root, and
        // that isn't only a cold volume's problem: a folder created since its
        // parent was last listed has no row on a fully indexed drive either. Give
        // the walk the chain it needs, without claiming a listing for any of it.
        if let Err(e) = bootstrap::ensure_walkable(context, ground, root) {
            log::warn!("Cover: can't walk {path}: {e}");
            continue;
        }
        // A partial walk's totals count exactly as much as a complete one's, so
        // both arms hand the same summary to the same accumulation and only the
        // VERDICT differs. Keeping one `+=` pair is what stops the cancel path
        // drifting from the completion path.
        let (summary, verdict) = ground.cover(context, root, sender, cancel);
        if let Some(summary) = summary {
            outcome.entries_found += summary.total_entries;
            outcome.dirs_found += summary.total_dirs;
        }
        match verdict {
            RootOutcome::Covered => outcome.roots_covered += 1,
            RootOutcome::Cancelled => {
                outcome.cancelled = true;
                break;
            }
            RootOutcome::Failed => {}
        }
    }
    outcome
}

/// How a volume's ground gets read.
///
/// Two halves, and every volume kind falls in one of them: the LOCAL guarded
/// walker reads the disk directly, and everything else is reached only through its
/// [`Volume`]. That's the whole per-kind branch in the coverage concept — the
/// frontier query, the descent rule, the epochs, and the writer are identical on
/// both sides, so a new backend needs no coverage code of its own.
pub(super) enum Ground {
    /// A local filesystem: the boot disk or a plain external mount.
    Local,
    /// A share, a phone, or whatever backend comes next.
    ViaTrait {
        volume: std::sync::Arc<dyn cmdr_fs::volume::Volume>,
        /// The same per-volume listing budget the background scan yields with, so
        /// browsing the share while it walks drops it to one listing in flight.
        pacer: crate::indexing::network_scanner::scan_pace::ScanPacer,
    },
}

impl Ground {
    /// Which half this volume falls in, or `None` when a trait-scanned volume
    /// isn't registered any more (ejected, or a share that dropped between the
    /// coverage answer and the walk).
    fn under(context: &CoverContext) -> Option<Self> {
        if !context.kind.is_trait_scanned() {
            return Some(Ground::Local);
        }
        let volume = crate::indexing::host::volumes::current().get(&context.volume_id)?;
        Some(Ground::ViaTrait {
            volume,
            pacer: crate::indexing::network_scanner::scan_pace::ScanPacer::for_volume(context.volume_id.clone()),
        })
    }

    /// Let the backend open whatever a walk's worth of listings needs (SMB spins up
    /// a small pool of extra connections). Default no-op everywhere else.
    fn open_session(&self) {
        if let Ground::ViaTrait { volume, .. } = self {
            crate::indexing::host::runtime::block_on(volume.begin_scan_session());
        }
    }

    /// Tear it back down, on every outcome. Paired with
    /// [`open_session`](Self::open_session) by the shape of `walk_frontier`.
    fn close_session(&self) {
        if let Ground::ViaTrait { volume, .. } = self {
            crate::indexing::host::runtime::block_on(volume.end_scan_session());
        }
    }

    /// Whether `path` is a directory this walk may descend into, and what to record
    /// for it. `None` for anything else: gone, unreadable, or a symlink.
    pub(super) fn stat_directory(&self, path: &Path) -> Option<crate::indexing::metadata::MetadataSnapshot> {
        match self {
            Ground::Local => {
                let metadata = std::fs::symlink_metadata(path).ok()?;
                // A symlink reports `is_dir() == false` here, which is the answer we
                // want: the index stores symlinks without descending into them.
                metadata
                    .is_dir()
                    .then(|| crate::indexing::metadata::extract_metadata(&metadata, true, false))
            }
            Ground::ViaTrait { volume, .. } => {
                let entry = crate::indexing::host::runtime::block_on(
                    crate::indexing::network_scanner::stat_one_directory(
                        std::sync::Arc::clone(volume),
                        path.to_path_buf(),
                    ),
                )?;
                Some(crate::indexing::metadata::MetadataSnapshot {
                    // A directory's own row carries no size, on every walk here.
                    logical_size: None,
                    physical_size: None,
                    modified_at: entry.modified_at,
                    inode: entry.inode,
                    nlink: None,
                })
            }
        }
    }

    /// Cover one frontier root, and say how it went.
    fn cover(
        &self,
        context: &CoverContext,
        root: &Path,
        sender: &SyncSender<Vec<CoveredEntry>>,
        cancel: &CancellationToken,
    ) -> (Option<ScanSummary>, RootOutcome) {
        match self {
            Ground::Local => {
                match cover_subtree(root, &context.space, &context.writer, Some(sender.clone()), cancel) {
                    Ok(summary) => (Some(summary), RootOutcome::Covered),
                    Err(ScanError::Cancelled(summary)) => (Some(summary), RootOutcome::Cancelled),
                    Err(ScanError::NotVirgin) => (None, repair_non_virgin(context, root, cancel)),
                    Err(e) => {
                        // One unwalkable root doesn't stop the others: it simply stays
                        // frontier, and the next search asks for it again.
                        log::warn!("Cover: couldn't walk {}: {e}", root.display());
                        (None, RootOutcome::Failed)
                    }
                }
            }
            Ground::ViaTrait { volume, pacer } => {
                // ❌ There is no `NotVirgin` arm here, and no repair path to route
                // one to: the trait walk is add-only per directory (it keeps the
                // rows a name already has), so ground an earlier walk touched is
                // simply walked. Over a network round trip the per-directory name
                // check is free; over a local `readdir` it wouldn't be, which is why
                // the two halves differ here and nowhere else.
                let result = crate::indexing::host::runtime::block_on(
                    crate::indexing::network_scanner::cover_volume_subtree(
                        std::sync::Arc::clone(volume),
                        root.to_path_buf(),
                        &context.space,
                        &context.writer,
                        Some(sender.clone()),
                        cancel,
                        pacer,
                    ),
                );
                match result {
                    Ok(summary) => (Some(summary), RootOutcome::Covered),
                    Err(VolumeScanError::Cancelled(summary)) => (Some(summary), RootOutcome::Cancelled),
                    Err(e) => {
                        log::warn!("Cover: couldn't walk {}: {e}", root.display());
                        (None, RootOutcome::Failed)
                    }
                }
            }
        }
    }
}

/// The repair path for a frontier node the index already holds rows under.
///
/// Rare — it takes a verification pass writing children under a directory nothing
/// listed — and unsafe for the parallel walker, whose fresh ids would collide.
/// The serial reconcile compares by name and writes only differences, so it can
/// take the case without deleting anything.
fn repair_non_virgin(context: &CoverContext, root: &Path, cancel: &CancellationToken) -> RootOutcome {
    let db_path = context.writer.db_path();
    let conn = match IndexStore::open_read_connection(&db_path) {
        Ok(conn) => conn,
        Err(e) => {
            log::warn!("Cover: couldn't open a connection to repair {}: {e}", root.display());
            return RootOutcome::Failed;
        }
    };
    match crate::indexing::reconcile::reconciler::reconcile_subtree(
        root,
        &context.space,
        &conn,
        &context.writer,
        cancel,
    ) {
        Ok(summary) => {
            log::debug!(
                "Cover: repaired {} through the serial reconcile (+{} -{} ~{})",
                root.display(),
                summary.added,
                summary.removed,
                summary.updated,
            );
            if summary.cancelled {
                RootOutcome::Cancelled
            } else {
                RootOutcome::Covered
            }
        }
        Err(e) => {
            log::warn!("Cover: couldn't repair {}: {e}", root.display());
            RootOutcome::Failed
        }
    }
}

/// How one frontier root's walk ended, whichever primitive took it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootOutcome {
    /// The node is covered now.
    Covered,
    /// Someone stopped the walk partway. Whatever it listed is still marked, and
    /// the rest of the frontier is left for the next search.
    Cancelled,
    /// It couldn't run. The node stays frontier and the next search asks again.
    Failed,
}

mod bootstrap;
mod live;

pub(crate) use bootstrap::{NoCoverContext, context_for_walk};
use live::Claim;

#[cfg(test)]
mod network_tests;
#[cfg(test)]
mod tests;
