//! Tauri commands for directory listing and virtual-scroll API.

use crate::file_system::get_files_at_indices as ops_get_files_at_indices;
use crate::file_system::get_paths_at_indices as ops_get_paths_at_indices;
use crate::file_system::{
    BriefColumnWidths, BriefColumnsIpcError, DirectorySortMode, FileEntry, ListingStartResult, ListingStats,
    ResortResult, RowBeside, SortColumn, SortOrder, StreamingListingStartResult, cancel_listing as ops_cancel_listing,
    compute_brief_column_text_widths as ops_compute_brief_column_text_widths, find_file_index as ops_find_file_index,
    find_file_indices as ops_find_file_indices,
    fuzzy_find_first_match_in_listing as ops_fuzzy_find_first_match_in_listing, get_file_at as ops_get_file_at,
    get_file_beside as ops_get_file_beside, get_file_range as ops_get_file_range,
    get_listing_stats as ops_get_listing_stats, get_total_count as ops_get_total_count,
    list_directory_end as ops_list_directory_end, list_directory_start_streaming as ops_list_directory_start_streaming,
    list_directory_start_with_volume as ops_list_directory_start_with_volume,
    refresh_listing_index_sizes as ops_refresh_listing_index_sizes, resort_listing as ops_resort_listing,
};
use std::path::{Path, PathBuf};
use tokio::time::Duration;

use crate::commands::util::{TimedOut, blocking_typed_result_with_timeout, blocking_with_timeout_flag};
use crate::file_system::listing::brief_columns::BriefColumnsError;
use crate::file_system::listing::fuzzy_jump::FuzzyJumpError;
use crate::file_system::validation::{MAX_NAME_BYTES, MAX_PATH_BYTES};
use crate::file_system::volume::manager::get_volume_manager;
use cmdr_fs::volume::WatchCoverage;

use super::expand_tilde;

const PATH_EXISTS_TIMEOUT: Duration = Duration::from_secs(2);
const TAGS_TIMEOUT: Duration = Duration::from_secs(2);
/// Tag writes are the 5 s "write" tier per `commands/CLAUDE.md`. A `setxattr` on a
/// hung mount can block; the timeout keeps it off the IPC thread (the blocking task
/// runs to completion, but the IPC handler returns).
const TAGS_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Reads macOS Finder tags for the given paths and patches them into the cached
/// listing, emitting a coalesced `directory-diff` so the panes show the colored
/// dots. The frontend calls this for the VISIBLE range (and a background sweep
/// backfills the rest): a `getxattr` per path (~15 µs) is too costly to run
/// inline over a 100k-directory listing, so tag loading is deferred off the hot
/// path. Safe on any volume — a `getxattr` on a non-local or tagless path simply
/// yields no tags — and timeout-guarded so a hung mount can't stall the IPC thread.
#[tauri::command]
#[specta::specta]
pub async fn enrich_tags(listing_id: String, paths: Vec<String>) -> TimedOut<()> {
    blocking_with_timeout_flag(TAGS_TIMEOUT, (), move || {
        let updates: Vec<(String, Vec<crate::file_system::listing::metadata::TagRef>)> = paths
            .into_iter()
            .map(|p| {
                let tags = crate::file_system::tags::read_tags(Path::new(&p));
                (p, tags)
            })
            .collect();
        crate::file_system::listing::caching::apply_tags_to_listing(&listing_id, updates);
    })
    .await
}

/// Toggles a Finder color tag (`color` 1..=7) across `paths`, then patches the
/// resulting tags into the cached listing so the panes re-render immediately.
///
/// Read-modify-write that PRESERVES every other tag on each file (see
/// `tags::toggle_color` for the multi-file all-have/some-have semantics). The
/// frontend supplies `listing_id` so the cache refresh targets the right pane; an
/// empty / unknown id still writes to disk but skips the in-place refresh (the dots
/// then update on the next visible-range enrich). Off macOS this is a no-op.
#[tauri::command]
#[specta::specta]
pub async fn toggle_tags(listing_id: String, paths: Vec<String>, color: u8) -> TimedOut<()> {
    blocking_with_timeout_flag(TAGS_WRITE_TIMEOUT, (), move || {
        match crate::file_system::tags::toggle_color(&paths, color) {
            Ok(updates) => {
                if !updates.is_empty() {
                    crate::file_system::listing::caching::apply_tags_to_listing(&listing_id, updates);
                }
            }
            // A write failure (permission, dead mount) is logged, not surfaced as a
            // hard error: tags are low-stakes, each `setxattr` is atomic, and the
            // panes stay correct (they reflect what's actually on disk on next read).
            Err(e) => {
                log::warn!(target: "tags", "toggle_tags failed (color={color}): {e}");
            }
        }
    })
    .await
}

#[derive(serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PathLimits {
    pub max_name_bytes: usize,
    pub max_path_bytes: usize,
}

#[tauri::command]
#[specta::specta]
pub fn get_path_limits() -> PathLimits {
    PathLimits {
        max_name_bytes: MAX_NAME_BYTES,
        max_path_bytes: MAX_PATH_BYTES,
    }
}

/// Returns `TimedOut<bool>` so the frontend can distinguish a real "doesn't exist"
/// from "we couldn't tell" (timeout, or SMB volume in `Disconnected` state). Without this
/// distinction, the directory-eviction poll in `FilePane.svelte` evicts users from a
/// network folder on any transient connection blip.
#[tauri::command]
#[specta::specta]
pub async fn path_exists(volume_id: Option<String>, path: String) -> TimedOut<bool> {
    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" { expand_tilde(&path) } else { path };

    // Resolve so an archive-inner path checks existence inside the `.zip`.
    if let Some(volume) = get_volume_manager()
        .resolve(&volume_id, Path::new(&expanded_path))
        .await
        .volume
    {
        // For SMB volumes, an immediate `false` from `exists()` may be the connection
        // being dead (`clone_session` returns `Err`) rather than the path actually missing.
        // Snapshot whether this is an SMB volume by whether it reports an SMB connection state.
        let is_smb = volume.smb_connection_state().is_some();

        // The transfer dialog asks about its VOLUME-RELATIVE destination box
        // (`/photos`), the panes about absolute paths. Anchoring folds both into
        // what the volume accepts; without it a share answers "doesn't exist"
        // for every subfolder, and the dialog promises to create one that's
        // already there.
        let path_for_check = cmdr_fs::volume::root_anchored(volume.root(), Path::new(&expanded_path));
        match tokio::time::timeout(PATH_EXISTS_TIMEOUT, volume.exists(&path_for_check)).await {
            Ok(exists) => {
                // SMB volume just transitioned to `Disconnected`? The `false` we got back
                // is meaningless. Surface it as a timeout-equivalent so callers know.
                if !exists && is_smb && volume.smb_connection_state().is_none() {
                    return TimedOut {
                        data: false,
                        timed_out: true,
                    };
                }
                TimedOut {
                    data: exists,
                    timed_out: false,
                }
            }
            Err(_) => TimedOut {
                data: false,
                timed_out: true,
            },
        }
    } else {
        // Fallback for unknown volumes (shouldn't happen in practice)
        let path_buf = PathBuf::from(expanded_path);
        let result = tokio::time::timeout(
            PATH_EXISTS_TIMEOUT,
            tokio::task::spawn_blocking(move || path_buf.exists()),
        )
        .await;
        match result {
            Ok(Ok(exists)) => TimedOut {
                data: exists,
                timed_out: false,
            },
            _ => TimedOut {
                data: false,
                timed_out: true,
            },
        }
    }
}

// ============================================================================
// On-demand virtual scrolling API
// ============================================================================

/// Synchronous version. Prefer `list_directory_start_streaming` for non-blocking operation.
#[tauri::command]
#[specta::specta]
pub async fn list_directory_start(
    path: String,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: Option<DirectorySortMode>,
) -> Result<ListingStartResult, ListingStartError> {
    // Foreground activity: the user navigated. This command is the local-volume
    // path, so attribute it to "root" — the same volume id the FE uses for local.
    // Background work yields to this: media enrichment (app-wide), and the local
    // volume's own index scan and transfers (per-volume).
    crate::priority::foreground::note_foreground_activity_on("root");
    let expanded_path = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded_path);
    let dir_sort_mode = directory_sort_mode.unwrap_or_default();
    match tokio::time::timeout(
        Duration::from_secs(2),
        ops_list_directory_start_with_volume("root", &path_buf, include_hidden, sort_by, sort_order, dir_sort_mode),
    )
    .await
    {
        Ok(Ok(result)) => Ok(result),
        // `VolumeError` carries the errno AND the path, which is what the
        // frontend's listing-error factory renders from; a formatted sentence
        // would throw both away.
        Ok(Err(e)) => Err(ListingStartError::Volume {
            error: cmdr_fs::volume::VolumeError::from_io_at(&e, &path_buf),
        }),
        Err(_) => Err(ListingStartError::TimedOut),
    }
}

/// Why a synchronous listing start didn't produce a listing.
///
/// ❌ Not prose: `VolumeError` is the wire type the frontend's listing-error
/// factory already words, in every locale.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ListingStartError {
    /// The volume refused, and said why in its own vocabulary.
    Volume {
        /// The backend's typed answer, errno and path intact.
        error: cmdr_fs::volume::VolumeError,
    },
    /// The read didn't finish inside the command's wait. ❗ It was NOT
    /// cancelled.
    TimedOut,
}

/// Returns immediately; reads in background.
/// Emits listing-progress, listing-complete, listing-error, listing-cancelled.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments, reason = "Tauri commands require top-level arguments")]
pub async fn list_directory_start_streaming(
    app: tauri::AppHandle,
    volume_id: String,
    path: String,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: Option<DirectorySortMode>,
    listing_id: String,
) -> Result<StreamingListingStartResult, String> {
    // Foreground activity: the user navigated THIS volume. Attributing it is what
    // lets the NAS index scan and SMB transfers back off for the share the user is
    // actually browsing, without a local navigation slowing an unrelated share.
    crate::priority::foreground::note_foreground_activity_on(&volume_id);
    // Only expand tilde for local volumes (not MTP)
    let expanded_path = if volume_id == "root" {
        expand_tilde(&path)
    } else {
        path.clone()
    };
    let path_buf = PathBuf::from(&expanded_path);
    let dir_sort_mode = directory_sort_mode.unwrap_or_default();
    ops_list_directory_start_streaming(
        app,
        &volume_id,
        &path_buf,
        include_hidden,
        sort_by,
        sort_order,
        dir_sort_mode,
        listing_id,
    )
    .await
    .map_err(|e| format!("Failed to start directory listing '{}': {}", path, e))
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_listing(listing_id: String) {
    ops_cancel_listing(&listing_id);
}

#[allow(clippy::too_many_arguments, reason = "Tauri commands require top-level arguments")]
#[tauri::command]
#[specta::specta]
pub async fn resort_listing(
    listing_id: String,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: Option<DirectorySortMode>,
    cursor_filename: Option<String>,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
    all_selected: Option<bool>,
) -> Result<ResortResult, String> {
    ops_resort_listing(
        &listing_id,
        sort_by,
        sort_order,
        directory_sort_mode.unwrap_or_default(),
        cursor_filename.as_deref(),
        include_hidden,
        selected_indices.as_deref(),
        all_selected.unwrap_or(false),
    )
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_range(
    listing_id: String,
    start: usize,
    count: usize,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    ops_get_file_range(&listing_id, start, count, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub async fn get_total_count(listing_id: String, include_hidden: bool) -> Result<usize, String> {
    ops_get_total_count(&listing_id, include_hidden)
}

/// Returns the widest filename's text-only width (in px) per Brief-mode column.
///
/// Pure read path: takes a snapshot of `LISTING_CACHE` for `listing_id` and
/// measures each column's widest filename with `font_metrics::calculate_max_width_with_suffixes`.
/// The FE applies chrome + clamp on top.
///
/// Answers without waiting on measurement. A filename containing a code point
/// the font cache hasn't measured is costed at the average width and that code
/// point comes back in `missingCodePoints`; the FE measures those, calls
/// `extend_font_metrics`, and re-queries for exact widths.
///
/// Failures arrive as a typed `BriefColumnsIpcError { kind, message }`. The FE
/// branches on `kind` alone (`fontMetricsNotReady` drives a measure-and-retry,
/// `listingNotFound` / `timeout` / `other` a bounded backoff retry,
/// `invalidItemsPerColumn` an immediate give-up); `message` is log text only.
#[tauri::command]
#[specta::specta]
pub async fn get_brief_column_text_widths(
    listing_id: String,
    items_per_column: usize,
    has_parent: bool,
    font_id: String,
    include_hidden: bool,
) -> Result<BriefColumnWidths, BriefColumnsIpcError> {
    blocking_typed_result_with_timeout(
        Duration::from_secs(2),
        BriefColumnsIpcError::timeout,
        |detail| BriefColumnsIpcError::from(BriefColumnsError::Other(detail)),
        move || {
            ops_compute_brief_column_text_widths(&listing_id, items_per_column, has_parent, &font_id, include_hidden)
                .map_err(BriefColumnsIpcError::from)
        },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn find_file_index(listing_id: String, name: String, include_hidden: bool) -> Result<Option<usize>, String> {
    ops_find_file_index(&listing_id, &name, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub async fn find_file_indices(
    listing_id: String,
    names: Vec<String>,
    include_hidden: bool,
) -> Result<std::collections::HashMap<String, usize>, String> {
    ops_find_file_indices(&listing_id, &names, include_hidden)
}

/// Returns the backend index of the highest-scoring fuzzy match for `query` in
/// the cached listing, or `None` when nothing matches. Powers the type-to-jump
/// feature in `FilePane.svelte`. Hidden entries are skipped when `include_hidden`
/// is false. The frontend adjusts for the synthetic `..` parent offset before
/// setting the cursor (the parent entry is never in `LISTING_CACHE`).
#[tauri::command]
#[specta::specta]
pub async fn find_first_fuzzy_match(
    listing_id: String,
    query: String,
    include_hidden: bool,
) -> Result<Option<usize>, FuzzyJumpError> {
    ops_fuzzy_find_first_match_in_listing(&listing_id, &query, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_at(listing_id: String, index: usize, include_hidden: bool) -> Result<Option<FileEntry>, String> {
    ops_get_file_at(&listing_id, index, include_hidden)
}

/// The entry immediately before or after the one named `name`, in one call.
///
/// For a caller holding a row it knows rather than an index it trusts: a chained
/// rename asks for the row beside the file its editor is open on, because a
/// re-sort moves every index it could have kept while the name stays the name.
#[tauri::command]
#[specta::specta]
pub async fn get_file_beside(
    listing_id: String,
    name: String,
    side: RowBeside,
    include_hidden: bool,
) -> Result<Option<FileEntry>, String> {
    ops_get_file_beside(&listing_id, &name, side, include_hidden)
}

/// Gets file paths at specific frontend indices from a cached listing (batch version of path
/// extraction). Handles the parent ".." offset internally; callers pass frontend indices.
#[tauri::command]
#[specta::specta]
pub async fn get_paths_at_indices(
    listing_id: String,
    selected_indices: Vec<usize>,
    include_hidden: bool,
    has_parent: bool,
) -> Result<Vec<String>, String> {
    ops_get_paths_at_indices(&listing_id, &selected_indices, include_hidden, has_parent)
        .map(|paths| paths.into_iter().map(|p| p.to_string_lossy().into_owned()).collect())
}

/// Gets full FileEntry objects at specific backend indices from a cached listing.
/// Callers are responsible for any parent offset adjustment before passing indices.
#[tauri::command]
#[specta::specta]
pub async fn get_files_at_indices(
    listing_id: String,
    selected_indices: Vec<usize>,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    ops_get_files_at_indices(&listing_id, &selected_indices, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub async fn list_directory_end(listing_id: String) {
    ops_list_directory_end(&listing_id);
}

/// The listing's path when a non-local volume's own watcher claims to see every
/// writer of it, so an unforced [`refresh_listing`] can skip the re-read. `None`
/// whenever the re-read has to happen: local volume, unregistered volume, no
/// cache entry, or a watcher that admits it misses writes.
async fn watcher_backed_listing_path(listing_id: &str) -> Option<PathBuf> {
    let (volume_id, path) = crate::file_system::listing::get_listing_volume_id_and_path(listing_id)?;
    let volume = get_volume_manager().resolve(&volume_id, &path).await.volume?;
    let watcher_sees_every_writer =
        volume.local_path().is_none() && volume.listing_watch_coverage(&path) == WatchCoverage::EveryWriter;
    watcher_sees_every_writer.then_some(path)
}

/// Re-reads a directory listing, emitting any diff.
///
/// `force` says whose idea the refresh was. `true` is an explicit "re-read this
/// now" from the user (⌘R) or an agent (the MCP `refresh` tool), and always
/// re-reads. `false` is a background top-up after a write op (mkdir, rename,
/// move), where a re-read is only worth paying for when nothing else keeps the
/// cache fresh.
///
/// So `force: false` short-circuits when the listing lives on a **non-local**
/// volume that reports [`WatchCoverage::EveryWriter`]. There the cache is being
/// kept fresh by the volume's `notify_mutation` pipeline (per-file `Added` /
/// `Removed` / `Modified` events patched into `LISTING_CACHE` after every
/// successful mutation), so a full `list_directory` re-read is pure redundancy
/// and costs a lot on slow backends: a 1k-entry MTP folder takes ~17 s and holds
/// the USB session, colliding with the user's next op.
///
/// `force: true` skips that check, because `EveryWriter` is a claim about the
/// volume's OWN writes, not about everyone's: SMB's watcher misses writes made
/// from another machine, so answering a user's refresh out of the cache would be
/// a lie. The cost is bounded by how rarely a person presses ⌘R, which is the
/// whole reason the write-op callers stay unforced.
///
/// Local volumes always re-read either way. FSEvents on macOS races with
/// `/tmp` ↔ `/private/tmp` symlink resolution and with the fixture-recreate
/// beforeEach loops we run in E2E, so the cache is not reliably fresh at the
/// moment `refresh_listing` lands — and a local `list_directory` is
/// sub-millisecond, so paying for a re-read is the right trade.
///
/// Returns `TimedOut { data: (), timed_out: false }` immediately when the
/// short-circuit fires, matching the `timed_out: false` shape the FE already
/// handles on the fast-path.
///
/// Note: only this command is gated. The FSEvents/SMB/MTP watcher callbacks call
/// `handle_directory_change` directly and are intentionally left alone — they're
/// how the cache stays in sync in the first place.
#[tauri::command]
#[specta::specta]
pub async fn refresh_listing(listing_id: String, force: bool) -> TimedOut<()> {
    refresh_listing_within(listing_id, force, REFRESH_WAIT).await
}

/// How long [`refresh_listing`] waits for the re-read before answering
/// `timed_out: true`. The re-read runs on regardless; this is only how long the
/// caller blocks.
const REFRESH_WAIT: Duration = Duration::from_secs(2);

/// [`refresh_listing`] with the wait spelled out, so tests don't spend it.
async fn refresh_listing_within(listing_id: String, force: bool, wait: Duration) -> TimedOut<()> {
    // Forcing skips the volume lookup as well as the short-circuit: the answer
    // can't change the outcome, and resolving reaches out to the volume manager.
    if !force && let Some(path) = watcher_backed_listing_path(&listing_id).await {
        log::debug!(
            target: "refresh_listing",
            "refresh_listing: short-circuit, watcher-backed non-local listing (listing_id={}, path={})",
            listing_id,
            path.display(),
        );
        return TimedOut {
            data: (),
            timed_out: false,
        };
    }

    // Spawned, not awaited in place: the wait must never DROP the re-read. A
    // dropped `list_directory` abandons an MTP read mid-PTP-transaction (which
    // wedges the phone until it's replugged) and throws away the very re-read the
    // caller asked for. Dropping a `JoinHandle` only detaches the task, so a slow
    // read runs to completion and emits its diff whenever it lands. The wait
    // decides how long the CALLER blocks, nothing else.
    let reread = tokio::spawn(async move {
        crate::file_system::watcher::handle_directory_change(&listing_id).await;
    });
    let timed_out = tokio::time::timeout(wait, reread).await.is_err();
    TimedOut { data: (), timed_out }
}

/// Returns total file/dir counts and sizes, plus selection stats if `selected_indices` is given.
#[tauri::command]
#[specta::specta]
pub async fn get_listing_stats(
    listing_id: String,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
) -> Result<ListingStats, String> {
    ops_get_listing_stats(&listing_id, include_hidden, selected_indices.as_deref())
}

/// Re-enriches cached listing entries with fresh drive index data.
///
/// On the blocking pool rather than inline: this one runs two indexed SQLite
/// queries, and an index storm fires it once per `index-dir-updated` event per
/// pane. An async worker held for the length of a database query starves every
/// other future scheduled on it, which is the same shape of problem as the main
/// thread, one layer down.
#[tauri::command]
#[specta::specta]
pub async fn refresh_listing_index_sizes(listing_id: String) -> Result<(), String> {
    match tokio::task::spawn_blocking(move || ops_refresh_listing_index_sizes(&listing_id)).await {
        Ok(result) => result,
        // A `JoinError` means the blocking task panicked, so no answer is coming.
        Err(join_error) => Err(join_error.to_string()),
    }
}

// ============================================================================
// Benchmarking support
// ============================================================================

/// Logs a frontend benchmark event to stderr (unified timeline with Rust events).
/// Only logs if RUSTY_COMMANDER_BENCHMARK=1 is set.
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::print_stderr,
    reason = "Benchmark output intentionally bypasses log framework"
)]
pub fn benchmark_log(message: String) {
    if crate::benchmark::is_enabled() {
        eprintln!("{}", message);
    }
}

#[cfg(test)]
#[path = "refresh_listing_test.rs"]
mod refresh_listing_test;
