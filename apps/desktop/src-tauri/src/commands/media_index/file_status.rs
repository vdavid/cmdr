//! Per-file + per-folder index status (the quiet glanceable indicators): the file-icon
//! overlay's per-path state, and the folder badge's eligible/accounted subtree counts.
//!
//! Classification is entirely backend-side (smart-backend/thin-frontend) and its core is
//! PURE over already-resolved inputs, so every state is unit-testable; those tests live in
//! `commands/tests.rs`.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use cmdr_index::media_index::coverage;
use cmdr_index::media_index::gate;
use cmdr_index::media_index::network::config as network_config;
use cmdr_index::media_index::read;
use cmdr_index::media_index::scheduler::{self, MediaScheduler};
use cmdr_index::media_index::store;

/// The index status of ONE file, as the file-icon overlay reads it. Serialized
/// camelCase across the IPC boundary; classification is entirely backend-side (the
/// frontend renders an icon per state, never re-deriving from mtime/size).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FileIndexState {
    /// A `done` row whose `(mtime, size)` + analyze stamp are current — fully indexed.
    Indexed,
    /// A stored row whose live file changed since indexing (or the analyze engine
    /// bumped): it needs re-enrichment. The same `needs_enrichment` predicate a pass uses.
    Stale,
    /// A `failed` row (a broken/undecodable file). Won't progress on its own.
    Failed,
    /// An indexable image that the coverage gate would enrich, but which has no stored
    /// row yet, and NO pass is running for the volume — genuinely queued, indexing hasn't
    /// reached it.
    Pending,
    /// An indexable image with no stored row yet WHILE a pass is actively running for the
    /// volume: it's being worked on right now, not merely queued. Distinct from `Pending`
    /// so a file whose row was transiently GC'd during a re-enriching pass (a move/rename)
    /// reads "indexing now" rather than a false "never indexed."
    Indexing,
    /// An indexable image the coverage gate would NOT enrich: out of scope, below the
    /// importance threshold, or under an excluded folder.
    Excluded,
    /// Not an indexable media type (a video, a document, a folder, a RAW deferring to a
    /// JPEG sibling, or a file the index no longer holds) — the frontend renders no badge.
    NotApplicable,
}

/// One file's path + its index status. `path` echoes the request exactly (the frontend
/// keys its per-path map by it).
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileIndexStatus {
    pub path: String,
    pub state: FileIndexState,
}

/// Classify the index status of each file in `paths` (in request order) on `volume_id`.
///
/// **Paths are in the volume's INDEX-path space** — for a local volume that equals the
/// OS path, which is what the file lists show; a network volume's mount-root mapping is a
/// later slice (the file overlay ships local-first). The classification is bounded to the
/// paths passed (the visible rows), so a per-path drive-index lookup is cheap.
///
/// Backend-owned classification (smart-backend/thin-frontend): the drive index supplies
/// each path's live `(mtime, size)` and whether it qualifies as an image (sibling-aware
/// `qualify_dir`), `media.db` supplies the stored row, and the coverage gate
/// (`local_should_enrich` + the live exclusion veto) decides `pending` vs `excluded` for
/// an un-enriched image. A stored row wins over the gate: an already-indexed image reads
/// `indexed`/`stale`/`failed` even if the current setting no longer covers it (the rows
/// stay searchable, forward-only).
#[tauri::command]
#[specta::specta]
pub async fn media_index_file_status(
    app: AppHandle,
    volume_id: String,
    paths: Vec<String>,
) -> Result<Vec<FileIndexStatus>, String> {
    // Feature off ⇒ no badges. The frontend gates the overlay on the setting; this is
    // defense in depth (mirrors the other commands' feature-off short-circuit).
    if !gate::is_enabled() {
        return Ok(paths
            .into_iter()
            .map(|path| FileIndexStatus {
                path,
                state: FileIndexState::NotApplicable,
            })
            .collect());
    }
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let scope = gate::scope();
    let threshold = gate::importance_threshold();
    // The scheduler owns the backend that supplies the analyze provenance stamp (for the
    // `stale` check). A missing one (an early call before `start`) leaves the stamp `None`,
    // and classification falls back to comparing only `(mtime, size)` per row — an honest
    // degrade. The stamp itself is fetched INSIDE `spawn_blocking` (it can touch the Vision
    // framework), never on the IPC/async thread.
    let scheduler = app.try_state::<Arc<MediaScheduler>>().map(|s| Arc::clone(s.inner()));
    let vid = volume_id.clone();

    super::OVERLAY_QUERIES
        .run(move || {
            let stamp = scheduler.as_ref().map(|s| s.current_analysis_stamp());
            // A pass running for this volume ⇒ an un-rowed covered image is being worked on
            // NOW (`indexing`), not merely queued (`pending`). Cheap in-memory snapshot.
            let is_enriching = scheduler.as_ref().is_some_and(|s| s.is_enriching(&vid));
            classify_file_statuses(&data_dir, &vid, paths, scope, threshold, stamp.as_deref(), is_enriching)
        })
        .await
        .map_err(|e| format!("file-status task panicked: {e}"))
}

/// The coverage `scores` map the file-status gate consults, mirroring `pass_coverage`:
/// the automatic scope loads importance and filters to folders scoring at or above
/// `threshold` (what `local_should_enrich` checks membership against); the narrow scope
/// reads no importance at all (override-only, so `None`). An unscored volume in the
/// automatic scope is `None` too, matching the pass's defer-to-override-only behavior.
///
/// The filtered map is cached per volume, so this stays cheap however often the badge
/// query runs; ❌ don't re-derive it from `coverage::importance_scores` here, which is
/// a copy of every scored folder per call.
fn coverage_scores(
    data_dir: &std::path::Path,
    volume_id: &str,
    scope: gate::IndexScope,
    threshold: f64,
) -> Option<Arc<std::collections::HashMap<String, f64>>> {
    if !scope.consults_importance() {
        return None;
    }
    coverage::importance_scores(data_dir, volume_id, Some(threshold))
}

/// Resolve the inputs (qualifying-image walk + stored rows + coverage scores) and hand
/// off to the pure [`classify_all`]. Runs on the blocking worker.
fn classify_file_statuses(
    data_dir: &std::path::Path,
    volume_id: &str,
    paths: Vec<String>,
    scope: gate::IndexScope,
    threshold: f64,
    stamp: Option<&str>,
    is_enriching: bool,
) -> Vec<FileIndexStatus> {
    // The qualifying images (sibling-aware, with live `(mtime, size)`) for exactly the
    // dirs the requested paths live in — a bounded, scoped index walk.
    let qualifying = read::qualifying_images_for_paths(volume_id, &paths);

    // Stored rows for exactly the requested paths (bounded; a per-path point lookup).
    let stored = read::MediaIndex::open(data_dir, volume_id).status_for_paths(&paths);

    // Coverage inputs for the pending/excluded split (only consulted for un-enriched
    // images), threshold-filtered exactly as the enrichment gate sees them.
    let scores = coverage_scores(data_dir, volume_id, scope, threshold);
    let config = network_config::snapshot();

    classify_all(
        &paths,
        &qualifying,
        &stored,
        stamp,
        scores.as_deref(),
        &config,
        volume_id,
        is_enriching,
    )
}

/// The PURE classification core: one [`FileIndexStatus`] per input path, in order,
/// decided from already-resolved inputs (no index, DB, or app), so every state is
/// directly unit-testable.
#[allow(
    clippy::too_many_arguments,
    reason = "the classification inputs are all distinct and resolved once"
)]
pub(super) fn classify_all(
    paths: &[String],
    qualifying: &std::collections::HashMap<String, read::ImageEntry>,
    stored: &std::collections::HashMap<String, store::MediaStatusRow>,
    stamp: Option<&str>,
    scores: Option<&std::collections::HashMap<String, f64>>,
    config: &network_config::NetworkEnrichConfig,
    volume_id: &str,
    is_enriching: bool,
) -> Vec<FileIndexStatus> {
    paths
        .iter()
        .map(|path| FileIndexStatus {
            path: path.clone(),
            state: classify_one(
                path,
                qualifying.get(path),
                stored.get(path),
                stamp,
                scores,
                config,
                volume_id,
                is_enriching,
            ),
        })
        .collect()
}

/// Classify ONE path. Priority: a non-qualifying entry is `notApplicable`; otherwise a
/// stored row decides `failed`/`stale`/`indexed` (an indexed image reads as such even if
/// the current setting no longer covers it — forward-only); with no row, the coverage
/// gate splits a covered image into `indexing` (a pass is running now) vs `pending` (none
/// is), and an uncovered one into `excluded`.
#[allow(
    clippy::too_many_arguments,
    reason = "the classification inputs are all distinct and resolved once"
)]
pub(super) fn classify_one(
    path: &str,
    entry: Option<&read::ImageEntry>,
    stored: Option<&store::MediaStatusRow>,
    stamp: Option<&str>,
    scores: Option<&std::collections::HashMap<String, f64>>,
    config: &network_config::NetworkEnrichConfig,
    volume_id: &str,
    is_enriching: bool,
) -> FileIndexState {
    use cmdr_index::media_index::store::{EnrichmentState, needs_enrichment};

    let Some(entry) = entry else {
        // Not a qualifying image (a video, document, folder, RAW+JPEG dup, or a path the
        // index no longer holds) — no badge.
        return FileIndexState::NotApplicable;
    };
    match stored {
        Some(row) => match row.state {
            EnrichmentState::Failed => FileIndexState::Failed,
            EnrichmentState::Done => {
                // The engine stamp defaults to the row's own when the scheduler is
                // unavailable, so a `None` stamp flags only `(mtime, size)` changes.
                let engine = stamp.unwrap_or(&row.engine_version);
                if needs_enrichment(Some(row), entry.mtime, entry.size, engine) {
                    FileIndexState::Stale
                } else {
                    FileIndexState::Indexed
                }
            }
        },
        None => {
            // No row yet: would a pass enrich it? The live exclusion veto beats coverage.
            let covered = !config.is_excluded(path) && scheduler::local_should_enrich(path, scores, config, volume_id);
            if !covered {
                FileIndexState::Excluded
            } else if is_enriching {
                // A pass is running for the volume, so this covered-but-unrowed image is
                // being worked on now (or its row was transiently GC'd mid-reenrich by a
                // move/rename), not merely queued.
                FileIndexState::Indexing
            } else {
                FileIndexState::Pending
            }
        }
    }
}

/// One folder's index coverage: the eligible denominator and accounted numerator, each a
/// subtree total (the folder plus all its descendants). Serialized camelCase; the
/// frontend derives the two-state badge (`accounted == eligible` vs `accounted <
/// eligible`, no badge when `eligible == 0`) and the `accounted/eligible` tooltip.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderCoverage {
    pub path: String,
    /// Images under this folder (subtree) the drive index says qualify for indexing.
    pub eligible: u64,
    /// Of those, how many have a stored `done`/`failed` row (both count as accounted).
    pub accounted: u64,
}

/// The eligible + accounted subtree counts for each folder in `folder_paths` (in request
/// order) on `volume_id`. Reads the per-volume rollups the aggregate maintains
/// ([`coverage::folder_coverage`]); never scans `media.db` per query. Folder paths are in
/// the volume's INDEX-path space (== OS path for a local volume), matching the stored
/// rows and the eligible cache.
#[tauri::command]
#[specta::specta]
pub async fn media_index_folder_coverage(
    app: AppHandle,
    volume_id: String,
    folder_paths: Vec<String>,
) -> Result<Vec<FolderCoverage>, String> {
    // Feature off ⇒ nothing is indexed, so no folder badges.
    if !gate::is_enabled() {
        return Ok(folder_paths
            .into_iter()
            .map(|path| FolderCoverage {
                path,
                eligible: 0,
                accounted: 0,
            })
            .collect());
    }
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let vid = volume_id.clone();

    super::OVERLAY_QUERIES
        .run(move || {
            let counts = coverage::folder_coverage(&data_dir, &vid, &folder_paths);
            folder_paths
                .into_iter()
                .zip(counts)
                .map(|(path, c)| FolderCoverage {
                    path,
                    eligible: c.eligible,
                    accounted: c.accounted,
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| format!("folder-coverage task panicked: {e}"))
}
