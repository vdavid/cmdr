//! Which primitive reads a frontier root, and how one root's walk ended.
//!
//! Two kinds of ground, and the one branch between them. [`Ground`] is the ONLY
//! thing in the cover walk that asks what kind of volume this is: a local
//! filesystem is read by the guarded walker, and everything the index reaches
//! only through a `Volume` — a share, a phone, whatever backend comes next — by
//! `network_scanner`'s scoped walk. Downstream of a discovered entry the two are
//! the same code: one writer, one set of epochs, one frontier query, one descent
//! rule.
//!
//! Two primitives on the local half, and which one runs. A frontier node is
//! virgin ground by definition, so the workload is a bulk add and the PARALLEL
//! walker wins it outright — measured on a real frontier in
//! `docs/notes/cover-walk-primitive-2026-08-05.md`. It runs by default.
//!
//! The serial reconcile is the repair path, for the one case the parallel walker
//! can't take: a frontier node that ISN'T virgin. Those exist (an FSEvents
//! verification pass writes children under a directory without marking that
//! directory listed), and the parallel walker allocates fresh ids for every name
//! it finds, so over pre-existing rows `INSERT OR IGNORE` would drop its rows
//! silently and orphan everything below them. `reconcile_subtree` compares by
//! name and writes only differences, which is exactly the shape that case needs.
//! The trait walk needs no such split: it compares names per directory as it
//! goes, so it takes that case itself. ❌ No path ever deletes: covering is
//! add-only work.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use cmdr_fs::volume::Volume;
use tokio_util::sync::CancellationToken;

use super::CoverContext;
use crate::indexing::host::runtime;
use crate::indexing::metadata::{MetadataSnapshot, extract_metadata};
use crate::indexing::network_scanner::scan_pace::ScanPacer;
use crate::indexing::network_scanner::{VolumeScanError, cover_volume_subtree, stat_one_directory};
use crate::indexing::scanner::{CoveredEntry, ScanError, ScanSummary, WalkHeartbeat, cover_subtree};
use crate::indexing::store::IndexStore;

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
        volume: Arc<dyn Volume>,
        /// The same per-volume listing budget the background scan yields with, so
        /// browsing the share while it walks drops it to one listing in flight.
        pacer: ScanPacer,
    },
}

impl Ground {
    /// Which half this volume falls in, or `None` when a trait-scanned volume
    /// isn't registered any more (ejected, or a share that dropped between the
    /// coverage answer and the walk).
    pub(super) fn under(context: &CoverContext) -> Option<Self> {
        if !context.kind.is_trait_scanned() {
            return Some(Ground::Local);
        }
        let volume = crate::indexing::host::volumes::current().get(&context.volume_id)?;
        Some(Ground::ViaTrait {
            volume,
            pacer: ScanPacer::for_volume(context.volume_id.clone()),
        })
    }

    /// Let the backend open whatever a walk's worth of listings needs (SMB spins up
    /// a small pool of extra connections). Default no-op everywhere else.
    pub(super) fn open_session(&self) {
        if let Ground::ViaTrait { volume, .. } = self {
            runtime::block_on(volume.begin_scan_session());
        }
    }

    /// Tear it back down, on every outcome. Paired with
    /// [`open_session`](Self::open_session) by the shape of `walk_frontier`.
    pub(super) fn close_session(&self) {
        if let Ground::ViaTrait { volume, .. } = self {
            runtime::block_on(volume.end_scan_session());
        }
    }

    /// Whether `path` is a directory this walk may descend into, and what to record
    /// for it. `None` for anything else: gone, unreadable, or a symlink.
    pub(super) fn stat_directory(&self, path: &Path) -> Option<MetadataSnapshot> {
        match self {
            Ground::Local => {
                let metadata = std::fs::symlink_metadata(path).ok()?;
                // A symlink reports `is_dir() == false` here, which is the answer we
                // want: the index stores symlinks without descending into them.
                metadata.is_dir().then(|| extract_metadata(&metadata, true, false))
            }
            Ground::ViaTrait { volume, .. } => {
                let entry = runtime::block_on(stat_one_directory(Arc::clone(volume), path.to_path_buf()))?;
                Some(MetadataSnapshot {
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
    pub(super) fn cover(
        &self,
        context: &CoverContext,
        root: &Path,
        sender: &SyncSender<Vec<CoveredEntry>>,
        cancel: &CancellationToken,
        heartbeat: &WalkHeartbeat,
    ) -> (Option<ScanSummary>, RootOutcome) {
        match self {
            Ground::Local => {
                match cover_subtree(
                    root,
                    &context.space,
                    &context.writer,
                    Some(sender.clone()),
                    cancel,
                    heartbeat,
                ) {
                    Ok(summary) => (Some(summary), RootOutcome::Covered),
                    Err(ScanError::Cancelled(summary)) => (Some(summary), RootOutcome::Cancelled),
                    Err(ScanError::NotVirgin) => repair_non_virgin(context, root, sender, cancel),
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
                let result = runtime::block_on(cover_volume_subtree(
                    Arc::clone(volume),
                    root.to_path_buf(),
                    &context.space,
                    &context.writer,
                    Some(sender.clone()),
                    cancel,
                    pacer,
                    heartbeat,
                ));
                match result {
                    Ok(summary) => (Some(summary), RootOutcome::Covered),
                    Err(VolumeScanError::Cancelled(summary)) => (Some(summary), RootOutcome::Cancelled),
                    // The one classification that says something about the VOLUME
                    // rather than about this root. ⚠️ Narrower than "the root failed"
                    // on purpose: a `Timeout` is one wedged directory on a share that
                    // is otherwise answering, and an `EmptyRoot` is no health claim
                    // at all.
                    Err(e) if e.is_terminal_disconnect() => {
                        log::warn!(
                            "Cover: '{}' went away while walking {}: {e}; leaving the rest of the frontier for the next search",
                            context.volume_id,
                            root.display(),
                        );
                        (None, RootOutcome::VolumeGone)
                    }
                    Err(e) => {
                        log::warn!("Cover: couldn't walk {}: {e}", root.display());
                        (None, RootOutcome::Failed)
                    }
                }
            }
        }
    }
}

/// The repair path for a frontier node the index already holds rows under: an
/// earlier walk materialized it on its way to a child, or a verification pass
/// wrote children under it without listing it. Unsafe for the parallel walker,
/// whose fresh ids would collide; the serial reconcile compares by name and writes
/// only differences.
///
/// ❌ It reports like every other primitive here, and that is not decoration: a
/// search answers with the index's covered half plus the walk's rows, and the
/// covered half holds NOTHING under a frontier root. `DETAILS.md` § "The repair
/// path REPORTS".
pub(super) fn repair_non_virgin(
    context: &CoverContext,
    root: &Path,
    sender: &SyncSender<Vec<CoveredEntry>>,
    cancel: &CancellationToken,
) -> (Option<ScanSummary>, RootOutcome) {
    let started = std::time::Instant::now();
    let db_path = context.writer.db_path();
    let conn = match IndexStore::open_read_connection(&db_path) {
        Ok(conn) => conn,
        Err(e) => {
            log::warn!("Cover: couldn't open a connection to repair {}: {e}", root.display());
            return (None, RootOutcome::Failed);
        }
    };
    match crate::indexing::reconcile::reconciler::reconcile_subtree(
        root,
        &context.space,
        &conn,
        &context.writer,
        cancel,
        Some(sender),
    ) {
        Ok(summary) => {
            log::debug!(
                "Cover: repaired {} through the serial reconcile (+{} -{} ~{})",
                root.display(),
                summary.added,
                summary.removed,
                summary.updated,
            );
            // Only the rows this pass CREATED: the ones it updated were already
            // the index's to report, so counting them would credit this walk with
            // rows the search had before it started. The cover outcome reads these
            // two counts and nothing else, so the bytes stay 0.
            let scanned = ScanSummary {
                total_entries: summary.added,
                total_dirs: summary.added_dirs,
                total_physical_bytes: 0,
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            };
            let verdict = if summary.cancelled {
                RootOutcome::Cancelled
            } else {
                RootOutcome::Covered
            };
            (Some(scanned), verdict)
        }
        Err(e) => {
            log::warn!("Cover: couldn't repair {}: {e}", root.display());
            (None, RootOutcome::Failed)
        }
    }
}

/// How one frontier root's walk ended, whichever primitive took it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RootOutcome {
    /// The node is covered now.
    Covered,
    /// Someone stopped the walk partway. Whatever it listed is still marked, and
    /// the rest of the frontier is left for the next search.
    Cancelled,
    /// It couldn't run. The node stays frontier and the next search asks again.
    Failed,
    /// The VOLUME is gone, and this root is only where the walk found out. Every
    /// root behind it is on the same volume and the same session, so the frontier
    /// loop stops rather than re-asking a question that can't be answered.
    ///
    /// ⚠️ [`Failed`](Self::Failed) for this root plus a verdict about the rest, and
    /// that is the whole of it: a root the loop skips is walked by nothing, so it is
    /// marked by nothing and stays frontier. ❌ Never write, mark, or count anything
    /// for a skipped root — that would turn "the NAS is asleep" into thousands of
    /// folders written out of search. `DETAILS.md` § "A dead volume is concluded
    /// once, not per root".
    VolumeGone,
}
