//! The registry-free walk + enrich + GC core of the media scheduler: read a
//! volume's index once, qualify its images, run the backend over the stale ones,
//! and GC rows whose source files vanished. Split out of [`super`] (the coordinator
//! and bus wiring) so this I/O-shaped-but-registry-free logic is directly testable:
//! a test drives it with a synthetic index, a real [`MediaWriter`], and the fake
//! backend, with no registry, no async driver, and no FFI (mirroring `importance`'s
//! `recompute.rs`).

use std::collections::{HashMap, HashSet};

use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::media_index::backend::{Analysis, ImageInput, VisionBackend};
use crate::media_index::predicate::{MediaKind, Qualification, qualify_dir};
use crate::media_index::store::{EnrichmentState, MediaStatusRow, needs_enrichment};
use crate::media_index::writer::{MediaWriter, UpsertAnalysis};

/// One qualifying image discovered while walking the index: its absolute path, the
/// `(mtime, size)` staleness key, and the typed kind the predicate assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageEntry {
    pub(crate) path: String,
    pub(crate) mtime: Option<u64>,
    pub(crate) size: Option<u64>,
    pub(crate) kind: MediaKind,
}

/// What one pass did: how many images it enriched and how many rows it GC'd.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct PassSummary {
    pub(crate) enriched: usize,
    pub(crate) gc_count: usize,
}

/// Walk every directory in a volume's index, qualify each directory's files
/// (sibling-aware, via [`qualify_dir`]), and return the qualifying image entries
/// with their `(mtime, size)` and kind.
///
/// Directories are materialized once into an `id → row` map for path
/// reconstruction (as `importance`'s walk does); files are read carrying their
/// `modified_at` + `logical_size` (the staleness key) and grouped per parent so the
/// sibling-aware predicate can run per directory.
pub(crate) fn walk_image_entries(conn: &rusqlite::Connection) -> Result<Vec<ImageEntry>, String> {
    let dirs = IndexStore::all_directories(conn).map_err(|e| e.to_string())?;
    let by_id: HashMap<i64, &crate::indexing::store::EntryRow> = dirs.iter().map(|e| (e.id, e)).collect();

    // Group file children by parent, carrying the fields the predicate + staleness
    // need. One row per file; grouped so the sibling-aware predicate sees a whole
    // directory at once.
    struct FileRow {
        name: String,
        mtime: Option<u64>,
        size: Option<u64>,
    }
    let mut by_parent: HashMap<i64, Vec<FileRow>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare_cached("SELECT parent_id, name, modified_at, logical_size FROM entries WHERE is_directory = 0")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let parent_id: i64 = row.get(0).map_err(|e| e.to_string())?;
            let name: String = row.get(1).map_err(|e| e.to_string())?;
            let mtime: Option<i64> = row.get(2).map_err(|e| e.to_string())?;
            let size: Option<i64> = row.get(3).map_err(|e| e.to_string())?;
            by_parent.entry(parent_id).or_default().push(FileRow {
                name,
                mtime: mtime.map(|v| v as u64),
                size: size.map(|v| v as u64),
            });
        }
    }

    let mut out = Vec::new();
    for (parent_id, files) in &by_parent {
        let dir_path = reconstruct_dir_path(*parent_id, &by_id);
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        for (file, qual) in files.iter().zip(qualify_dir(&names)) {
            if let Qualification::Enrich(kind) = qual {
                out.push(ImageEntry {
                    path: join_path(&dir_path, &file.name),
                    mtime: file.mtime,
                    size: file.size,
                    kind,
                });
            }
        }
    }
    Ok(out)
}

/// Reconstruct a directory's absolute path from the in-memory `id → row` map,
/// walking parent pointers up to the root sentinel. Returns `"/"` for the root.
fn reconstruct_dir_path(id: i64, by_id: &HashMap<i64, &crate::indexing::store::EntryRow>) -> String {
    if id == ROOT_ID {
        return "/".to_string();
    }
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

/// Join a directory path and a file name into an absolute path, avoiding a double
/// slash at the root.
fn join_path(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The stored paths whose source files no longer qualify as images in the CURRENT
/// (completed) index walk — the deletion-driven GC target set (an M1 TDD target).
///
/// A pure set difference: everything stored but not in `current`. Safe ONLY because
/// the caller runs it against a COMPLETED scan (the `Completed` bus edge fires
/// post-writer-flush, so the tree is whole) — never mid-`Scanning`, when the index
/// truncate window transiently empties the tree (plan Decision 3).
pub(crate) fn gc_targets<'a>(stored: impl IntoIterator<Item = &'a String>, current: &HashSet<String>) -> Vec<String> {
    stored.into_iter().filter(|p| !current.contains(*p)).cloned().collect()
}

/// The parent directory of an absolute path (the folder importance keys on). `"/"`
/// for a top-level file. A pure slice, no allocation.
pub(crate) fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &path[..i],
    }
}

/// Order the walked images so HIGH-importance folders enrich first (plan
/// Cross-cutting § Importance-prioritized enrichment): sort by the folder's
/// importance score descending, ties broken by path for determinism. A folder with
/// no score (offline importance DB, floored/unscored, override-only) sorts as `0.0`,
/// so it enriches after the scored folders but is NOT dropped — the `should_enrich`
/// filter, not the ordering, decides what enriches. Returns a fresh ordered `Vec`.
pub(crate) fn prioritized(images: &[ImageEntry], folder_score: &dyn Fn(&str) -> f64) -> Vec<ImageEntry> {
    let mut ordered = images.to_vec();
    ordered.sort_by(|a, b| {
        let sa = folder_score(parent_dir(&a.path));
        let sb = folder_score(parent_dir(&b.path));
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    ordered
}

/// Enrich the stale images and GC vanished rows through `writer`, over a set of
/// already-loaded `statuses` (path → row). Returns what the pass did.
///
/// - `images` is the caller's priority-ordered list ([`prioritized`]); enrichment
///   walks it in that order so high-importance folders land first.
/// - `should_enrich(path)` is the importance/override/exclude filter: an image it
///   rejects is DEFERRED (not enriched) but stays in the GC `current` set, so a
///   below-threshold folder's existing rows aren't wiped — only genuinely vanished
///   files are GC'd.
/// - Enriches only images the staleness predicate marks stale ([`needs_enrichment`]).
/// - Checks `cancel` BETWEEN images so an emergency stop (the memory watchdog)
///   yields promptly; a cancelled pass ALSO skips GC (yield fully) — the vanished
///   rows are collected on the next completed scan.
/// - GC uses the FULL walked image set as `current` (not just the freshly enriched
///   ones), so a still-present image whose enrichment this pass skipped isn't GC'd.
pub(crate) fn enrich_and_gc(
    images: &[ImageEntry],
    statuses: &HashMap<String, MediaStatusRow>,
    backend: &dyn VisionBackend,
    writer: &MediaWriter,
    should_enrich: &dyn Fn(&str) -> bool,
    cancel: &dyn Fn() -> bool,
) -> Result<PassSummary, String> {
    let stamp = backend.analysis_stamp();
    let current: HashSet<String> = images.iter().map(|i| i.path.clone()).collect();

    let mut enriched = 0;
    let mut cancelled = false;
    for image in images {
        if cancel() {
            cancelled = true;
            break;
        }
        // Importance / override / exclude gate: a deferred image is skipped here but
        // stays in `current`, so GC never wipes it.
        if !should_enrich(&image.path) {
            continue;
        }
        if !needs_enrichment(statuses.get(&image.path), image.mtime, image.size, &stamp) {
            continue;
        }
        let input = ImageInput {
            path: image.path.clone(),
            kind: image.kind,
            // Local volume: the backend reads the real on-disk path itself.
            bytes: None,
        };
        match backend.analyze(&input) {
            Ok(analysis) => {
                writer
                    .upsert(
                        status_row(image, EnrichmentState::Done, &stamp),
                        Some(to_upsert_analysis(analysis)),
                    )
                    .map_err(|e| e.to_string())?;
                enriched += 1;
            }
            Err(e) => {
                log::warn!(target: "media_index", "analysis failed for '{}': {e}", image.path);
                writer
                    .upsert(status_row(image, EnrichmentState::Failed, &stamp), None)
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    // Deletion-driven GC — only on a completed scan (this fn's caller runs it on a
    // `Completed` edge / the Fresh sweep), and skipped when cancelled so an
    // emergency stop yields fully.
    let gc_count = if cancelled {
        0
    } else {
        let targets = gc_targets(statuses.keys(), &current);
        let n = targets.len();
        writer.gc_paths(targets).map_err(|e| e.to_string())?;
        n
    };

    writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(PassSummary { enriched, gc_count })
}

/// Build the `media_status` row for an image at a given state and analyze provenance
/// stamp (stored in the `engine_version` column).
pub(crate) fn status_row(image: &ImageEntry, state: EnrichmentState, stamp: &str) -> MediaStatusRow {
    MediaStatusRow {
        path: image.path.clone(),
        mtime: image.mtime,
        size: image.size,
        media_kind: image.kind,
        state,
        engine_version: stamp.to_string(),
    }
}

/// Convert a backend [`Analysis`] into the writer's persistence shape: the OCR text,
/// tags, and embedding a successful `upsert` stores.
pub(crate) fn to_upsert_analysis(analysis: Analysis) -> UpsertAnalysis {
    UpsertAnalysis {
        ocr_text: analysis.ocr.text,
        tags: analysis.tags,
        embedding: analysis.embedding,
    }
}

/// Load every stored `media_status` row for a volume into a `path → row` map. A
/// missing/unopenable DB yields an empty map (a first pass has none).
pub(crate) fn load_statuses(data_dir: &std::path::Path, volume_id: &str) -> HashMap<String, MediaStatusRow> {
    let db_path = crate::media_index::store::media_db_path(data_dir, volume_id);
    let mut out = HashMap::new();
    if let Ok(conn) = crate::media_index::store::open_read_connection(&db_path)
        && let Ok(rows) = crate::media_index::store::read_all_status(&conn)
    {
        for row in rows {
            out.insert(row.path.clone(), row);
        }
    }
    out
}
