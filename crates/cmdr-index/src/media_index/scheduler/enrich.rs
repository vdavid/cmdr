//! The registry-free walk + enrich + GC core of the media scheduler: read a
//! volume's index once, qualify its images, run the backend over the stale ones,
//! and GC rows whose source files vanished. Split out of [`super`] (the coordinator
//! and bus wiring) so this I/O-shaped-but-registry-free logic is directly testable:
//! a test drives it with a synthetic index, a real [`MediaWriter`], and the fake
//! backend, with no registry, no async driver, and no FFI (mirroring `importance`'s
//! `recompute.rs`).

use std::collections::{HashMap, HashSet};

use crate::indexing::store::{DirTree, IndexStore, resolve_path};
use crate::media_index::backend::{Analysis, MediaAnalysis, VisionBackend};
use crate::media_index::paths::parent_dir;
use crate::media_index::predicate::{MediaKind, Qualification, qualify_dir};
use crate::media_index::progress::EnrichProgressSink;
use crate::media_index::store::{EnrichmentState, MediaStatusRow};
use crate::media_index::writer::{MediaWriter, UpsertAnalysis};

/// One qualifying image discovered while walking the index: its absolute path, the
/// `(mtime, size)` staleness key, and the typed kind the predicate assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageEntry {
    /// Absolute path, as the drive index stores it.
    pub path: String,
    /// Modified time as the index last saw it; `None` when it has none.
    pub mtime: Option<u64>,
    /// Size in bytes as the index last saw it.
    pub size: Option<u64>,
    /// What kind of media the name says it is.
    pub kind: MediaKind,
}

/// What one pass did: how many images it enriched, how many rows it GC'd, and whether
/// the memory watchdog cancelled it partway (so the scheduler maps a cancelled pass to
/// a `Cancelled` terminal event, distinct from a clean `Completed`).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct PassSummary {
    pub(crate) enriched: usize,
    pub(crate) gc_count: usize,
    pub(crate) cancelled: bool,
    /// Files whose BYTES couldn't be read (`FetchError::Unreadable`: permission
    /// denied and friends) — skipped without a row write, counted so the pass can
    /// log the total honestly. Always 0 on a local pass (the backend reads the
    /// file itself; a local read failure surfaces as a decode failure → `Failed`).
    pub(crate) skipped_unreadable: usize,
}

/// The pass's side-channels to the outside world: cooperative cancellation and progress
/// reporting. Bundled so the enrich core stays under the argument-count lint and callers
/// pass one value (mirroring the network core's `NetworkEnrichCtx`).
pub(crate) struct PassHooks<'a> {
    /// The emergency-stop check (memory watchdog), checked between images. `+ Sync`
    /// because a parallel pass checks it from every worker thread.
    pub(crate) cancel: &'a (dyn Fn() -> bool + Sync),
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
    should_enrich: &(dyn Fn(&str) -> bool + Sync),
    is_excluded: &(dyn Fn(&str) -> bool + Sync),
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

/// One file row carried through the streaming walk: the fields the sibling-aware
/// predicate and the `(mtime, size)` staleness key need.
struct FileRow {
    name: String,
    mtime: Option<u64>,
    size: Option<u64>,
}

/// One qualifying image handed to a walk sink, still SPLIT into its directory and file
/// name so a counting sink never has to build the absolute path. A sink that needs the
/// path joins it itself ([`QualifyingImage::path`]).
pub(crate) struct QualifyingImage<'a> {
    /// The directory's absolute path, reconstructed once per parent group and reused for
    /// every qualifying image in it.
    pub(crate) dir: &'a str,
    /// The file name within `dir`.
    pub(crate) name: &'a str,
    pub(crate) mtime: Option<u64>,
    pub(crate) size: Option<u64>,
    pub(crate) kind: MediaKind,
}

impl QualifyingImage<'_> {
    /// The absolute path, allocated only when a sink actually needs it.
    pub(crate) fn path(&self) -> String {
        join_path(self.dir, self.name)
    }
}

/// Walk every directory in a volume's index, qualify each directory's files
/// (sibling-aware, via [`qualify_dir`]), and hand each qualifying image to `sink` — the
/// ONE walk shape, so a counting consumer and a collecting one can never disagree about
/// what qualifies.
///
/// Directories are held for the whole walk (path reconstruction follows parent pointers in
/// any order), but in the compact [`DirTree`] shape: an arena plus 24 bytes per folder, never
/// a `Vec<EntryRow>` and a heap `String` each. Files stream ordered by `parent_id` so each
/// directory's children arrive as one contiguous group, and the walk holds only the single
/// in-flight group, never the whole file set (which on an 11.5M-row index would be a
/// transient `by_parent` map in the hundreds of MB). The `idx_parent_name_folded` index leads
/// on `parent_id`, so SQLite supplies the order off the index. Sink order therefore follows
/// `parent_id`, not insertion order — no caller depends on it (coverage aggregates into a map;
/// the passes re-sort via [`prioritized`]).
///
/// On top of that floor, resident memory is whatever the SINK keeps: `O(folders)` for
/// `count_qualifying_images`,
/// `O(images)` for the collecting [`walk_image_entries`]. Reach for the counting sink
/// whenever only counts are needed; a multi-million-entry NAS index turns the collecting
/// one into gigabytes (11.3M entries, measured 2026-07-25 —
/// `docs/notes/memory-runaway-rust-heap-2026-07-25.md`).
pub(crate) fn for_each_qualifying_image(
    conn: &rusqlite::Connection,
    sink: &mut dyn FnMut(&QualifyingImage<'_>),
) -> Result<(), String> {
    let mut dirs = DirTree::load(conn)?;

    let mut stmt = conn
        .prepare_cached(
            "SELECT parent_id, name, modified_at, logical_size FROM entries WHERE is_directory = 0 ORDER BY parent_id",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    // Accumulate the current parent's file group; when parent_id changes, qualify the
    // completed group (sibling-aware) and emit its images, then reset for the next dir.
    // One reused path buffer, so a volume's folders cost one allocation between them.
    let mut group_parent: Option<i64> = None;
    let mut group: Vec<FileRow> = Vec::new();
    let mut dir_path = String::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let parent_id: i64 = row.get(0).map_err(|e| e.to_string())?;
        let name: String = row.get(1).map_err(|e| e.to_string())?;
        let mtime: Option<i64> = row.get(2).map_err(|e| e.to_string())?;
        let size: Option<i64> = row.get(3).map_err(|e| e.to_string())?;
        if group_parent != Some(parent_id) {
            if let Some(pid) = group_parent {
                dirs.path_into(pid, &mut dir_path);
                emit_qualifying_group(&dir_path, &group, sink);
            }
            group_parent = Some(parent_id);
            group.clear();
        }
        group.push(FileRow {
            name,
            mtime: mtime.map(|v| v as u64),
            size: size.map(|v| v as u64),
        });
    }
    if let Some(pid) = group_parent {
        dirs.path_into(pid, &mut dir_path);
        emit_qualifying_group(&dir_path, &group, sink);
    }
    Ok(())
}

/// Walk every directory in a volume's index and COLLECT the qualifying image entries with
/// their absolute path, `(mtime, size)`, and kind — [`for_each_qualifying_image`] with a
/// collecting sink.
///
/// This holds one heap `String` path per qualifying image, so it's for the passes that
/// genuinely need the list (enrich, GC, the cache refill). ❌ Never use it to derive counts:
/// see `count_qualifying_images`.
pub(crate) fn walk_image_entries(conn: &rusqlite::Connection) -> Result<Vec<ImageEntry>, String> {
    let mut out = Vec::new();
    for_each_qualifying_image(conn, &mut |image| {
        out.push(ImageEntry {
            path: image.path(),
            mtime: image.mtime,
            size: image.size,
            kind: image.kind,
        });
    })?;
    Ok(out)
}

/// Qualify one directory's COMPLETE file group (sibling-aware) and hand its qualifying
/// images to `sink`, under the already-reconstructed `dir_path`.
/// [`for_each_qualifying_image`] calls this once per parent group — the group is complete
/// here, which is exactly what the sibling-aware rules (RAW+JPEG pairing, Live Photos)
/// need, so ❌ never move qualification to a per-row shape.
fn emit_qualifying_group(dir_path: &str, files: &[FileRow], sink: &mut dyn FnMut(&QualifyingImage<'_>)) {
    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    for (file, qual) in files.iter().zip(qualify_dir(&names)) {
        if let Qualification::Enrich(kind) = qual {
            sink(&QualifyingImage {
                dir: dir_path,
                name: &file.name,
                mtime: file.mtime,
                size: file.size,
                kind,
            });
        }
    }
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
    /// a rejected image is DEFERRED but stays in the GC `current` set. `+ Sync` because a
    /// parallel pass calls it from every worker thread.
    pub(crate) should_enrich: &'a (dyn Fn(&str) -> bool + Sync),
    /// The LIVE privacy veto (read fresh, beats coverage), checked before enriching AND
    /// again right before the upsert (the in-flight-analyze TOCTOU). `+ Sync` for the same
    /// reason.
    pub(crate) is_excluded: &'a (dyn Fn(&str) -> bool + Sync),
    /// Which stored rows this pass may GC: the whole store (full pass) or only rows under
    /// the touched dirs (live tick) — the scoped-GC data-safety line.
    pub(crate) gc_scope: GcScope<'a>,
    /// The currently-installed CLIP model's provenance stamp, or `None` when no CLIP model
    /// is installed. Drives the INDEPENDENT CLIP half of two-part staleness
    /// ([`needs_clip`](crate::media_index::store::needs_clip)): an image whose stored `clip_stamp` differs gets CLIP-embedded even
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
    should_enrich: &(dyn Fn(&str) -> bool + Sync),
    is_excluded: &(dyn Fn(&str) -> bool + Sync),
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
/// `enrich_and_gc` (whole store); the live tick calls it directly with
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
/// - Enriches only images the staleness predicate marks stale ([`needs_enrichment`](crate::media_index::store::needs_enrichment)).
/// - A VANISHED source (a typed [`VisionError::Missing`](crate::media_index::backend::VisionError::Missing), an ENOENT-class read
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
    // The serial pass IS the parallel pool at ONE worker: worker 0 rides `backend` and
    // pulls every image in cursor order, so a steady N=1 pass is byte-for-byte the old
    // sequential loop (same order, same writes, same GC). `make` is never called at
    // width 1 (worker 0 reuses `backend`), so it's unreachable here.
    super::pool::run_enrich_pool(
        images,
        statuses,
        backend,
        &(|| -> std::sync::Arc<dyn VisionBackend> { unreachable!("the N=1 serial path never builds an extra backend") })
            as &super::pool::MakeBackend,
        &|| 1,
        writer,
        gates,
        hooks.cancel,
        hooks.progress,
    )
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
