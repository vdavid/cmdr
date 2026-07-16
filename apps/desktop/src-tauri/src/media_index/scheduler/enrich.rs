//! The registry-free walk + enrich + GC core of the media scheduler: read a
//! volume's index once, qualify its images, run the backend over the stale ones,
//! and GC rows whose source files vanished. Split out of [`super`] (the coordinator
//! and bus wiring) so this I/O-shaped-but-registry-free logic is directly testable:
//! a test drives it with a synthetic index, a real [`MediaWriter`], and the fake
//! backend, with no registry, no async driver, and no FFI (mirroring `importance`'s
//! `recompute.rs`).

use std::collections::{HashMap, HashSet};

use crate::indexing::store::{IndexStore, ROOT_ID, resolve_path};
use crate::media_index::backend::{Analysis, ImageInput, MediaAnalysis, VisionBackend, VisionError};
use crate::media_index::predicate::{MediaKind, Qualification, qualify_dir};
use crate::media_index::progress::{EnrichProgress, EnrichProgressSink};
use crate::media_index::store::{EnrichmentState, MediaStatusRow, needs_clip, needs_enrichment};
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

/// What one pass did: how many images it enriched, how many rows it GC'd, and whether
/// the memory watchdog cancelled it partway (so the scheduler maps a cancelled pass to
/// a `Cancelled` terminal event, distinct from a clean `Completed`).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct PassSummary {
    pub(crate) enriched: usize,
    pub(crate) gc_count: usize,
    pub(crate) cancelled: bool,
}

/// The pass's side-channels to the outside world: cooperative cancellation and progress
/// reporting. Bundled so the enrich core stays under the argument-count lint and callers
/// pass one value (mirroring the network core's `NetworkEnrichCtx`).
pub(crate) struct PassHooks<'a> {
    /// The emergency-stop check (memory watchdog), checked between images.
    pub(crate) cancel: &'a dyn Fn() -> bool,
    /// The throttled progress sink (the top-right indicator's second publisher).
    /// A no-op in unit tests that don't assert progress.
    pub(crate) progress: &'a dyn EnrichProgressSink,
}

/// The ENRICHABLE-subset denominators for a pass: the count of images passing the
/// coverage gates (`should_enrich` AND not `is_excluded`) and their total bytes
/// (`ImageEntry.size`, a `None` counting 0). This is the honest progress denominator —
/// NEVER `images.len()`, which would leave the bar stuck at "150 of 223,228" for a
/// volume most of whose images are deferred below the slider threshold. Pure,
/// so the denominator rule is unit-testable.
pub(crate) fn enrichable_totals(
    images: &[ImageEntry],
    should_enrich: &dyn Fn(&str) -> bool,
    is_excluded: &dyn Fn(&str) -> bool,
) -> (u64, u64) {
    let mut total = 0u64;
    let mut bytes_total = 0u64;
    for image in images {
        if !is_excluded(&image.path) && should_enrich(&image.path) {
            total += 1;
            bytes_total += image.size.unwrap_or(0);
        }
    }
    (total, bytes_total)
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

/// Walk ONLY the given directories' qualifying images — the live-tick scoped walk,
/// the incremental counterpart to [`walk_image_entries`]'s whole-index
/// sweep. For each touched dir it resolves the dir's entry id and fetches ALL of that
/// dir's file children, then runs the sibling-aware predicate over the COMPLETE name
/// set — fetching only the changed files would mis-qualify (RAW+JPEG pairing and Live
/// Photos are sibling-aware, so deleting `DSC.jpg` must promote the lone `DSC.cr2`).
/// A dir absent from the index (removed since the change fired) is skipped — its
/// stored rows fall to the scoped GC. `dirs` are absolute index paths; a network
/// volume never reaches here (live-follow is Local-only), so no mount mapping.
pub(crate) fn walk_image_entries_in_dirs(
    conn: &rusqlite::Connection,
    dirs: &HashSet<String>,
) -> Result<Vec<ImageEntry>, String> {
    let mut out = Vec::new();
    for dir in dirs {
        // A dir gone from the index resolves to `None`: skip it (its rows fall to the
        // scoped GC). The bare `/` resolves to `ROOT_ID`, so listing its direct children
        // is a cheap no-op rather than a whole-index walk.
        let Some(dir_id) = resolve_path(conn, dir).map_err(|e| e.to_string())? else {
            continue;
        };
        let children = IndexStore::list_children_on(dir_id, conn).map_err(|e| e.to_string())?;
        let files: Vec<&crate::indexing::store::EntryRow> = children.iter().filter(|c| !c.is_directory).collect();
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        for (file, qual) in files.iter().zip(qualify_dir(&names)) {
            if let Qualification::Enrich(kind) = qual {
                out.push(ImageEntry {
                    path: join_path(dir, &file.name),
                    mtime: file.modified_at,
                    size: file.logical_size,
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

/// Which stored rows a pass may GC — the data-safety line between the full pass and a
/// scoped live tick.
///
/// GC deletes a stored row whose source path is absent from the pass's `current`
/// (walked) set. A FULL pass walks the WHOLE index, so every stored row absent from
/// the walk genuinely vanished ⇒ [`WholeStore`](GcScope::WholeStore). A live tick walks
/// ONLY the touched dirs, so a whole-store set-difference against its scoped walk would
/// delete every row in every dir the tick never visited — the data-safety trap. A tick
/// must therefore GC only rows UNDER the dirs it actually walked
/// ⇒ [`TouchedDirs`](GcScope::TouchedDirs).
#[derive(Clone, Copy)]
pub(crate) enum GcScope<'a> {
    /// GC every stored row absent from the (complete) walk. The full pass / Fresh sweep.
    WholeStore,
    /// GC only stored rows whose parent dir is in this set AND absent from the (scoped)
    /// walk. The live tick, whose walk covers exactly these dirs — never the whole store.
    TouchedDirs(&'a HashSet<String>),
}

/// The per-pass POLICY the enrich core applies: which images to enrich, which the privacy
/// veto forbids, and which stored rows to GC. Bundled so the core stays under the
/// argument-count lint (like [`PassHooks`]) and so the full pass and the scoped live tick
/// differ in ONE value the caller supplies.
pub(crate) struct EnrichGates<'a> {
    /// The COVERAGE filter (importance threshold + "always index" override, snapshot):
    /// a rejected image is DEFERRED but stays in the GC `current` set.
    pub(crate) should_enrich: &'a dyn Fn(&str) -> bool,
    /// The LIVE privacy veto (read fresh, beats coverage), checked before enriching AND
    /// again right before the upsert (the in-flight-analyze TOCTOU).
    pub(crate) is_excluded: &'a dyn Fn(&str) -> bool,
    /// Which stored rows this pass may GC: the whole store (full pass) or only rows under
    /// the touched dirs (live tick) — the scoped-GC data-safety line.
    pub(crate) gc_scope: GcScope<'a>,
    /// The currently-installed CLIP model's provenance stamp, or `None` when no CLIP model
    /// is installed. Drives the INDEPENDENT CLIP half of two-part staleness
    /// ([`needs_clip`]): an image whose stored `clip_stamp` differs gets CLIP-embedded even
    /// when its Vision analysis is current, so installing/upgrading CLIP re-embeds without
    /// re-running OCR/tags. `None` ⇒ CLIP is never attempted.
    pub(crate) clip_stamp: Option<&'a str>,
}

/// The stored paths whose source files no longer qualify as images in the CURRENT
/// (completed) index walk — the deletion-driven GC target set (a TDD target).
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

/// The whole-store entry point: enrich the stale covered images and GC every stored row
/// the COMPLETE walk no longer holds. The full pass and the Fresh sweep call this — they
/// walk the whole index, so a missing row genuinely vanished. Delegates to
/// [`enrich_and_gc_scoped`] with [`GcScope::WholeStore`]; the scoped live tick passes
/// [`GcScope::TouchedDirs`] instead. Both share the one per-image loop.
///
/// Test-only now: production reaches [`enrich_and_gc_scoped`] directly with the installed
/// CLIP stamp (this Vision-only wrapper can't carry it), so the OCR/tag tests keep a terse
/// entry point without a model.
#[cfg(test)]
pub(crate) fn enrich_and_gc(
    images: &[ImageEntry],
    statuses: &HashMap<String, MediaStatusRow>,
    backend: &dyn VisionBackend,
    writer: &MediaWriter,
    should_enrich: &dyn Fn(&str) -> bool,
    is_excluded: &dyn Fn(&str) -> bool,
    hooks: &PassHooks,
) -> Result<PassSummary, String> {
    enrich_and_gc_scoped(
        images,
        statuses,
        backend,
        writer,
        &EnrichGates {
            should_enrich,
            is_excluded,
            gc_scope: GcScope::WholeStore,
            // The whole-store wrapper is CLIP-agnostic (Vision-only): the production full
            // pass reaches the scoped core directly with the installed CLIP stamp, and the
            // OCR/tag tests use this wrapper without a model.
            clip_stamp: None,
        },
        hooks,
    )
}

/// The shared enrich + GC core, over a set of already-loaded `statuses` (path → row).
/// Parameterized by `gates.gc_scope` so the whole-store full pass and the touched-dirs live
/// tick share ONE per-image loop (never a fork). Callers usually reach it via
/// [`enrich_and_gc`] (whole store); the live tick calls it directly with
/// [`GcScope::TouchedDirs`]. Returns what the pass did.
///
/// - `images` is the caller's priority-ordered list ([`prioritized`]); enrichment
///   walks it in that order so high-importance folders land first.
/// - `gates.should_enrich(path)` is the COVERAGE filter (importance threshold + "always
///   index" override, snapshot-based): an image it rejects is DEFERRED (not enriched)
///   but stays in the GC `current` set, so a below-threshold folder's existing rows
///   aren't wiped — only genuinely vanished files are GC'd.
/// - `gates.is_excluded(path)` is the LIVE privacy veto (read fresh, NOT from a pass
///   snapshot). It's a hard veto that beats coverage, checked BOTH before enriching
///   AND again immediately before the upsert: the second check closes the in-flight
///   TOCTOU where an exclusion lands DURING the slow `analyze`, so a just-excluded
///   folder never gets a row persisted (which a later pass wouldn't collect, since the
///   file is still present in the GC `current` set). An excluded image is deferred, so
///   like any deferred image it stays in `current` and isn't GC'd.
/// - Enriches only images the staleness predicate marks stale ([`needs_enrichment`]).
/// - A VANISHED source (a typed [`VisionError::Missing`], an ENOENT-class read
///   failure) is skipped QUIETLY (DEBUG, no row) but still counts toward `done` —
///   the vanished/phantom-file handling.
/// - Checks `hooks.cancel` BETWEEN images so an emergency stop (the memory watchdog)
///   yields promptly; a cancelled pass ALSO skips GC (yield fully) — the vanished
///   rows are collected on the next completed scan — and returns `cancelled: true`.
/// - Reports throttled progress through `hooks.progress` over the ENRICHABLE subset
///   (the honest denominator), so image indexing joins the top-right indicator.
/// - GC uses the walked image set as `current` (not just the freshly enriched ones), so
///   a still-present image whose enrichment this pass skipped isn't GC'd. `gates.gc_scope`
///   decides WHICH stored rows are GC candidates: a full pass considers the whole store
///   ([`GcScope::WholeStore`]); a scoped live tick considers only rows under the touched
///   dirs ([`GcScope::TouchedDirs`]), so it never wipes rows in dirs it didn't walk
///   (the data-safety trap).
pub(crate) fn enrich_and_gc_scoped(
    images: &[ImageEntry],
    statuses: &HashMap<String, MediaStatusRow>,
    backend: &dyn VisionBackend,
    writer: &MediaWriter,
    gates: &EnrichGates,
    hooks: &PassHooks,
) -> Result<PassSummary, String> {
    let should_enrich = gates.should_enrich;
    let is_excluded = gates.is_excluded;
    let stamp = backend.analysis_stamp();
    let current: HashSet<String> = images.iter().map(|i| i.path.clone()).collect();

    // The honest progress denominator: the enrichable subset, never the full
    // walked set. `done` counts every subset image the pass finishes handling — enriched,
    // already-current, or a quiet vanished skip — so it reaches `total` on completion.
    let (total, bytes_total) = enrichable_totals(images, should_enrich, is_excluded);
    let mut done = 0u64;
    let mut bytes_done = 0u64;
    // The pass-start tick, so the indicator row appears immediately at 0 / total.
    hooks.progress.report(EnrichProgress {
        done,
        total,
        bytes_done,
        bytes_total,
    });

    let mut enriched = 0;
    let mut cancelled = false;
    for image in images {
        if (hooks.cancel)() {
            cancelled = true;
            break;
        }
        // Privacy veto (LIVE hard veto, beats coverage): an excluded image is deferred
        // like any other, so it stays in `current` and GC never wipes it. Not in the
        // enrichable subset, so it doesn't count toward `done` / `total`.
        if is_excluded(&image.path) {
            continue;
        }
        // Coverage gate: a deferred image is skipped here but stays in `current`, so
        // GC never wipes it. Also not in the subset.
        if !should_enrich(&image.path) {
            continue;
        }
        // In the enrichable subset ⇒ count it as processed no matter the outcome
        // (enriched, already-current, or a quiet skip), so the bar reaches `total`.
        done += 1;
        bytes_done += image.size.unwrap_or(0);

        let stored = statuses.get(&image.path);
        let want_vision = needs_enrichment(stored, image.mtime, image.size, &stamp);
        let want_clip = needs_clip(stored, gates.clip_stamp);
        if want_vision || want_clip {
            let input = ImageInput {
                path: image.path.clone(),
                kind: image.kind,
                // Local volume: the backend reads the real on-disk path itself.
                bytes: None,
            };
            // ONE decode runs the requested side(s) (plan M3 Q5).
            let analysis = backend.analyze_media(&input, want_vision, want_clip);
            // Re-check the LIVE veto AFTER the slow analyze: an exclusion that landed
            // during it must not persist a row (the in-flight-analyze TOCTOU — a later
            // pass wouldn't collect it, since the file is still in the GC `current` set).
            if !is_excluded(&image.path) {
                match analysis {
                    Ok(media) => {
                        if apply_media_upsert(writer, image, &stamp, gates.clip_stamp, want_vision, media)? {
                            enriched += 1;
                        }
                    }
                    // A VANISHED source (ENOENT-class) — a file deleted between the walk
                    // and its analyze, or an orphaned index row's phantom path: skip
                    // QUIETLY (DEBUG, never WARN), write NO row (the file is gone; a
                    // later completed pass's GC collects any stale row). It already
                    // counted toward `done` above, so the bar still reaches `total`.
                    // Typed variant, never a message match.
                    Err(VisionError::Missing(msg)) => {
                        log::debug!(target: "media_index", "skipping vanished image '{}': {msg}", image.path);
                    }
                    // A present-but-bad file (a good read, a decode/OCR failure) ⇒ a real
                    // per-file failure. Mark the Vision side `Failed` (if it was attempted),
                    // and stamp the CLIP side so a bad file isn't re-decoded for CLIP every
                    // pass (embedding `None` = stamp-without-vector).
                    Err(e) => {
                        log::warn!(target: "media_index", "analysis failed for '{}': {e}", image.path);
                        if want_vision {
                            writer
                                .upsert(status_row(image, EnrichmentState::Failed, &stamp), None)
                                .map_err(|e| e.to_string())?;
                        }
                        if want_clip
                            && let Some(clip_stamp) = gates.clip_stamp
                        {
                            writer
                                .upsert_clip(image.path.clone(), clip_stamp.to_string(), None)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
        }
        hooks.progress.report(EnrichProgress {
            done,
            total,
            bytes_done,
            bytes_total,
        });
    }

    // Deletion-driven GC — only on a completed scan (this fn's caller runs it on a
    // `Completed` edge / the Fresh sweep), and skipped when cancelled so an
    // emergency stop yields fully.
    let gc_count = if cancelled {
        0
    } else {
        let targets: Vec<String> = match gates.gc_scope {
            // The full pass: every stored row absent from the complete walk vanished.
            GcScope::WholeStore => gc_targets(statuses.keys(), &current),
            // The live tick: only rows UNDER a walked (touched) dir are candidates, so a
            // row in a dir this tick never visited is never GC'd.
            GcScope::TouchedDirs(dirs) => statuses
                .keys()
                .filter(|p| dirs.contains(parent_dir(p)) && !current.contains(*p))
                .cloned()
                .collect(),
        };
        let n = targets.len();
        writer.gc_paths(targets).map_err(|e| e.to_string())?;
        n
    };

    writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(PassSummary {
        enriched,
        gc_count,
        cancelled,
    })
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
        clip_stamp: String::new(),
    }
}

/// Persist the requested side(s) of a combined [`MediaAnalysis`] (plan M3 two-part
/// writes): the Vision analysis (when `want_vision`) via the Vision `upsert`, and the CLIP
/// embedding (when the backend produced one) via `upsert_clip` — each independent, so a
/// CLIP-only pass never disturbs stored OCR/tags and vice versa. A CLIP side that couldn't
/// encode yet (model still loading) leaves `clip_stamp` unstamped, so the next pass retries
/// it. Returns whether anything was persisted (for the `enriched` counter).
pub(crate) fn apply_media_upsert(
    writer: &MediaWriter,
    image: &ImageEntry,
    stamp: &str,
    clip_stamp: Option<&str>,
    want_vision: bool,
    media: MediaAnalysis,
) -> Result<bool, String> {
    let mut did = false;
    if want_vision && let Some(vision) = media.vision {
        writer
            .upsert(
                status_row(image, EnrichmentState::Done, stamp),
                Some(to_upsert_analysis(vision)),
            )
            .map_err(|e| e.to_string())?;
        did = true;
    }
    if let Some(clip_vec) = media.clip
        && let Some(clip_stamp) = clip_stamp
    {
        writer
            .upsert_clip(image.path.clone(), clip_stamp.to_string(), Some(clip_vec))
            .map_err(|e| e.to_string())?;
        did = true;
    }
    Ok(did)
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
