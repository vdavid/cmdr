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
use crate::indexing::read::coverage::CoverageDimension;
use crate::indexing::scanner::{CoveredEntry, ScanError, cover_subtree};
use crate::indexing::store::IndexStore;
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
}

impl CoverWalk {
    /// The next batch of entries, blocking until one arrives. `None` once the
    /// walk has ended, for whatever reason.
    pub fn next_batch(&self) -> Option<Vec<CoveredEntry>> {
        self.batches.recv().ok()
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
    pub writer: IndexWriter,
    pub space: IndexPathSpace,
}

/// Start walking `frontier` on the volume `context` describes.
///
/// The paths are the ones a [`coverage`](crate::Index::coverage) answer named,
/// each taken whole: nothing under a frontier node is covered, so there is no
/// pruning to do inside one.
pub(crate) fn start(
    context: CoverContext,
    frontier: Vec<String>,
    dimension: CoverageDimension,
    cancel: CancellationToken,
) -> CoverWalk {
    // Deliberately an irrefutable `let`: a second dimension has to become a
    // compile error here, not a silently-ignored parameter.
    let CoverageDimension::Listing = dimension;

    let (sender, batches) = sync_channel(BATCH_QUEUE_DEPTH);
    let walk_cancel = cancel.clone();
    let thread = std::thread::Builder::new()
        .name("index-cover".into())
        .spawn(move || {
            // Yield CPU to the UI, exactly as the full scan does: someone is
            // waiting on the results, but they're waiting on the UI more.
            cmdr_fs::thread_qos::set_current_thread_qos(cmdr_fs::thread_qos::QosClass::Utility);
            walk_frontier(&context, &frontier, &sender, &walk_cancel)
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
    }
}

/// Walk every frontier root in turn, on the walk thread.
fn walk_frontier(
    context: &CoverContext,
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
        // A partial walk's totals count exactly as much as a complete one's, so
        // both arms hand the same summary to the same accumulation and only the
        // VERDICT differs. Keeping one `+=` pair is what stops the cancel path
        // drifting from the completion path.
        let (summary, verdict) =
            match cover_subtree(root, &context.space, &context.writer, Some(sender.clone()), cancel) {
                Ok(summary) => (Some(summary), RootOutcome::Covered),
                Err(ScanError::Cancelled(summary)) => (Some(summary), RootOutcome::Cancelled),
                Err(ScanError::NotVirgin) => (None, repair_non_virgin(context, root, cancel)),
                Err(e) => {
                    // One unwalkable root doesn't stop the others: it simply stays
                    // frontier, and the next search asks for it again.
                    log::warn!("Cover: couldn't walk {path}: {e}");
                    (None, RootOutcome::Failed)
                }
            };
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

    log::debug!(
        "Cover: {} over {}{}",
        pluralize(outcome.entries_found, "entry"),
        pluralize(outcome.roots_covered as u64, "frontier root"),
        if outcome.cancelled { " (cancelled)" } else { "" },
    );
    outcome
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

#[cfg(test)]
mod tests;
