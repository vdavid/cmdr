//! The scoring core of the importance scheduler: take a volume's walk
//! ([`super::walk`]), assemble each folder's signals, run the scorer, and write the
//! rows. Split out of [`super`] (the scheduler handle + bus wiring) so the
//! I/O-shaped-but-registry-free logic is a self-contained, directly-testable unit
//! — a test drives these with a synthetic walk and a directly-built writer, no
//! registry, no async driver, no FFI.
//!
//! Nothing here touches the lifecycle bus, Tauri, or the coalescing coordinator;
//! it reads the index read pool's connection and writes through an
//! [`ImportanceWriter`]. The scheduler's `run_pass_blocking` /
//! `run_incremental_blocking` methods resolve the pool + writer and call in here.

use std::collections::HashMap;

use super::walk::{IndexFolder, WalkedFolders, walk_index_folders};
use crate::importance::scorer::{SignalSet, Weights, explain};
use crate::importance::signals::{OptionalSignals, signals_for_dir};
use crate::importance::store::importance_db_path;
use crate::importance::writer::{ImportanceWriter, WeightRow};
#[cfg(any(test, feature = "tooling"))]
use crate::indexing::store::IndexStore;

// ── Recompute (full-volume) ───────────────────────────────────────────────

/// Score every folder in `folders` and return the weight rows to persist —
/// OMITTING floored folders, which get no row at all (the storage-compaction
/// discipline; see the store's storage model).
///
/// Pure over the walked folders + the optional-signal lookups: given a function
/// that resolves a folder's visit count and last-used timestamp (from
/// `importance.db` + Spotlight sampling), it assembles each `FolderSignals`, runs
/// the scorer, and produces a `WeightRow` for every folder whose score is NOT
/// floored. A floored folder (denylisted / hidden / under a floored ancestor) is
/// derivable from its path alone at read time, so persisting its full signal blob
/// would only bloat the store — on a dev home ~76% of folders floor. Split out so a
/// test can drive it with synthetic folders and no index.
pub(super) fn score_folders(
    folders: &mut WalkedFolders,
    home: &str,
    weights: &Weights,
    available: &SignalSet,
    now_secs: u64,
    mut optional_for: impl FnMut(&str) -> OptionalSignals,
) -> Vec<WeightRow> {
    let mut rows = Vec::new();
    folders.for_each(|f, path| {
        let optional = optional_for(path);
        let signals = signals_for_dir(
            f.modified_at,
            f.children,
            path,
            home,
            f.has_marker_below,
            f.under_floored_ancestor,
            optional,
        );
        let explanation = explain(&signals, available, weights, now_secs);
        // A floored folder gets no row: its floored-ness is re-derivable from the
        // path at read time (`WeightLookup::Floored`), so the signal blob is waste.
        if explanation.floored {
            return;
        }
        let signals_json = serde_json::to_string(&signals).unwrap_or_else(|_| "{}".to_string());
        rows.push(WeightRow {
            path: path.to_string(),
            score: explanation.score.value(),
            signals_json,
        });
    });
    rows
}

/// The inputs to a full-volume recompute pass, bundled so the pass signature
/// stays readable (and under clippy's argument cap). Borrowed for the pass's
/// lifetime; nothing is retained.
pub(super) struct RecomputeInputs<'a> {
    /// The shared long-lived writer for this volume's `importance.db` (one writer
    /// thread per DB). Reads the current generation and writes the pass through it.
    pub(super) writer: &'a ImportanceWriter,
    pub(super) weights: &'a Weights,
    pub(super) home: &'a str,
    pub(super) now_secs: u64,
    /// The signal-availability mask for the volume kind: `SignalSet::all()` for a
    /// local macOS volume (both optional signals producible), `listing_only()`
    /// where Spotlight is absent.
    pub(super) available: SignalSet,
    /// Per-folder navigation-visit counts (from `importance.db`).
    pub(super) visits: &'a HashMap<String, u32>,
    /// Per-folder sampled `kMDItemLastUsedDate` seconds (macOS-local).
    pub(super) last_used: &'a HashMap<String, u64>,
}

/// Run a full-volume recompute over the already-walked `folders`, writing to
/// `data_dir`'s `importance-{volume_id}.db`. Returns the number of folders scored.
///
/// Takes the walked folders (not the pool) so the caller walks the index ONCE and
/// reuses that walk for both the `kMDItemLastUsedDate` path-set and the score —
/// no second traversal. Split from the volume-id resolution so a test drives it
/// with a synthetic walk (no registry, no FFI). Weights are stamped at a
/// freshly-bumped generation so every row carries the pass's as-of marker (plan
/// Decision 2/5).
pub(super) fn recompute_folders(
    inputs: &RecomputeInputs<'_>,
    folders: &mut WalkedFolders,
) -> Result<RecomputeOutcome, String> {
    if folders.is_empty() {
        return Ok(RecomputeOutcome {
            count: 0,
            generation: 0,
        });
    }

    let rows = score_folders(
        folders,
        inputs.home,
        inputs.weights,
        &inputs.available,
        inputs.now_secs,
        |path| OptionalSignals {
            visit_count: inputs.visits.get(path).copied(),
            last_used_secs: inputs.last_used.get(path).copied(),
        },
    );
    let count = rows.len();

    let writer = inputs.writer;
    let generation = writer.next_generation().map_err(|e| e.to_string())?;
    writer.write_weights(generation, rows).map_err(|e| e.to_string())?;
    writer.flush_blocking().map_err(|e| e.to_string())?;
    // A full pass REPLACES the whole `weights` table, so the WAL just grew to ~DB size.
    // Truncate it now that the pass is committed (a quiet point). Best-effort: it never
    // fails the recompute (plan M9).
    let _ = writer.checkpoint_wal();

    Ok(RecomputeOutcome { count, generation })
}

/// The result of a recompute pass: how many folders were scored and the
/// generation the pass wrote at (the as-of marker consumers see; the recompute
/// subscription fires with it).
pub(super) struct RecomputeOutcome {
    pub(super) count: usize,
    pub(super) generation: u64,
}

/// The result of a measurement recompute: rows written, the phase wall-clock
/// split (walk-and-score vs write-and-flush), and the memory the pass cost, for
/// the `importance-measure` dev bin.
#[cfg(any(test, feature = "tooling"))]
pub struct MeasureOutcome {
    /// Weight rows written (floored folders omitted).
    pub rows_written: usize,
    /// Total folders the walk produced (rows + floored-omitted).
    pub folders_walked: usize,
    /// Reading the index and scoring every folder (the walk + `score_folders`).
    pub walk_and_score: std::time::Duration,
    /// Writing the rows through the writer and flushing to disk.
    pub write_and_flush: std::time::Duration,
    /// What the WALK alone added to the process's `phys_footprint`: the reading
    /// taken right after `walk_index_folders` returns, minus the one taken before
    /// it. The number to watch — the walk's output stays resident through scoring,
    /// so it sets the pass's floor.
    pub walk_footprint_bytes: Option<u64>,
    /// What the WHOLE pass added to the process's `phys_footprint` (the walk's
    /// output plus the scored rows and the writer), read at the end.
    pub pass_footprint_bytes: Option<u64>,
}

/// Walk a real `index-{volume_id}.db` READ-ONLY, score every folder, and write the
/// weights into `importance_db` through a fresh writer — the whole full-pass core
/// without the registry, read-pool registry, or async driver.
///
/// The measurement/tuning entry point: a dev tool points it at a real index and a
/// scratch `importance.db` to see how many rows a pass writes, how large the store
/// is, and the phase wall-clock split, exercising the SAME walk + score + write path
/// a live recompute uses (so the floored-skip and trimmed-JSON shape are measured
/// faithfully). Spotlight is never sampled here (the tool has no live volume;
/// `last_used` redistributes per `available`), and visits come from the target
/// `importance.db` if it already holds any.
#[cfg(any(test, feature = "tooling"))]
pub fn recompute_index_to_db(
    index_db: &std::path::Path,
    importance_db: &std::path::Path,
    home: &str,
    available: SignalSet,
    now_secs: u64,
) -> Result<MeasureOutcome, String> {
    // Walk + score (the read/compute phase).
    let footprint_before = cmdr_fs::process_memory::current_phys_footprint();
    let walk_started = std::time::Instant::now();
    let conn = IndexStore::open_read_connection(index_db).map_err(|e| e.to_string())?;
    let mut folders = walk_index_folders(&conn, home)?;
    // Read the footprint while the walk's output is still the only thing resident,
    // so the number is the walk's cost rather than the whole pass's.
    let walk_footprint_bytes = footprint_growth(footprint_before);
    if folders.is_empty() {
        return Ok(MeasureOutcome {
            rows_written: 0,
            folders_walked: 0,
            walk_and_score: walk_started.elapsed(),
            write_and_flush: std::time::Duration::ZERO,
            walk_footprint_bytes,
            pass_footprint_bytes: walk_footprint_bytes,
        });
    }
    let folders_walked = folders.len();
    let rows = score_folders(&mut folders, home, &Weights::default(), &available, now_secs, |_| {
        OptionalSignals::default()
    });
    let walk_and_score = walk_started.elapsed();

    // Write + flush (the write phase).
    let write_started = std::time::Instant::now();
    let writer = ImportanceWriter::spawn(importance_db).map_err(|e| e.to_string())?;
    let generation = writer.next_generation().map_err(|e| e.to_string())?;
    let rows_written = rows.len();
    writer.write_weights(generation, rows).map_err(|e| e.to_string())?;
    writer.flush_blocking().map_err(|e| e.to_string())?;
    writer.shutdown();
    let write_and_flush = write_started.elapsed();

    Ok(MeasureOutcome {
        rows_written,
        folders_walked,
        walk_and_score,
        write_and_flush,
        walk_footprint_bytes,
        pass_footprint_bytes: footprint_growth(footprint_before),
    })
}

/// How much the process's `phys_footprint` has grown since `before`, or `None` when
/// either reading failed (or the platform has no Mach `task_info`).
#[cfg(any(test, feature = "tooling"))]
fn footprint_growth(before: Option<u64>) -> Option<u64> {
    match (cmdr_fs::process_memory::current_phys_footprint(), before) {
        (Some(now), Some(before)) => Some(now.saturating_sub(before)),
        _ => None,
    }
}

/// Read the visit table into a path→count map for the recompute pass. A missing
/// or unopenable DB yields an empty map (the visit signal is absent, not an
/// error).
pub(super) fn load_visits(data_dir: &std::path::Path, volume_id: &str) -> HashMap<String, u32> {
    let db_path = importance_db_path(data_dir, volume_id);
    let mut out = HashMap::new();
    if let Ok(conn) = crate::importance::store::open_read_connection(&db_path)
        && let Ok(mut stmt) = conn.prepare("SELECT path, visit_count FROM visits")
        && let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32)))
    {
        for row in rows.flatten() {
            out.insert(row.0, row.1);
        }
    }
    out
}

// ── Incremental rescore ────────────────────────────────────────────────────

/// Which folders an incremental pass writes rows for.
///
/// Set by which walk produced the folders, so the rescore never reaches for a folder
/// the walk didn't read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RescoreScope {
    /// Only the folders inside the changed subtrees — what a scoped walk holds.
    ChangedSubtreesOnly,
    /// Those PLUS each origin's capped ancestor chain, which only a whole-volume
    /// walk can score correctly (an ancestor's `has_marker_below` depends on
    /// everything below it, not only on the changed subtree).
    WithAncestors,
}

/// Read the folders one incremental pass needs out of `conn`, and say which of them
/// the pass may write.
///
/// The single seam between "how the index is read" and "what gets scored and
/// written": everything downstream ([`incremental_rescore`]) is a pure function of
/// the [`WalkedFolders`] this returns. It prefers the SCOPED walk
/// ([`super::scoped_walk`]), which reads only the changed subtrees, and falls back to
/// the full O(dirs) [`walk_index_folders`] whenever the scoped one can't stand in for
/// it — which is also what keeps the full walk exercised as the fallback path.
pub(super) fn walk_for_incremental(
    conn: &rusqlite::Connection,
    home: &str,
    origins: &[String],
    previous_markers: &HashMap<String, bool>,
) -> Result<(WalkedFolders, RescoreScope), String> {
    match super::scoped_walk::try_scoped_walk(conn, home, origins, previous_markers)? {
        super::scoped_walk::ScopedWalkOutcome::Scoped(folders) => Ok((folders, RescoreScope::ChangedSubtreesOnly)),
        super::scoped_walk::ScopedWalkOutcome::FullWalkNeeded(reason) => {
            log::debug!(target: "importance", "incremental rescore takes the full walk: {reason}");
            Ok((walk_index_folders(conn, home)?, RescoreScope::WithAncestors))
        }
    }
}

/// Drop every origin that sits under another origin in the same batch.
///
/// `subtree(P/x) ⊆ subtree(P)`, so a nested origin adds no folder and clears no extra
/// row — it only makes the pass read the same rows twice. Dropping it BEFORE the walk
/// keeps clear and insert on one slice (both take the de-duplicated list), and it is
/// what usually spares a brand-new folder from the marker-comparison fallback: the
/// folder's creation changed its PARENT's listing, so the parent is an origin too,
/// and the parent has a stored row where the new child doesn't.
pub(super) fn dedupe_nested_origins(origins: &[String]) -> Vec<String> {
    let mut sorted: Vec<&String> = origins.iter().collect();
    // Shortest first, so an ancestor is always considered before its descendants.
    sorted.sort_unstable_by_key(|p| p.len());
    let mut kept: Vec<String> = Vec::new();
    for origin in sorted {
        if !is_in_changed_subtree(origin, &kept) {
            kept.push(origin.clone());
        }
    }
    // Back to the batch's own order, so the clear list stays stable across passes.
    let mut out: Vec<String> = origins.iter().filter(|p| kept.contains(p)).cloned().collect();
    out.dedup();
    out
}

/// Read each origin's stored `has_project_marker` out of this volume's
/// `importance.db`: the "before" side of the scoped walk's guard. An origin missing
/// from the result has no stored row.
pub(super) fn load_previous_markers(
    data_dir: &std::path::Path,
    volume_id: &str,
    origins: &[String],
) -> HashMap<String, bool> {
    let db_path = importance_db_path(data_dir, volume_id);
    let mut out = HashMap::new();
    let Ok(conn) = crate::importance::store::open_read_connection(&db_path) else {
        return out;
    };
    for origin in origins {
        let Ok(Some(stored)) = crate::importance::store::read_weight(&conn, origin) else {
            continue;
        };
        let Ok(signals) = serde_json::from_str::<crate::importance::scorer::FolderSignals>(&stored.signals_json) else {
            continue;
        };
        out.insert(origin.clone(), signals.has_project_marker);
    }
    out
}

/// Everything scoring one folder needs, with no writer attached — so the
/// differential harness can build rows off two walks without a store behind it.
pub(super) struct ScoringInputs<'a> {
    pub(super) weights: &'a Weights,
    pub(super) home: &'a str,
    pub(super) now_secs: u64,
    pub(super) available: SignalSet,
    pub(super) visits: &'a HashMap<String, u32>,
}

/// The inputs to an incremental rescore, bundled like [`RecomputeInputs`].
pub(super) struct IncrementalInputs<'a> {
    pub(super) writer: &'a ImportanceWriter,
    pub(super) weights: &'a Weights,
    pub(super) home: &'a str,
    pub(super) now_secs: u64,
    pub(super) available: SignalSet,
    pub(super) visits: &'a HashMap<String, u32>,
}

impl<'a> IncrementalInputs<'a> {
    /// The scoring half of these inputs, without the writer.
    fn scoring(&self) -> ScoringInputs<'a> {
        ScoringInputs {
            weights: self.weights,
            home: self.home,
            now_secs: self.now_secs,
            available: self.available,
            visits: self.visits,
        }
    }
}

/// Rescore the changed subtrees WITHOUT advancing the generation, so every
/// untouched folder keeps its as-of marker (plan Decision 5). Returns the number of
/// (non-floored) folders written.
///
/// The touched set is each changed path's capped ancestor chain (upward: a marker
/// or size change can raise parents) UNION each changed path's whole descendant
/// subtree (downward: a folder that became — or stopped being — floored flips its
/// entire subtree's floor status). The write CLEARS each changed subtree first,
/// then inserts only the NON-FLOORED folders in the touched set. Clearing before
/// inserting is what makes transitions correct in ONE model:
///
/// - a folder renamed away or deleted: its old-path row is cleared and never
///   re-inserted (it's not in the current walk);
/// - a folder that BECAME floored (and its now-under-floored descendants): cleared,
///   and skipped on re-insert because it floors;
/// - a folder that STOPPED being floored: cleared (it had no row anyway), then
///   inserted because it now scores.
///
/// Split from the pool/registry resolution so a test drives it with a synthetic
/// walk and a directly-built writer (no registry, no FFI). Samples
/// `kMDItemLastUsedDate` only for the touched subset (bounded work).
pub(super) fn incremental_rescore(
    inputs: &IncrementalInputs<'_>,
    folders: &mut WalkedFolders,
    changed_paths: &[String],
    scope: RescoreScope,
) -> Result<usize, String> {
    // The set of folders to (re)insert: every walked folder in a changed path's
    // subtree (downward floor propagation), plus — on a full walk only — each
    // changed path's capped ancestor chain (upward marker propagation). The
    // ancestor cap bounds the upward walk; the downward side is bounded by the
    // subtree that actually changed. Only THIS subset materializes a path, so the
    // memory stays proportional to what changed.
    let subset = rescore_subset(folders, changed_paths, scope);
    if subset.is_empty() && changed_paths.is_empty() {
        return Ok(0);
    }

    // Sample Spotlight only when the kind's mask allows it (SMB has none, and
    // sampling would touch the mount). When unavailable the map is empty and the
    // `last_used` weight redistributes. Hand over only as many paths as the sample
    // can use — it queries the first `SAMPLE_CAP` and drops the rest.
    let last_used = if inputs.available.last_used_available {
        let subset_paths: Vec<String> = subset
            .iter()
            .take(crate::importance::last_used::SAMPLE_CAP)
            .map(|(_, path)| path.clone())
            .collect();
        crate::importance::last_used::sample_last_used(&subset_paths)
    } else {
        HashMap::new()
    };

    let rows = rescore_rows(&inputs.scoring(), &subset, &last_used);
    let count = rows.len();

    let writer = inputs.writer;
    // The incremental rows carry the CURRENT generation (no bump), so they're
    // as-fresh-as the last full pass and untouched folders don't turn stale. The
    // changed subtrees are cleared first so renamed-away / deleted / now-floored
    // folders leave no orphan row.
    let generation = writer.next_generation().map_err(|e| e.to_string())?.saturating_sub(1);

    writer
        .write_weights_incremental(generation, rows, changed_paths.to_vec())
        .map_err(|e| e.to_string())?;
    writer.flush_blocking().map_err(|e| e.to_string())?;
    // The every-60s incremental is the WAL churn source (plan M9): truncate at this
    // quiet point so the file doesn't creep up in place. Best-effort, never fails.
    let _ = writer.checkpoint_wal();
    Ok(count)
}

/// The folders an incremental pass writes rows for, each with its materialized path.
///
/// Only THIS subset materializes a path, so the memory stays proportional to what
/// changed. Split out of [`incremental_rescore`] so the differential harness can
/// build the same subset off two different walks without writing anything.
pub(super) fn rescore_subset(
    folders: &mut WalkedFolders,
    changed_paths: &[String],
    scope: RescoreScope,
) -> Vec<(IndexFolder, String)> {
    let touched = match scope {
        RescoreScope::WithAncestors => touched_folder_set(changed_paths),
        // A scoped walk never read the ancestors, and doesn't need to: the only
        // signal that can move for them is `has_project_marker`, and the scoped
        // walk falls back to the full one whenever that could have happened.
        RescoreScope::ChangedSubtreesOnly => std::collections::HashSet::new(),
    };
    let mut subset = Vec::new();
    folders.for_each(|f, path| {
        if touched.contains(path) || is_in_changed_subtree(path, changed_paths) {
            subset.push((*f, path.to_string()));
        }
    });
    subset
}

/// Assemble each subset folder's signals and score it; only the NON-FLOORED ones get
/// a row (floored folders are cleared by the subtree delete and never re-inserted).
pub(super) fn rescore_rows(
    inputs: &ScoringInputs<'_>,
    subset: &[(IndexFolder, String)],
    last_used: &HashMap<String, u64>,
) -> Vec<WeightRow> {
    subset
        .iter()
        .filter_map(|(f, path)| {
            let optional = OptionalSignals {
                visit_count: inputs.visits.get(path).copied(),
                last_used_secs: last_used.get(path).copied(),
            };
            let signals = signals_for_dir(
                f.modified_at,
                f.children,
                path,
                inputs.home,
                f.has_marker_below,
                f.under_floored_ancestor,
                optional,
            );
            let explanation = explain(&signals, &inputs.available, inputs.weights, inputs.now_secs);
            if explanation.floored {
                return None;
            }
            let signals_json = serde_json::to_string(&signals).unwrap_or_else(|_| "{}".to_string());
            Some(WeightRow {
                path: path.clone(),
                score: explanation.score.value(),
                signals_json,
            })
        })
        .collect()
}

/// Whether `path` sits at or under any of `changed_paths` — the downward subtree
/// expansion for incremental rescoring. A folder whose ancestor's listing changed
/// may have flipped floored-ness (a `node_modules` renamed, or a plain folder
/// renamed to one), so its whole subtree must be revisited. Pure path math.
///
/// **Allocation-free, deliberately.** This runs once per walked folder per changed
/// path inside [`incremental_rescore`]'s loop over the WHOLE volume, every 60 s —
/// ~161 k folders per pass on a dev machine. Building the `{changed}/` needle with
/// `format!` here was a top allocation site (`docs/notes/memory-runaway-rust-heap-2026-07-25.md`);
/// ❌ don't reintroduce one. The `rest` must be empty or start with `/`: a bare
/// `starts_with(changed)` wrongly matches `/a/bc` against changed `/a/b`.
pub(super) fn is_in_changed_subtree(path: &str, changed_paths: &[String]) -> bool {
    changed_paths.iter().any(|changed| {
        path.strip_prefix(changed.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// The maximum number of ancestor levels an incremental rescore walks up from a
/// changed folder. A project marker (or a size/mtime change) can raise ancestors,
/// but a deep change must not rescope half the volume, so the walk is capped
/// (plan open-question / Decision 5). Generous enough for realistic home trees.
pub(super) const ANCESTOR_WALK_CAP: usize = 32;

/// Build the set of folder paths an incremental rescore should touch: each changed
/// path plus its ancestors, up to [`ANCESTOR_WALK_CAP`] levels each. Pure string
/// math over absolute paths, so it's unit-testable without an index.
pub(super) fn touched_folder_set(changed_paths: &[String]) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for path in changed_paths {
        set.insert(path.clone());
        let mut current = path.as_str();
        for _ in 0..ANCESTOR_WALK_CAP {
            let Some(pos) = current.rfind('/') else { break };
            if pos == 0 {
                break; // reached the root `/`; don't add the bare root as a folder.
            }
            let parent = &current[..pos];
            set.insert(parent.to_string());
            current = parent;
        }
    }
    set
}

/// Drop the paths that must never drive an incremental rescore from a live
/// dir-changed batch: the bare root `/`, empty strings, and anything FLOORED by
/// path. An empty result means the batch has nothing to rescore, and the caller
/// returns before the O(dirs) walk.
///
/// **Why the floor filter is the idle gate.** A boot volume is never silent:
/// builds write under `target/`, `.git`, and `node_modules`, browsers and toolchains
/// write under `Library/Caches`, and agents write under dot-directories. Every one of
/// those floors by path, and a floored folder gets NO row — so a batch of only such
/// paths would walk the whole index (O(dirs), seconds of CPU), rescope to a set that
/// scores nothing, write zero rows, and clear subtrees that hold none. Skipping it is
/// exactly a no-op, and it's what makes a quiet machine actually quiet: without the
/// filter, background churn drove a full walk plus a full weight reload every 60 s for
/// the whole session (prod v0.36.2, measured 2026-07-28).
///
/// `classify::floors_by_path` is the SAME predicate the walk applies when it decides
/// to skip a row (`../CLAUDE.md` § the floor overrides), so the gate and the writer
/// can't disagree about what scores. It's pure path math — no index, no syscall — and
/// typed set-membership, never a message or substring branch.
///
/// **Decision/Why the accepted lossiness.** Filtering a floored path also drops the
/// UNFLOORED ancestors `touched_folder_set` would have pulled in from it. The only
/// signal that can move for such an ancestor is `has_marker_below` (a project marker
/// appearing or vanishing inside a floored subtree — a `.git` under a `node_modules`,
/// say), because a deep write changes neither an ancestor's mtime nor its direct-child
/// aggregate. We accept that: a marker buried in machine output is the weakest possible
/// reason to raise a folder, importance is advisory derived data, and the next full
/// pass heals it. ❌ Don't "fix" it by dropping the filter — that reinstates the
/// treadmill.
///
/// **Why the bare root matters (the "everything changed" trap):** every live
/// FSEvent's affected-paths set carries the full ancestor chain up to `/`
/// (`reconciler::collect_ancestor_paths`, so the frontend can refresh every
/// ancestor's displayed size). So `/` is present in essentially *every* batch —
/// it's the universal ancestor, NOT a signal that the whole volume changed. Left
/// in, it would (a) get treated as a full-refresh sentinel and escalate every
/// incremental to a whole-volume rewrite, and (b) reach `write_weights_incremental`'s
/// subtree-clear. Full recomputes are driven by `ScanCompleted`, never by a live
/// batch, so the incremental path drops `/` and scores only real folders.
pub(super) fn sanitize_incremental_batch(changed_paths: &[String], home: &str) -> Vec<String> {
    changed_paths
        .iter()
        .filter(|p| !p.is_empty() && p.as_str() != "/")
        .filter(|p| !crate::importance::classify::floors_by_path(p, home))
        .cloned()
        .collect()
}
