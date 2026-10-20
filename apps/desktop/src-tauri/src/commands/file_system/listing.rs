//! Tauri commands for directory listing and virtual-scroll API.

use crate::listing_host::listings;
use cmdr_fs::entry::FileEntry;
use cmdr_listing::{
    CompareDirectoriesError, CompareDirectoriesMode, CompareDirectoriesResult, DirectorySortMode, FuzzyJumpError,
    ListingId, ListingLookupError, ListingRequest, ListingStats, NameFilterResult, OnNoMatch, PaneMarks, ResortResult,
    RowBeside, RowSpan, SelectionSnapshot, SortColumn, SortOrder,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::time::Duration;

use crate::file_system::volume::manager::get_volume_manager;
use crate::font_metrics::brief_columns::{
    BriefColumnWidths, BriefColumnsError, BriefColumnsIpcError, compute_brief_column_text_widths,
};
use crate::listing_index_sizes::count::{CountFolderSizesError, FolderSizeCountOutcome};
use cmdr_fs::deadline::{TimedOut, blocking_typed_result_with_timeout, blocking_with_timeout_flag};
use cmdr_fs::volume::WatchCoverage;

use super::expand_tilde;

const TAGS_TIMEOUT: Duration = Duration::from_secs(2);
/// Tag writes are the 5 s "write" tier per `commands/CLAUDE.md`. A `setxattr` on a
/// hung mount can block; the timeout keeps it off the IPC thread (the blocking task
/// runs to completion, but the IPC handler returns).
const TAGS_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Status of a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "status")]
pub enum ListingStatus {
    Loading,
    Ready,
    Cancelled,
    Error { message: String },
}

/// Result of starting a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StreamingListingStartResult {
    pub listing_id: String,
    /// Always `Loading`.
    pub status: ListingStatus,
}

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
        let updates: Vec<(String, Vec<cmdr_fs::entry::TagRef>)> = paths
            .into_iter()
            .map(|p| {
                let tags = crate::file_system::tags::read_tags(Path::new(&p));
                (p, tags)
            })
            .collect();
        listings().apply_tags(&ListingId::new(listing_id), updates);
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
/// then update on the next visible-range enrich). Files whose existing tags couldn't
/// be read are left alone and reported through `tags-skipped`. Off macOS this is a
/// no-op.
#[tauri::command]
#[specta::specta]
pub async fn toggle_tags(app: tauri::AppHandle, listing_id: String, paths: Vec<String>, color: u8) -> TimedOut<()> {
    blocking_with_timeout_flag(TAGS_WRITE_TIMEOUT, (), move || {
        match crate::file_system::tags::toggle_color(&paths, color) {
            Ok(outcome) => {
                crate::file_system::tags::report_skips(&app, &outcome.skipped);
                if !outcome.updates.is_empty() {
                    listings().apply_tags(&ListingId::new(listing_id), outcome.updates);
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

// ============================================================================
// On-demand virtual scrolling API
// ============================================================================

/// Returns immediately; reads in background.
/// Emits listing-progress, listing-complete, listing-error, listing-cancelled.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments, reason = "Tauri commands require top-level arguments")]
pub async fn list_directory_start_streaming(
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
    // A pane on an SMB share macOS mounted, and Cmdr hasn't upgraded yet, tries the
    // direct connection in the background. Returns at once (issue #123).
    crate::network::smb_pane_upgrade::upgrade_on_pane_open(&volume_id);
    // Only expand tilde for local volumes (not MTP)
    let expanded_path = if volume_id == "root" {
        expand_tilde(&path)
    } else {
        path.clone()
    };
    listings().start(ListingRequest {
        id: ListingId::new(listing_id.clone()),
        volume_id,
        path: PathBuf::from(&expanded_path),
        include_hidden,
        sort_by,
        sort_order,
        dir_sort_mode: directory_sort_mode.unwrap_or_default(),
    });
    Ok(StreamingListingStartResult {
        listing_id,
        status: ListingStatus::Loading,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_listing(listing_id: String) {
    listings().cancel(&ListingId::new(listing_id));
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
    expected_sequence: Option<u64>,
) -> Result<ResortResult, ListingLookupError> {
    listings().resort(
        &ListingId::new(listing_id),
        (sort_by, sort_order, directory_sort_mode.unwrap_or_default()),
        include_hidden,
        PaneMarks {
            cursor_filename: cursor_filename.as_deref(),
            selected: selected_indices.as_deref(),
            all_selected: all_selected.unwrap_or(false),
        },
        expected_sequence,
    )
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_range(
    listing_id: String,
    start: usize,
    count: usize,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, ListingLookupError> {
    listings().file_range(&ListingId::new(listing_id), RowSpan { start, count }, include_hidden)
}

/// Returns the widest filename's text-only width (in px) per Brief-mode column.
///
/// Pure read path: takes a snapshot of the listing cache for `listing_id` and
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
            compute_brief_column_text_widths(&listing_id, items_per_column, has_parent, &font_id, include_hidden)
                .map_err(BriefColumnsIpcError::from)
        },
    )
    .await
}

/// Compare directories (⇧F2): which rows each pane should mark against the
/// other. A pure read of the two cached listings; see `listing/compare.rs`.
#[tauri::command]
#[specta::specta]
pub async fn compare_directories(
    left_listing_id: String,
    left_include_hidden: bool,
    right_listing_id: String,
    right_include_hidden: bool,
    mode: CompareDirectoriesMode,
) -> Result<CompareDirectoriesResult, CompareDirectoriesError> {
    // Off the IPC thread: two large listings mean two O(n) passes with a fold per name.
    blocking_typed_result_with_timeout(
        Duration::from_secs(10),
        || CompareDirectoriesError::TimedOut,
        |detail| CompareDirectoriesError::Internal { detail },
        move || {
            listings()
                .compare_directories(
                    (&ListingId::new(left_listing_id), left_include_hidden),
                    (&ListingId::new(right_listing_id), right_include_hidden),
                    mode,
                )
                .map_err(CompareDirectoriesError::from)
        },
    )
    .await
}

/// Calculates the sizes of folders the pane shows (⌥⇧⏎; `paths` for Space on a
/// folder), sending each reading as `listing-index-sizes-changed`. A request
/// while a count of the same listing runs joins its queue. Resolves when the
/// count ends: done, or stopped by [`cancel_folder_size_count`]. See
/// `listing_index_sizes/count/`.
#[tauri::command]
#[specta::specta]
pub async fn count_folder_sizes(
    app: tauri::AppHandle,
    listing_id: String,
    include_hidden: bool,
    paths: Option<Vec<String>>,
) -> Result<FolderSizeCountOutcome, CountFolderSizesError> {
    use tauri_specta::Event;
    let sink = move |event: crate::listing_index_sizes::ListingIndexSizesChanged| {
        let _ = event.emit(&app);
    };
    crate::listing_index_sizes::count::count(&listing_id, include_hidden, paths.as_deref(), &sink).await
}

/// Stops the folder-size count running for `listing_id` (Esc). Reports whether one was running.
#[tauri::command]
#[specta::specta]
pub async fn cancel_folder_size_count(listing_id: String) -> bool {
    crate::listing_index_sizes::count::cancel(&listing_id)
}

#[tauri::command]
#[specta::specta]
pub async fn find_file_index(
    listing_id: String,
    name: String,
    include_hidden: bool,
) -> Result<Option<usize>, ListingLookupError> {
    listings().find_file_index(&ListingId::new(listing_id), &name, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub async fn find_file_indices(
    listing_id: String,
    names: Vec<String>,
    include_hidden: bool,
) -> Result<std::collections::HashMap<String, usize>, ListingLookupError> {
    listings().find_file_indices(&ListingId::new(listing_id), &names, include_hidden)
}

/// Returns the backend index of the highest-scoring fuzzy match for `query` in
/// the cached listing, or `None` when nothing matches. Powers the type-to-jump
/// feature in `FilePane.svelte`. Hidden entries are skipped when `include_hidden`
/// is false. The frontend adjusts for the synthetic `..` parent offset before
/// setting the cursor (the parent entry is never in the listing cache).
#[tauri::command]
#[specta::specta]
pub async fn find_first_fuzzy_match(
    listing_id: String,
    query: String,
    include_hidden: bool,
) -> Result<Option<usize>, FuzzyJumpError> {
    listings().fuzzy_find_first_match(&ListingId::new(listing_id), &query, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_at(
    listing_id: String,
    index: usize,
    include_hidden: bool,
) -> Result<Option<FileEntry>, ListingLookupError> {
    listings().file_at(&ListingId::new(listing_id), index, include_hidden)
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
) -> Result<Option<FileEntry>, ListingLookupError> {
    listings().file_beside(&ListingId::new(listing_id), &name, side, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub async fn list_directory_end(listing_id: String) {
    listings().end(&ListingId::new(listing_id));
}

/// The panes' heartbeat: keeps the named listings safe from the orphan reaper and
/// returns the ids no longer cached, which the frontend re-lists.
/// See `Listings::keep_alive`.
#[tauri::command]
#[specta::specta]
pub async fn keep_listings_alive(listing_ids: Vec<String>) -> Vec<String> {
    let ids: Vec<ListingId> = listing_ids.into_iter().map(ListingId::new).collect();
    listings()
        .keep_alive(&ids)
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect()
}

/// Tells the backend the pane showing `listing_id` now shows (or hides) hidden
/// files. Its `directory-diff` events speak that pane's rows, and skip changes
/// to rows it doesn't show, so the pane calls this before re-reading its rows
/// after the hidden-files toggle.
#[tauri::command]
#[specta::specta]
pub async fn set_listing_include_hidden(
    listing_id: String,
    include_hidden: bool,
    expected_sequence: Option<u64>,
    cursor_filename: Option<String>,
    selected_indices: Option<Vec<usize>>,
    all_selected: Option<bool>,
) -> Result<ResortResult, ListingLookupError> {
    listings().set_include_hidden(
        &ListingId::new(listing_id),
        include_hidden,
        expected_sequence,
        PaneMarks {
            cursor_filename: cursor_filename.as_deref(),
            selected: selected_indices.as_deref(),
            all_selected: all_selected.unwrap_or(false),
        },
    )
}

/// Consume a selection only while it still names the committed backend rows.
#[tauri::command]
#[specta::specta]
pub async fn get_selection_snapshot(
    listing_id: String,
    include_hidden: bool,
    selected_indices: Vec<usize>,
    expected_sequence: u64,
) -> Result<SelectionSnapshot, ListingLookupError> {
    listings().selection_snapshot(
        &ListingId::new(listing_id),
        include_hidden,
        &selected_indices,
        expected_sequence,
    )
}

/// The full entries on the selected rows, only while they still name the
/// committed backend rows: the delete dialog's input.
#[tauri::command]
#[specta::specta]
pub async fn get_selection_entries(
    listing_id: String,
    include_hidden: bool,
    selected_indices: Vec<usize>,
    expected_sequence: u64,
) -> Result<Vec<FileEntry>, ListingLookupError> {
    listings().selection_entries(
        &ListingId::new(listing_id),
        include_hidden,
        &selected_indices,
        expected_sequence,
    )
}

/// Sets the quick filter of the pane showing `listing_id` (an empty or `null`
/// pattern clears it), and returns the new row count plus where the cursor's
/// file and the selected files landed in the filtered rows. With
/// `refuse_empty`, a pattern that matches nothing is refused (`accepted: false`).
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments, reason = "Tauri commands require top-level arguments")]
pub async fn set_listing_name_filter(
    listing_id: String,
    pattern: Option<String>,
    include_hidden: bool,
    cursor_filename: Option<String>,
    selected_indices: Vec<usize>,
    refuse_empty: bool,
    expected_sequence: Option<u64>,
) -> Result<NameFilterResult, ListingLookupError> {
    listings().set_name_filter_guarded(
        &ListingId::new(listing_id),
        pattern.as_deref(),
        include_hidden,
        PaneMarks {
            cursor_filename: cursor_filename.as_deref(),
            selected: Some(&selected_indices),
            all_selected: false,
        },
        if refuse_empty {
            OnNoMatch::Refuse
        } else {
            OnNoMatch::Apply
        },
        expected_sequence,
    )
}

/// The listing's path when a non-local volume's own watcher claims to see every
/// writer of it, so an unforced [`refresh_listing`] can skip the re-read. `None`
/// whenever the re-read has to happen: local volume, unregistered volume, no
/// cache entry, or a watcher that admits it misses writes.
async fn watcher_backed_listing_path(listing_id: &str) -> Option<PathBuf> {
    let (volume_id, path) = listings().location(&ListingId::new(listing_id))?;
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
/// `Removed` / `Modified` events patched into the listing cache after every
/// successful mutation), so a full `list_directory` re-read is pure redundancy
/// and costs a lot on slow backends: a 1k-entry MTP folder takes ~17 s and holds
/// the USB session, colliding with the user's next op. A listing inside a local
/// zip qualifies too (its content watch claims `EveryWriter`); there the edit
/// driver re-reads it before the op completes (`archive_edit::engine::show_the_edit`).
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
            cmdr_fs::log_private::LogPrivate::dir_path(&path),
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
) -> Result<ListingStats, ListingLookupError> {
    listings().stats(&ListingId::new(listing_id), include_hidden, selected_indices.as_deref())
}

/// Re-enriches cached listing entries with fresh drive index data.
///
/// On the blocking pool rather than inline: this one runs two indexed SQLite
/// queries, and a whole-volume index update (a scan finishing) runs it for every
/// open listing. An async worker held for the length of a database query starves every
/// other future scheduled on it, which is the same shape of problem as the main
/// thread, one layer down.
#[tauri::command]
#[specta::specta]
pub async fn refresh_listing_index_sizes(listing_id: String) -> Result<(), String> {
    match tokio::task::spawn_blocking(move || listings().refresh_index_sizes(&ListingId::new(listing_id))).await {
        Ok(()) => Ok(()),
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
    if cmdr_fs::benchmark::is_enabled() {
        eprintln!("{}", message);
    }
}

#[cfg(test)]
#[path = "refresh_listing_test.rs"]
mod refresh_listing_test;
