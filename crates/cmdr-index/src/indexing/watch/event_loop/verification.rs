//! Post-replay background verification: a bidirectional readdir diff over the
//! directories the replay touched. `run_background_verification` runs off the
//! async pool after live mode starts; `verify_affected_dirs` does the lock-free
//! two-phase DB-vs-disk reconcile. Root-scoped (boot disk only), so it stays on
//! `BootDisk` / `ROOT_VOLUME_ID`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::verify_guard::{self, VerifyVerdict};
use crate::ROOT_VOLUME_ID;
use crate::indexing::DEBUG_STATS;
use crate::indexing::events::emit_dir_updated;
use crate::indexing::hold::VolumeWork;
use crate::indexing::lifecycle::lifecycle_bus;
use crate::indexing::metadata;
use crate::indexing::paths::path_prefix;
use crate::indexing::read::enrichment::get_read_pool;
use crate::indexing::reconcile::reconciler;
use crate::indexing::scanner;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::firmlinks;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::pluralize::{pluralize, pluralize_with};

/// Run post-replay verification in the background.
///
/// Called after live mode starts so the app is responsive immediately.
/// Corrections found by verification go through the writer channel,
/// which serializes them with live writes.
///
/// `work` holds the volume for the task's whole run, and each blocking step
/// carries a share of its own, since the blocking pool doesn't stop with a
/// dropped task.
pub(super) async fn run_background_verification(
    origin_dirs: HashSet<String>,
    writer: IndexWriter,
    events: std::sync::Arc<dyn crate::EventSink>,
    work: VolumeWork,
) {
    DEBUG_STATS.verifying.store(true, Ordering::Relaxed);
    let verify_start = Instant::now();

    // Verification wants the WIDE set: journal replay coalesces events, so a change
    // deep in a tree can surface only as "some ancestor was modified", and readdir'ing
    // the whole chain is what catches it. So expand the replay's origin dirs to their
    // ancestor closure here — the bus publishes the narrow origins instead.
    let origins: Vec<String> = origin_dirs.into_iter().collect();
    let affected_paths: HashSet<String> = path_prefix::with_ancestor_closure(&origins).into_iter().collect();
    log::debug!(
        "Background verification started ({} affected dirs)",
        affected_paths.len(),
    );

    // Verify affected directories: FSEvents journal replay coalesces events,
    // so child deletions may only show as "parent dir modified," and new
    // children may not get individual creation events. Readdir each affected
    // parent and reconcile with DB.
    //
    // Run on the blocking pool: `verify_affected_dirs` is sync (Phase 1 SQLite
    // reads via `ReadPool`, Phase 2 `read_dir`/`symlink_metadata` per child).
    // On a typical home folder it takes seconds. Doing it inline on an async
    // worker pins that worker for the full duration; on macOS it also feeds
    // a burst of writer messages and event emits through the main thread,
    // which competes with user-initiated IPCs like `plugin:window|close`.
    // The blocking pool absorbs the sync work; the async runtime stays free
    // to serve UI requests responsively (top-5 principle #3 — UI must always
    // be responsive).
    let verify_writer = writer.clone();
    let verify_affected_paths = affected_paths.clone();
    let verify_share = work.clone();
    let verify_result = match crate::indexing::host::runtime::spawn_blocking(move || {
        let _verify_share = verify_share;
        verify_affected_dirs(&verify_affected_paths, &verify_writer)
    })
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Background verification: verify_affected_dirs join failed: {e}");
            VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            }
        }
    };

    // Scan newly discovered directories (inserts children + computes subtree aggregates).
    // Skip excluded paths (system dirs like /System, /dev) that aren't in the index.
    if !verify_result.new_dir_paths.is_empty() {
        // Flush first: verify_affected_dirs sent UpsertEntryV2 for each new dir, but those
        // writes are still queued. scan_subtree opens a read connection to resolve the dir's
        // path → entry_id, which fails if the entry isn't committed yet.
        if let Err(e) = writer.flush().await {
            log::warn!("Background verification pre-scan flush failed: {e}");
        }

        // Guarded-walker-based parallel walk + sync writer-channel sends — same blocking-pool
        // reasoning as `verify_affected_dirs` above. A subtree scan can take many
        // seconds and saturates multiple rayon threads; keeping it off the async
        // pool is essential.
        let scan_writer = writer.clone();
        let scan_dirs = verify_result.new_dir_paths.clone();
        let scan_work = work.clone();
        if let Err(e) = crate::indexing::host::runtime::spawn_blocking(move || {
            // Background verification backs FSEvents journal replay, which only a
            // journaled volume (the boot disk) has, so the space is root's.
            let space = crate::indexing::IndexPathSpace::root();
            for dir_path in &scan_dirs {
                if scanner::should_exclude(dir_path, space.exclusion_scope()) {
                    continue;
                }
                match scanner::scan_subtree(Path::new(dir_path), &space, &scan_writer, &scan_work) {
                    Ok(summary) => {
                        log::debug!(
                            "Background verification: scanned new dir path={:?} ({} entries, {}ms)",
                            LogPrivate::dir_path(dir_path),
                            summary.total_entries,
                            summary.duration_ms,
                        );
                    }
                    Err(e) => {
                        log::warn!(
                            "Background verification: scan_subtree({}) failed: {}",
                            LogPrivate::dir_path(dir_path),
                            LogError(&e)
                        );
                    }
                }
            }
        })
        .await
        {
            log::warn!("Background verification: scan_subtree batch join failed: {e}");
        }
    }

    let has_changes =
        verify_result.stale_count > 0 || verify_result.new_file_count > 0 || !verify_result.new_dir_paths.is_empty();

    if has_changes {
        log::debug!(
            "Background verification found {} stale, {} new files, {} new dirs; flushing",
            verify_result.stale_count,
            verify_result.new_file_count,
            verify_result.new_dir_paths.len(),
        );
        if let Err(e) = writer.flush().await {
            log::warn!("Background verification flush failed: {e}");
        }

        // Tell the UI about the newly-scanned subtrees so open listings can
        // refresh them. Coalesced into a single emit: the scan loop above
        // already finished all subtrees before we get here (the loop is
        // synchronous), so emitting per-path here only paid the per-emit
        // macOS main-thread cost N times without giving the FE any new info.
        // The FE handler is throttled at 2 s per pane anyway, so N separate
        // emits and one batched emit produce the same UX. This keeps the main
        // thread free for user-initiated IPCs like `plugin:window|close`.
        // (Was the post-commit-66712c2d "1.83 TB ghost-size" fix; the
        // `affected_paths` problem it solved persists — we just batch the
        // emit instead of looping it.)
        let visible_new_dirs: Vec<String> = verify_result
            .new_dir_paths
            .iter()
            .filter(|p| !scanner::should_exclude(p, &scanner::ExclusionScope::boot_disk()))
            .cloned()
            .collect();
        if !visible_new_dirs.is_empty() {
            // Background verification is root-scoped (uses the root read pool), so
            // its live corrections publish under the local root for the importance
            // scheduler's incremental rescore (plan Decision 5).
            lifecycle_bus::publish_dirs_changed(ROOT_VOLUME_ID, &visible_new_dirs);
            emit_dir_updated(events.as_ref(), visible_new_dirs);
        }

        // No off-writer ancestor compensation for the new dirs: each `scan_subtree`
        // above sent `ComputeSubtreeAggregates`, whose handler queues the ancestor
        // chain (sizes, counts, symlinks, AND coverage — which this path never
        // corrected before) for the writer's roll-up drain, race-free and without
        // the 2× credit a read-then-`PropagateDeltaById` here caused (Leak A).
        // ⚠️ The `has_changes` flush above does NOT mean the roll-up landed: it runs
        // at the writer's caught-up point (`writer/pending_rollups.rs`), which emits
        // its own refresh for the sizes it moves.

        // Final emit for the replay-affected paths whose stats were corrected
        // (stale-row deletions and new-file additions in the affected_paths set).
        // `new_dir_paths` are not included here — they were already emitted
        // progressively above as each subtree's scan finished.
        if !affected_paths.is_empty() {
            // The bus gets the ORIGINS (the dirs whose own listings the replay
            // changed); the FE emit gets the ancestor closure, whose recursive sizes
            // the corrections moved.
            lifecycle_bus::publish_dirs_changed(ROOT_VOLUME_ID, &origins);
            emit_dir_updated(events.as_ref(), affected_paths.into_iter().collect());
        }
    }

    DEBUG_STATS.verifying.store(false, Ordering::Relaxed);
    log::debug!(
        "Background verification completed in {}ms",
        verify_start.elapsed().as_millis(),
    );
}

/// Phase 1's snapshot: affected parent path → (its entry id, its DB children).
type DbSnapshot = HashMap<String, (i64, Vec<store::EntryRow>)>;

/// Result of `verify_affected_dirs`.
struct VerifyResult {
    /// Entries in DB but not on disk (deleted).
    stale_count: u64,
    /// Files on disk but not in DB (inserted with delta propagation).
    new_file_count: u64,
    /// Directories on disk but not in DB (inserted, need subtree scan by caller).
    new_dir_paths: Vec<String>,
}

/// Verify that DB entries for affected directories match what's on disk.
///
/// FSEvents journal replay coalesces events: child deletions may appear as
/// "parent directory modified" without individual removal events. Similarly,
/// new children may not get individual creation events.
///
/// Two-phase approach, no `INDEXING` lock needed:
///
/// **Phase 1 (ReadPool, no lock):** Resolve each affected path to its entry ID,
/// list children as `EntryRow` (integer-keyed), and snapshot into a `HashMap`.
/// Uses `get_read_pool()` + `pool.with_conn()` for lock-free DB reads.
///
/// **Phase 2 (no lock):** Walk the snapshot, check the filesystem
/// (`Path::exists`, `read_dir`, `symlink_metadata`), and send corrections to
/// the writer channel using integer-keyed write messages:
/// 1. **Stale entries**: DB children that no longer exist on disk get
///    `DeleteEntryById`/`DeleteSubtreeById` (auto-propagates deltas).
/// 2. **Missing entries**: Disk children not in DB get `UpsertEntryV2`. New files also get
///    `PropagateDeltaById`. New directories are collected in `new_dir_paths` for the caller to scan
///    via `scan_subtree`.
///
/// **Both phases are cost-guarded** (`verify_guard`): a directory with more than
/// `HUGE_DIR_CHILDREN` index children is declined before Phase 1 snapshots it, and
/// Phase 2's `read_dir` loop stops after that many iterations. See
/// `indexing/DETAILS.md` § "Bounding verification cost (the two teeth)" for the
/// trade this makes.
fn verify_affected_dirs(affected_paths: &HashSet<String>, writer: &IndexWriter) -> VerifyResult {
    verify_affected_dirs_with(affected_paths, writer, verify_guard::HUGE_DIR_CHILDREN)
}

/// [`verify_affected_dirs`] with the guard threshold injected, so tests can drive
/// both teeth with tiny fixtures instead of a million-file directory.
fn verify_affected_dirs_with(affected_paths: &HashSet<String>, writer: &IndexWriter, threshold: usize) -> VerifyResult {
    // ── Phase 1: Bulk-read DB state via ReadPool (no lifecycle/registry lock) ──
    // Snapshot: parent_path → (parent_id, Vec<EntryRow>)
    let pool = match get_read_pool() {
        Some(p) => p,
        None => {
            return VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            };
        }
    };

    let (db_snapshot, declined): (DbSnapshot, Vec<String>) = match pool.with_conn(|conn| {
        let mut snapshot = HashMap::with_capacity(affected_paths.len());
        let mut declined = Vec::new();
        for parent_path in affected_paths {
            let parent_id = match store::resolve_path(conn, parent_path) {
                Ok(Some(id)) => id,
                _ => continue, // Path not in index, skip
            };
            // ── Guard tooth 1 ────────────────────────────────────────
            // Probe the child count with `LIMIT threshold + 1` BEFORE
            // `list_children_on`. The snapshot below owns every `EntryRow` it
            // reads, so a 1.14M-child directory costs hundreds of MB and
            // minutes of writer traffic before a single child is examined —
            // the guard has to sit here, not around the upsert.
            // A probe that errors falls through to the diff: refusing to
            // verify on a transient read failure would be the worse default.
            let probe =
                IndexStore::count_children_capped(parent_id, conn, verify_guard::probe_limit(threshold)).unwrap_or(0);
            if verify_guard::classify_db_children(probe, threshold) == VerifyVerdict::Decline {
                declined.push(parent_path.clone());
                continue;
            }
            match IndexStore::list_children_on(parent_id, conn) {
                Ok(entries) => {
                    snapshot.insert(parent_path.clone(), (parent_id, entries));
                }
                Err(_) => {
                    // Insert empty vec so Phase 2 still checks disk for new entries
                    snapshot.insert(parent_path.clone(), (parent_id, Vec::new()));
                }
            }
        }
        (snapshot, declined)
    }) {
        Ok(pair) => pair,
        Err(e) => {
            log::warn!("verify_affected_dirs: ReadPool error: {e}");
            return VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            };
        }
    };

    if !declined.is_empty() {
        DEBUG_STATS
            .verify_declined_dirs
            .fetch_add(declined.len() as u64, Ordering::Relaxed);
        // ❌ Do NOT mark a declined dir unlisted here. Affected dirs carry a
        // positive `listed_epoch` from the scan, and `absorbing_min_epoch`
        // propagates a zero to every ancestor, so one declined temp directory
        // would render the whole home folder incomplete and make `expected_totals`
        // return `None` for every copy of `~`. Leave the epoch untouched.
        //
        // One line per episode with a bounded sample: the declined set can be
        // hundreds of paths, and this runs on the cold-start path.
        log::info!(
            "verify_affected_dirs: declined {} (over {} index children, a diff would cost O(children)): {}",
            pluralize(declined.len() as u64, "dir"),
            threshold,
            declined.iter().take(10).cloned().collect::<Vec<_>>().join(", "),
        );
    }

    // ── Phase 2: Filesystem checks without the lock ──────────────────
    let mut stale_count = 0u64;
    let mut new_file_count = 0u64;
    let mut new_dir_paths = Vec::<String>::new();

    for (parent_path, (parent_id, db_children)) in &db_snapshot {
        // Build a set of normalized DB child names for fast lookup
        let db_child_names: HashSet<String> = db_children
            .iter()
            .map(|c| store::normalize_for_comparison(&c.name))
            .collect();

        // Build child path from parent_path + name for filesystem checks
        let parent_prefix = if parent_path == "/" {
            String::new()
        } else {
            parent_path.clone()
        };

        // ⚠️ **The parent's own listing FIRST, before any child is probed.** A
        // directory we couldn't read tells us nothing about what is in it, and the
        // probe below reads every failure as "gone" — so probing first reaps the
        // entire listing of a directory that merely went unreadable. A directory the
        // user really deleted still loses its rows, from its PARENT's pass, whose
        // listing genuinely stops mentioning it.
        let read_dir = match std::fs::read_dir(parent_path) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        // ❗ **These deletes carry no drive-presence gate and no `deletes::batch_sent`,
        // and that is sound ONLY because this path is boot-disk-only.** Verification
        // runs off journal replay, which is gated on `has_event_journal()` (`Local`, the
        // boot disk), and the boot-disk scan excludes `/Volumes/` outright
        // (`scanner/exclusions.rs`), so no row reachable from here sits on a drive that
        // can leave the mount table. ❌ The moment verification runs for any other
        // volume kind, gather these into a batch and send it the way
        // `replay.rs::send_replay_deletes` does — a presence read AFTER the probes, then
        // `deletes::batch_sent` — or a drive pulled mid-pass reaps its whole index and
        // nothing marks the index for a rebuild.
        //
        // Detect stale entries (in DB but not on disk)
        for child in db_children {
            let child_path = format!("{}/{}", parent_prefix, child.name);
            // Errno-typed, ❌ never `Path::exists()`, which collapses every error to
            // `false`: only a child that is really absent may be swept, and a
            // permission wall or an I/O error means we never got to look.
            match std::fs::symlink_metadata(&child_path) {
                Ok(_) => continue,
                Err(e) if !reconciler::child_is_absent(&e) => continue,
                Err(_) => {}
            }
            if child.is_directory {
                let _ = writer.send(WriteMessage::DeleteSubtreeById(child.id));
            } else {
                let _ = writer.send(WriteMessage::DeleteEntryById(child.id));
            }
            stale_count += 1;
        }

        // ── Guard tooth 2 ────────────────────────────────────────────────
        // Cap ITERATIONS, not upserts. The loop below `continue`s past every
        // DB-known child before doing any work, so an already-indexed
        // pathological directory produces ~zero upserts while iterating 1.14M
        // times — an upsert cap would be a no-op on the measured incident. This
        // tooth also covers the directory that is small in the DB (so it passes
        // tooth 1) but huge on disk.
        for (iterations, dir_entry) in read_dir.flatten().enumerate() {
            if verify_guard::classify_iteration(iterations, threshold) == VerifyVerdict::Decline {
                DEBUG_STATS.verify_truncated_dirs.fetch_add(1, Ordering::Relaxed);
                log::info!(
                    "verify_affected_dirs: stopped diffing {parent_path} after {} on disk (partial diff)",
                    pluralize_with(threshold as u64, "entry", "entries"),
                    parent_path = LogPrivate::dir_path(&parent_path),
                );
                break;
            }

            let child_path = dir_entry.path();
            let child_path_str = child_path.to_string_lossy().to_string();
            let normalized = firmlinks::normalize_path(&child_path_str);

            let name = dir_entry.file_name().to_string_lossy().to_string();
            if db_child_names.contains(&store::normalize_for_comparison(&name)) {
                continue;
            }

            // Skip excluded system paths (e.g. /System, /dev, /Volumes).
            // Root-scoped background verification (boot disk), so `BootDisk`.
            if scanner::should_exclude(&normalized, &scanner::ExclusionScope::boot_disk()) {
                continue;
            }

            let metadata = match std::fs::symlink_metadata(&child_path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_dir = metadata.is_dir();
            let is_symlink = metadata.is_symlink();
            let snap = metadata::extract_metadata(&metadata, is_dir, is_symlink);

            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: *parent_id,
                name,
                is_directory: is_dir,
                is_symlink,
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode: snap.inode,
                nlink: snap.nlink,
            });

            // UpsertEntryV2 auto-propagates deltas in the writer.
            if is_dir {
                log::debug!("verify_affected_dirs: new dir on disk: {normalized} (parent_id={parent_id})");
                new_dir_paths.push(normalized);
            } else {
                new_file_count += 1;
            }
        }
    }

    if stale_count > 0 || new_file_count > 0 || !new_dir_paths.is_empty() {
        log::debug!(
            "Replay verification: {stale_count} stale, {}, {} across {}",
            pluralize(new_file_count, "new file"),
            pluralize(new_dir_paths.len() as u64, "new dir"),
            pluralize(affected_paths.len() as u64, "affected dir"),
        );
    }

    VerifyResult {
        stale_count,
        new_file_count,
        new_dir_paths,
    }
}

#[cfg(test)]
#[path = "verification_tests.rs"]
mod tests;
