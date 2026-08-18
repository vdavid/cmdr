//! Per-navigation background readdir diff.
//!
//! After each directory navigation, compares disk reality against the index DB
//! and corrects any drift. Runs asynchronously, deduplicated and debounced.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use crate::indexing::IndexPathSpace;
use crate::indexing::events::emit_dir_updated;
use crate::indexing::lifecycle::lifecycle_bus;
use crate::indexing::metadata::extract_metadata;
use crate::indexing::read::enrichment::get_read_pool_for;
use crate::indexing::scanner;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::writer::{IndexWriter, WriteMessage};

// ── Dedup/debounce state ─────────────────────────────────────────────

struct VerifierState {
    in_flight: HashSet<String>,
    recent: Vec<(String, Instant)>,
}

static VERIFIER_STATE: LazyLock<Mutex<VerifierState>> = LazyLock::new(|| {
    Mutex::new(VerifierState {
        in_flight: HashSet::new(),
        recent: Vec::new(),
    })
});

const VERIFY_DEBOUNCE_SECS: u64 = 30;
const MAX_CONCURRENT_VERIFICATIONS: usize = 2;

/// RAII guard that frees a path's `in_flight` slot when dropped.
///
/// Constructed right after `in_flight.insert(dir_path)`. The verification body
/// (`verify_and_correct` + `emit_dir_updated`) runs in a spawned task that the
/// tokio runtime catches on panic, so a panic there would otherwise skip the
/// post-`await` `in_flight.remove` and permanently leak the slot against
/// `MAX_CONCURRENT_VERIFICATIONS`. Routing the removal through `Drop` frees the
/// slot on unwind too. Mirrors `write_operations`'s `WriteSettledGuard` pattern.
struct InFlightGuard {
    dir_path: String,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = VERIFIER_STATE.lock() {
            state.in_flight.remove(&self.dir_path);
            state.recent.push((self.dir_path.clone(), Instant::now()));
        }
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Attempt to verify a directory against the index. Checks dedup/debounce,
/// spawns an async task if the directory qualifies.
///
/// `volume_id` and `space` name ONE volume, and they must be the same one
/// `writer` belongs to: the read half routes by volume id and the path half by the
/// space's mount root, so a mismatch either reads an index that can't hold the path
/// (a silent no-op) or writes corrections derived from another volume's rows. The
/// caller takes all three off the same running instance (`trigger_verification`).
pub(crate) fn maybe_verify(
    volume_id: String,
    dir_path: String,
    space: IndexPathSpace,
    writer: IndexWriter,
    events: std::sync::Arc<dyn crate::EventSink>,
    scanning: bool,
    cancel: CancellationToken,
) {
    if scanning {
        return;
    }

    let mut state = match VERIFIER_STATE.lock() {
        Ok(s) => s,
        Err(_) => return,
    };

    // Prune expired recent entries
    let now = Instant::now();
    state
        .recent
        .retain(|(_, ts)| now.duration_since(*ts).as_secs() < VERIFY_DEBOUNCE_SECS);

    // Debounce: skip if recently verified
    if state.recent.iter().any(|(p, _)| p == &dir_path) {
        return;
    }

    // Dedup: skip if already in flight
    if state.in_flight.contains(&dir_path) {
        return;
    }

    // Concurrency limit
    if state.in_flight.len() >= MAX_CONCURRENT_VERIFICATIONS {
        return;
    }

    state.in_flight.insert(dir_path.clone());
    drop(state);

    crate::indexing::host::runtime::spawn(async move {
        // Free the `in_flight` slot (and record the debounce) on every exit
        // path, including a panic inside the body that the runtime catches.
        let _slot = InFlightGuard {
            dir_path: dir_path.clone(),
        };

        let affected_paths = verify_and_correct(&volume_id, &dir_path, &space, &writer, &cancel).await;

        if !affected_paths.is_empty() {
            // Corrections publish under the volume they were read from and written
            // to, for the importance scheduler's incremental rescore (plan Decision
            // 5), alongside the FE emit (which carries absolute paths, to match pane
            // paths on every volume).
            lifecycle_bus::publish_dirs_changed(&volume_id, &affected_paths);
            emit_dir_updated(events.as_ref(), affected_paths);
        }
    });
}

/// Clear all dedup/debounce state. Called on shutdown and clear_index.
pub(crate) fn invalidate() {
    if let Ok(mut state) = VERIFIER_STATE.lock() {
        state.in_flight.clear();
        state.recent.clear();
    }
}

// ── Core verification ────────────────────────────────────────────────

/// Whether this directory belongs to a walk that hasn't got here yet, in which
/// case the verifier leaves it alone.
///
/// Two facts have to hold together. **Nothing has listed the directory**
/// (`listed_epoch == 0`), so its rows are a lower bound rather than its contents,
/// and diffing them would treat every name on disk as new — writing children under
/// a directory nothing marked, which is precisely the non-virgin node that sends a
/// later cover walk down the serial repair path, and running a full recursive
/// `scan_subtree` per new subdirectory to get there. **And the volume still has a
/// frontier**, meaning no scan has completed on it, so a walk genuinely is coming.
///
/// ⚠️ The second half is what keeps this from swallowing a repair nobody else
/// makes. A directory the reconcile cost budget SKIPPED has the same
/// `listed_epoch == 0` and no cause; on a volume whose scan completed, no walk is
/// coming for it and this pass is the only thing that heals it.
///
/// This restores exactly the behavior uncovered ground had before the stitch gave
/// every frontier root a row: back then the verifier bailed because there was no
/// row to resolve. Being a property of the DATABASE rather than a runtime flag is
/// the point — it holds between launch and the first walk, and while drive
/// indexing is off, where no flag would be set.
fn is_the_walks_to_cover(conn: &rusqlite::Connection, dir_id: i64) -> bool {
    let listed = IndexStore::get_listed_epoch_by_id(conn, dir_id)
        .ok()
        .flatten()
        .unwrap_or(0);
    if listed > 0 {
        return false;
    }
    IndexStore::get_meta(conn, "scan_completed_at").ok().flatten().is_none()
}

struct DiskEntry {
    name: String,
    is_dir: bool,
    is_symlink: bool,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    nlink: Option<u64>,
}

/// Compare disk contents of `dir_path` against `volume_id`'s index DB, sending
/// corrections to that volume's writer. New directories are scanned via
/// `scan_subtree`.
/// Returns the list of affected ABSOLUTE paths (for UI refresh), empty if no changes.
///
/// Every path here stays absolute — the disk reads, the exclusion checks, the
/// returned set — and crosses into index-relative space at exactly one point, the
/// `resolve_abs` below (`../paths/CLAUDE.md` § three path spaces).
async fn verify_and_correct(
    volume_id: &str,
    dir_path: &str,
    space: &IndexPathSpace,
    writer: &IndexWriter,
    cancel: &CancellationToken,
) -> Vec<String> {
    let normalized = space.absolute(dir_path);

    // Phase 1: read DB state via THIS volume's ReadPool. `None` means the volume
    // has no registered index, which is the read path's skip signal.
    let pool = match get_read_pool_for(volume_id) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let (parent_id, db_children) = match pool.with_conn(|conn| {
        let parent_id = match space.resolve_abs(conn, &normalized) {
            Ok(Some(id)) => id,
            _ => return None,
        };
        if is_the_walks_to_cover(conn, parent_id) {
            return None;
        }
        match IndexStore::list_children_on(parent_id, conn) {
            Ok(entries) => Some((parent_id, entries)),
            Err(_) => Some((parent_id, Vec::new())),
        }
    }) {
        Ok(Some(result)) => result,
        _ => return Vec::new(),
    };

    // Phase 2: read disk entries.
    // Offload the `read_dir` + per-entry `symlink_metadata` loop onto a blocking
    // thread. This task runs on a plain tokio worker (spawned via
    // the host runtime seam's `spawn`, not `spawn_blocking`), so a slow/hung disk
    // here would otherwise stall an async executor thread. The diff that follows
    // is pure CPU and stays on the async path.
    let disk_map: HashMap<String, DiskEntry> = {
        let scan_path = normalized.clone();
        // Snapshots cross into the DB through here, so this is where a FAT/exFAT
        // volume's derived inode is dropped: an unstable inode reaching the index
        // would drive the live rename pre-pass into a false `MoveEntryV2`.
        let inode_space = space.clone();
        // The closure returns `Option`: `None` distinguishes a `read_dir` failure
        // (bail, exactly as the old synchronous code did) from a genuinely empty
        // directory (`Some(empty map)`, which the diff below treats as "all DB
        // children are stale").
        let joined = tokio::task::spawn_blocking(move || {
            let disk_entries = std::fs::read_dir(&scan_path).ok()?;
            let mut disk_map: HashMap<String, DiskEntry> = HashMap::new();
            for dir_entry in disk_entries.flatten() {
                let name = dir_entry.file_name().to_string_lossy().to_string();
                let metadata = match std::fs::symlink_metadata(dir_entry.path()) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let is_dir = metadata.is_dir();
                let is_symlink = metadata.is_symlink();
                let snap = extract_metadata(&metadata, is_dir, is_symlink);

                let key = store::normalize_for_comparison(&name);
                disk_map.insert(
                    key,
                    DiskEntry {
                        name,
                        is_dir,
                        is_symlink,
                        logical_size: snap.logical_size,
                        physical_size: snap.physical_size,
                        modified_at: snap.modified_at,
                        inode: inode_space.trust_inode(snap.inode),
                        nlink: snap.nlink,
                    },
                );
            }
            Some(disk_map)
        })
        .await;
        match joined {
            Ok(Some(map)) => map,
            Ok(None) => return Vec::new(),
            Err(e) => {
                log::warn!("Verifier: disk-scan task failed: {e}");
                return Vec::new();
            }
        }
    };

    // Build name-keyed map of DB children
    let mut db_map: HashMap<String, &store::EntryRow> = HashMap::with_capacity(db_children.len());
    for child in &db_children {
        let key = store::normalize_for_comparison(&child.name);
        db_map.insert(key, child);
    }

    // Phase 3: diff
    let mut stale_count: u64 = 0;
    let mut new_file_count: u64 = 0;
    let mut new_dir_paths: Vec<String> = Vec::new();
    let mut modified_count: u64 = 0;
    let mut samples: Vec<String> = Vec::new();

    let parent_prefix = if normalized == "/" {
        String::new()
    } else {
        normalized.clone()
    };

    // Stale entries (in DB but not on disk)
    for (key, db_entry) in &db_map {
        if !disk_map.contains_key(key) {
            if db_entry.is_directory {
                let _ = writer.send(WriteMessage::DeleteSubtreeById(db_entry.id));
            } else {
                let _ = writer.send(WriteMessage::DeleteEntryById(db_entry.id));
            }
            stale_count += 1;
            if samples.len() < 5 {
                samples.push(format!("-{}", db_entry.name));
            }
        }
    }

    // New and modified entries (on disk but not in DB, or changed)
    for (key, disk_entry) in &disk_map {
        match db_map.get(key) {
            None => {
                // Skip excluded paths, under THIS volume's scope: `BootDisk` gates
                // `/System`, `/dev`, `/Volumes`; a mount-rooted volume gates only
                // junk basenames, since the boot tier would exclude its own subtree.
                let child_path = format!("{}/{}", parent_prefix, disk_entry.name);
                if scanner::should_exclude(&child_path, space.exclusion_scope()) {
                    continue;
                }

                // New entry on disk
                let _ = writer.send(WriteMessage::UpsertEntryV2 {
                    parent_id,
                    name: disk_entry.name.clone(),
                    is_directory: disk_entry.is_dir,
                    is_symlink: disk_entry.is_symlink,
                    logical_size: disk_entry.logical_size,
                    physical_size: disk_entry.physical_size,
                    modified_at: disk_entry.modified_at,
                    inode: disk_entry.inode,
                    nlink: disk_entry.nlink,
                });

                // UpsertEntryV2 auto-propagates deltas in the writer.
                if disk_entry.is_dir {
                    let new_dir = format!("{}/{}", parent_prefix, disk_entry.name);
                    new_dir_paths.push(new_dir);
                    if samples.len() < 5 {
                        samples.push(format!("+/{}", disk_entry.name));
                    }
                } else {
                    new_file_count += 1;
                    if samples.len() < 5 {
                        samples.push(format!("+{}", disk_entry.name));
                    }
                }
            }
            Some(db_entry) => {
                // Type change (dir <-> file)
                if db_entry.is_directory != disk_entry.is_dir {
                    if db_entry.is_directory {
                        let _ = writer.send(WriteMessage::DeleteSubtreeById(db_entry.id));
                    } else {
                        let _ = writer.send(WriteMessage::DeleteEntryById(db_entry.id));
                    }
                    let _ = writer.send(WriteMessage::UpsertEntryV2 {
                        parent_id,
                        name: disk_entry.name.clone(),
                        is_directory: disk_entry.is_dir,
                        is_symlink: disk_entry.is_symlink,
                        logical_size: disk_entry.logical_size,
                        physical_size: disk_entry.physical_size,
                        modified_at: disk_entry.modified_at,
                        inode: disk_entry.inode,
                        nlink: disk_entry.nlink,
                    });
                    // UpsertEntryV2 auto-propagates deltas in the writer.
                    if disk_entry.is_dir {
                        let new_dir = format!("{}/{}", parent_prefix, disk_entry.name);
                        new_dir_paths.push(new_dir);
                    }
                    stale_count += 1;
                    if !disk_entry.is_dir {
                        new_file_count += 1;
                    }
                    if samples.len() < 5 {
                        samples.push(format!("~{}", disk_entry.name));
                    }
                    continue;
                }

                // Modified file: compare size and mtime.
                // Skip size comparison when DB has NULL size for a hardlink (nlink > 1):
                // the NULL is intentional dedup, not a real mismatch.
                if !db_entry.is_directory {
                    let is_deduped_hardlink =
                        db_entry.logical_size.is_none() && matches!(disk_entry.nlink, Some(n) if n > 1);
                    let size_changed = !is_deduped_hardlink && db_entry.logical_size != disk_entry.logical_size;
                    let mtime_changed = db_entry.modified_at != disk_entry.modified_at;
                    if size_changed || mtime_changed {
                        let _ = writer.send(WriteMessage::UpsertEntryV2 {
                            parent_id,
                            name: disk_entry.name.clone(),
                            is_directory: false,
                            is_symlink: disk_entry.is_symlink,
                            logical_size: disk_entry.logical_size,
                            physical_size: disk_entry.physical_size,
                            modified_at: disk_entry.modified_at,
                            inode: disk_entry.inode,
                            nlink: disk_entry.nlink,
                        });
                        modified_count += 1;
                        if samples.len() < 5 {
                            samples.push(format!("~{}", disk_entry.name));
                        }
                    }
                }
            }
        }
    }

    let has_changes = stale_count > 0 || new_file_count > 0 || !new_dir_paths.is_empty() || modified_count > 0;
    if !has_changes {
        return Vec::new();
    }

    let total_diffs = stale_count + new_file_count + new_dir_paths.len() as u64 + modified_count;
    log::info!(
        "Verifier: {} diffs in `{}` ({} stale, {} new files, {} new dir, {} modified) [samples: {}]",
        total_diffs,
        normalized,
        stale_count,
        new_file_count,
        new_dir_paths.len(),
        modified_count,
        samples.join(", "),
    );

    // Scan new directories: flush first so UpsertEntryV2 entries are committed,
    // then scan_subtree can resolve paths to entry IDs.
    if !new_dir_paths.is_empty() {
        if let Err(e) = writer.flush().await {
            log::warn!("Verifier: pre-scan flush failed: {e}");
        }

        for new_dir in &new_dir_paths {
            if scanner::should_exclude(new_dir, space.exclusion_scope()) {
                continue;
            }
            match scanner::scan_subtree(Path::new(new_dir), space, writer, cancel) {
                Ok(summary) => {
                    log::debug!(
                        "Verifier: scanned new dir {} ({} entries, {}ms)",
                        new_dir,
                        summary.total_entries,
                        summary.duration_ms,
                    );
                }
                Err(e) => {
                    log::warn!("Verifier: scan_subtree({new_dir}) failed: {e}");
                }
            }
        }
        // No off-writer ancestor compensation: each `scan_subtree` sends
        // `ComputeSubtreeAggregates`, whose handler hands the ancestor chain (sizes,
        // counts, symlinks, AND coverage) to the writer's roll-up queue, drained at
        // its caught-up point. Doing it on the writer is race-free and can't
        // double-count; a read-then-`PropagateDeltaById` here would credit the same
        // bytes twice (Leak A). ⚠️ The flush below does NOT mean the roll-up landed
        // (`writer/pending_rollups.rs`); the writer emits its own refresh when it
        // does.
    }

    // Flush all corrections
    if let Err(e) = writer.flush().await {
        log::warn!("Verifier: final flush failed: {e}");
    }

    let mut paths = vec![normalized];
    paths.extend(new_dir_paths);
    paths
}

#[cfg(test)]
mod tests;
