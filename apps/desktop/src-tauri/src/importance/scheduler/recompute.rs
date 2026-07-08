//! The pure walk + scoring core of the importance scheduler: read a volume's
//! index once, assemble each folder's signals, run the scorer, and write the
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

use crate::importance::scorer::{SignalSet, Weights, score};
use crate::importance::signals::{ChildAggregate, OptionalSignals, signals_for_dir};
use crate::importance::store::importance_db_path;
use crate::importance::writer::{ImportanceWriter, WeightRow};
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};

// ── Recompute (full-volume) ───────────────────────────────────────────────

/// A folder discovered while walking the index, carrying everything the signal
/// assembler needs. Built by [`walk_index_folders`]. Holds its children's
/// pre-aggregated summary ([`ChildAggregate`]), NOT the child rows — the walk
/// folds each file into this so no file rows stay resident (the O(dirs) memory
/// shape).
pub(crate) struct IndexFolder {
    pub(crate) entry: EntryRow,
    pub(crate) path: String,
    pub(crate) children: ChildAggregate,
    pub(crate) has_marker_below: bool,
    /// `true` when a self-flooring ancestor (a denylisted, hidden, or system
    /// folder) sits above this one — so the whole subtree under a `node_modules`
    /// or a cache floors, not just the named folder. The downward twin of
    /// `has_marker_below`'s upward marker propagation.
    pub(crate) under_floored_ancestor: bool,
}

/// Walk every directory in a volume's index and build each folder's row,
/// reconstructed path, aggregated child summary, and marker-below flag —
/// materializing DIRECTORIES only, not the whole entries table.
///
/// The memory shape matters: on a multi-million-entry NAS the directories are a
/// small fraction of the rows, so this pulls only them into memory
/// ([`all_directories`](IndexStore::all_directories)) and STREAMS file rows
/// ([`for_each_file_child`](IndexStore::for_each_file_child)) into small per-parent
/// accumulators (extension set, file count, direct-marker flag), which are then
/// collapsed to a [`ChildAggregate`] per folder. So pass memory is O(dirs), not
/// O(entries) — the earlier `all_entries` walk went transiently into the hundreds
/// of MB on exactly the NAS-sized volumes SMB scoring now enables.
///
/// Directory children still come from the directory set itself (a `.git`/`.hg`
/// marker is a directory), so the direct-marker flag folds both the streamed file
/// children and the sibling directory children. Paths are reconstructed from the
/// in-memory `id → (parent_id, name)` directory map (no per-directory point
/// query). `has_marker_below` is a single upward propagation after the walk, so a
/// `.git` deep in a tree raises its ancestors (plan Decision 3).
pub(crate) fn walk_index_folders(conn: &rusqlite::Connection, home: &str) -> Result<Vec<IndexFolder>, String> {
    let dirs = IndexStore::all_directories(conn).map_err(|e| e.to_string())?;

    // Index the directory rows: a lookup for path reconstruction, keyed by id.
    let by_id: HashMap<i64, &EntryRow> = dirs.iter().map(|e| (e.id, e)).collect();

    // Per-directory accumulator, folded from the streamed file children plus the
    // sibling directory children. Kept tiny (a small extension set + two scalars)
    // so the map is O(dirs), never O(files).
    #[derive(Default)]
    struct Accum {
        extensions: std::collections::HashSet<String>,
        file_count: u32,
        has_direct_marker: bool,
    }
    let mut accum: HashMap<i64, Accum> = HashMap::new();

    // Directory children first: a `.git`/`.hg`/`.svn` marker is a DIRECTORY, so
    // fold the directory set into each parent's direct-marker flag. (Directories
    // never contribute to the extension count or file count.)
    for d in dirs.iter().filter(|e| e.id != ROOT_ID) {
        if crate::importance::classify::is_project_marker(&d.name.to_lowercase()) {
            accum.entry(d.parent_id).or_default().has_direct_marker = true;
        }
    }

    // File children streamed one row at a time: fold each into its parent's
    // extension set, file count, and (for a `Cargo.toml`/`package.json`/… file)
    // marker flag. The file rows are never all resident.
    IndexStore::for_each_file_child(conn, |parent_id, name| {
        let entry = accum.entry(parent_id).or_default();
        entry.file_count += 1;
        let ext = std::path::Path::new(name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        entry.extensions.insert(ext);
        if crate::importance::classify::is_project_marker(&name.to_lowercase()) {
            entry.has_direct_marker = true;
        }
    })
    .map_err(|e| e.to_string())?;

    // One folder per directory entry (the root sentinel isn't a real folder).
    let mut folders: Vec<IndexFolder> = Vec::new();
    let mut dir_id_to_index: HashMap<i64, usize> = HashMap::new();
    for entry in dirs.iter().filter(|e| e.id != ROOT_ID) {
        let path = reconstruct_path_from_map(entry.id, &by_id);
        let a = accum.remove(&entry.id).unwrap_or_default();
        dir_id_to_index.insert(entry.id, folders.len());
        folders.push(IndexFolder {
            entry: entry.clone(),
            path,
            children: ChildAggregate {
                distinct_extension_count: a.extensions.len() as u32,
                file_count: a.file_count,
                has_direct_marker: a.has_direct_marker,
            },
            has_marker_below: false,
            under_floored_ancestor: false,
        });
    }

    // Propagate the floor DOWN to descendants: a folder under a self-flooring
    // ancestor (a denylisted / hidden / system folder) floors too, so a
    // `node_modules`'s whole subtree floors, not just the folder named
    // `node_modules` (the descendant-floor fix). The downward twin of the
    // marker-below upward propagation above.
    //
    // Seed the self-flooring folders (classified from each folder's own path via
    // the shared `classify` predicate), then mark every DESCENDANT of a seed by
    // walking each folder's parent chain and checking whether any ancestor is a
    // seed. Uses the same `id → parent_id` directory map path reconstruction uses,
    // so it's robust to the entries map not being parent-before-child sorted.
    let self_floored: std::collections::HashSet<i64> = folders
        .iter()
        .filter(|f| crate::importance::classify::self_floors(&f.path, &f.entry.name, home))
        .map(|f| f.entry.id)
        .collect();
    for folder in &mut folders {
        let mut cursor = by_id.get(&folder.entry.id).map(|e| e.parent_id);
        while let Some(pid) = cursor {
            if pid == ROOT_ID {
                break;
            }
            if self_floored.contains(&pid) {
                folder.under_floored_ancestor = true;
                break;
            }
            cursor = by_id.get(&pid).map(|e| e.parent_id);
        }
    }

    // Propagate a direct project marker up to every ancestor: a `.git` deep in a
    // subtree marks the whole path above it as project-adjacent (plan Decision 3).
    // Seed from each folder's own direct-marker flag, then walk parent pointers.
    let marker_seed: Vec<i64> = folders
        .iter()
        .filter(|f| f.children.has_direct_marker)
        .map(|f| f.entry.id)
        .collect();
    for seed in marker_seed {
        let mut cursor = by_id.get(&seed).map(|e| e.parent_id);
        while let Some(pid) = cursor {
            if pid == ROOT_ID {
                break;
            }
            if let Some(&idx) = dir_id_to_index.get(&pid) {
                folders[idx].has_marker_below = true;
            }
            cursor = by_id.get(&pid).map(|e| e.parent_id);
        }
    }

    Ok(folders)
}

/// Reconstruct an entry's absolute path from an in-memory `id → row` map, walking
/// parent pointers up to the root sentinel. The in-memory twin of the store's
/// `reconstruct_path` point query — used because a full recompute reconstructs
/// every folder's path and the per-query cost would be O(dirs × depth). The map
/// holds only directory rows, which is all a path walk (dir → dir → …) needs.
fn reconstruct_path_from_map(id: i64, by_id: &HashMap<i64, &EntryRow>) -> String {
    let mut components: Vec<&str> = Vec::new();
    let mut cursor = Some(id);
    while let Some(cid) = cursor {
        if cid == ROOT_ID {
            break;
        }
        let Some(entry) = by_id.get(&cid) else { break };
        components.push(&entry.name);
        cursor = Some(entry.parent_id);
    }
    components.reverse();
    format!("/{}", components.join("/"))
}

/// Score every folder in `folders` and return the weight rows to persist.
///
/// Pure over the walked folders + the optional-signal lookups: given a function
/// that resolves a folder's visit count and last-used timestamp (from
/// `importance.db` + Spotlight sampling), it assembles each `FolderSignals`, runs
/// the scorer, and produces a `WeightRow`. Split out so a test can drive it with
/// synthetic folders and no index.
pub(super) fn score_folders(
    folders: &[IndexFolder],
    home: &str,
    weights: &Weights,
    available: &SignalSet,
    now_secs: u64,
    mut optional_for: impl FnMut(&str) -> OptionalSignals,
) -> Vec<WeightRow> {
    folders
        .iter()
        .map(|f| {
            let optional = optional_for(&f.path);
            let signals = signals_for_dir(
                &f.entry,
                f.children,
                &f.path,
                home,
                f.has_marker_below,
                f.under_floored_ancestor,
                optional,
            );
            let s = score(&signals, available, weights, now_secs);
            let signals_json = serde_json::to_string(&signals).unwrap_or_else(|_| "{}".to_string());
            WeightRow {
                path: f.path.clone(),
                score: s.value(),
                signals_json,
            }
        })
        .collect()
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
    folders: &[IndexFolder],
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

    Ok(RecomputeOutcome { count, generation })
}

/// The result of a recompute pass: how many folders were scored and the
/// generation the pass wrote at (the as-of marker consumers see; the recompute
/// subscription fires with it).
pub(super) struct RecomputeOutcome {
    pub(super) count: usize,
    pub(super) generation: u64,
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

/// The inputs to an incremental rescore, bundled like [`RecomputeInputs`].
pub(super) struct IncrementalInputs<'a> {
    pub(super) writer: &'a ImportanceWriter,
    pub(super) weights: &'a Weights,
    pub(super) home: &'a str,
    pub(super) now_secs: u64,
    pub(super) available: SignalSet,
    pub(super) visits: &'a HashMap<String, u32>,
}

/// Rescore only the folders whose paths are in the touched set (changed paths +
/// their capped ancestors) and upsert them WITHOUT advancing the generation, so
/// every untouched folder keeps its as-of marker (plan Decision 5). Returns the
/// number of folders rescored.
///
/// Split from the pool/registry resolution so a test drives it with a synthetic
/// walk and a directly-built writer (no registry, no FFI). Samples
/// `kMDItemLastUsedDate` only for the touched subset (bounded work).
pub(super) fn incremental_rescore(
    inputs: &IncrementalInputs<'_>,
    folders: &[IndexFolder],
    changed_paths: &[String],
) -> Result<usize, String> {
    // The set of folders to rescore: each changed path plus its ancestors up to
    // the cap. Bounding the ancestor walk keeps a deep project marker from
    // rescoping half the volume (plan open-question / Decision 5).
    let touched = touched_folder_set(changed_paths);
    let subset: Vec<&IndexFolder> = folders.iter().filter(|f| touched.contains(&f.path)).collect();
    if subset.is_empty() {
        return Ok(0);
    }

    // Sample Spotlight only when the kind's mask allows it (SMB has none, and
    // sampling would touch the mount). When unavailable the map is empty and the
    // `last_used` weight redistributes.
    let last_used = if inputs.available.last_used_available {
        let subset_paths: Vec<String> = subset.iter().map(|f| f.path.clone()).collect();
        crate::importance::last_used::sample_last_used(&subset_paths)
    } else {
        HashMap::new()
    };

    let writer = inputs.writer;
    // The incremental rows carry the CURRENT generation (no bump), so they're
    // as-fresh-as the last full pass and untouched folders don't turn stale.
    let generation = writer.next_generation().map_err(|e| e.to_string())?.saturating_sub(1);

    let rows: Vec<WeightRow> = subset
        .iter()
        .map(|f| {
            let optional = OptionalSignals {
                visit_count: inputs.visits.get(&f.path).copied(),
                last_used_secs: last_used.get(&f.path).copied(),
            };
            let signals = signals_for_dir(
                &f.entry,
                f.children,
                &f.path,
                inputs.home,
                f.has_marker_below,
                f.under_floored_ancestor,
                optional,
            );
            let s = score(&signals, &inputs.available, inputs.weights, inputs.now_secs);
            let signals_json = serde_json::to_string(&signals).unwrap_or_else(|_| "{}".to_string());
            WeightRow {
                path: f.path.clone(),
                score: s.value(),
                signals_json,
            }
        })
        .collect();
    let count = rows.len();

    writer
        .write_weights_incremental(generation, rows)
        .map_err(|e| e.to_string())?;
    writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(count)
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
