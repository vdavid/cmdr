//! Post-replay background verification: a bidirectional readdir diff over the
//! directories the replay touched. `run_background_verification` runs off the
//! async pool after live mode starts; `verify_affected_dirs` does the lock-free
//! two-phase DB-vs-disk reconcile. Root-scoped (boot disk only), so it stays on
//! `BootDisk` / `ROOT_VOLUME_ID`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use tauri::AppHandle;

use super::super::DEBUG_STATS;
use super::super::ROOT_VOLUME_ID;
use super::super::enrichment::get_read_pool;
use super::super::firmlinks;
use super::super::lifecycle_bus;
use super::super::metadata;
use super::super::reconciler;
use super::super::scanner;
use super::super::store::{self, IndexStore};
use super::super::writer::{IndexWriter, WriteMessage};
use crate::pluralize::pluralize;

/// Run post-replay verification in the background.
///
/// Called after live mode starts so the app is responsive immediately.
/// Corrections found by verification go through the writer channel,
/// which serializes them with live writes.
pub(super) async fn run_background_verification(affected_paths: HashSet<String>, writer: IndexWriter, app: AppHandle) {
    DEBUG_STATS.verifying.store(true, Ordering::Relaxed);
    let verify_start = Instant::now();
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
    let verify_result = match tauri::async_runtime::spawn_blocking(move || {
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
        if let Err(e) = tauri::async_runtime::spawn_blocking(move || {
            let cancelled = AtomicBool::new(false);
            for dir_path in &scan_dirs {
                // Background verification is root-scoped (boot disk), so `BootDisk`.
                if scanner::should_exclude(dir_path, scanner::ExclusionScope::BootDisk) {
                    continue;
                }
                match scanner::scan_subtree(Path::new(dir_path), &scan_writer, &cancelled) {
                    Ok(summary) => {
                        log::debug!(
                            "Background verification: scanned new dir {dir_path} ({} entries, {}ms)",
                            summary.total_entries,
                            summary.duration_ms,
                        );
                    }
                    Err(e) => {
                        log::warn!("Background verification: scan_subtree({dir_path}) failed: {e}");
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
            .filter(|p| !scanner::should_exclude(p, scanner::ExclusionScope::BootDisk))
            .cloned()
            .collect();
        if !visible_new_dirs.is_empty() {
            // Background verification is root-scoped (uses the root read pool), so
            // its live corrections publish under the local root for the importance
            // scheduler's incremental rescore (plan Decision 5).
            lifecycle_bus::publish_dirs_changed(ROOT_VOLUME_ID, &visible_new_dirs);
            reconciler::emit_dir_updated(&app, visible_new_dirs);
        }

        // No off-writer ancestor compensation for the new dirs: each `scan_subtree`
        // above sent `ComputeSubtreeAggregates`, whose handler repairs the ancestor
        // chain (sizes, counts, symlinks, AND coverage — which this path never
        // corrected before) on the writer thread, race-free and without the 2×
        // credit a read-then-`PropagateDeltaById` here caused (Leak A). The
        // repairs already committed under the `has_changes` flush above.

        // Final emit for the replay-affected paths whose stats were corrected
        // (stale-row deletions and new-file additions in the affected_paths set).
        // `new_dir_paths` are not included here — they were already emitted
        // progressively above as each subtree's scan finished.
        if !affected_paths.is_empty() {
            let changed: Vec<String> = affected_paths.into_iter().collect();
            lifecycle_bus::publish_dirs_changed(ROOT_VOLUME_ID, &changed);
            reconciler::emit_dir_updated(&app, changed);
        }
    }

    DEBUG_STATS.verifying.store(false, Ordering::Relaxed);
    log::debug!(
        "Background verification completed in {}ms",
        verify_start.elapsed().as_millis(),
    );
}

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
fn verify_affected_dirs(affected_paths: &HashSet<String>, writer: &IndexWriter) -> VerifyResult {
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

    let db_snapshot: HashMap<String, (i64, Vec<store::EntryRow>)> = match pool.with_conn(|conn| {
        let mut snapshot = HashMap::with_capacity(affected_paths.len());
        for parent_path in affected_paths {
            let parent_id = match store::resolve_path(conn, parent_path) {
                Ok(Some(id)) => id,
                _ => continue, // Path not in index, skip
            };
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
        snapshot
    }) {
        Ok(snapshot) => snapshot,
        Err(e) => {
            log::warn!("verify_affected_dirs: ReadPool error: {e}");
            return VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            };
        }
    };

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

        // Detect stale entries (in DB but not on disk)
        for child in db_children {
            let child_path = format!("{}/{}", parent_prefix, child.name);
            if !Path::new(&child_path).exists() {
                if child.is_directory {
                    let _ = writer.send(WriteMessage::DeleteSubtreeById(child.id));
                } else {
                    let _ = writer.send(WriteMessage::DeleteEntryById(child.id));
                }
                stale_count += 1;
            }
        }

        // Detect missing entries (on disk but not in DB)
        let read_dir = match std::fs::read_dir(parent_path) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        for dir_entry in read_dir.flatten() {
            let child_path = dir_entry.path();
            let child_path_str = child_path.to_string_lossy().to_string();
            let normalized = firmlinks::normalize_path(&child_path_str);

            let name = dir_entry.file_name().to_string_lossy().to_string();
            if db_child_names.contains(&store::normalize_for_comparison(&name)) {
                continue;
            }

            // Skip excluded system paths (e.g. /System, /dev, /Volumes).
            // Root-scoped background verification (boot disk), so `BootDisk`.
            if scanner::should_exclude(&normalized, scanner::ExclusionScope::BootDisk) {
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
