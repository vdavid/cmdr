//! IPC commands for drive search.
//!
//! Thin wrappers around `search` module functions, exposed to the frontend via Tauri commands.

use std::sync::atomic::Ordering;

use serde::Serialize;

use genai::chat::ChatOptions;

use crate::ai::AiTranslateError;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder, SortableEntry, entry_comparator};
use crate::search::{self, ParsedScope, SearchQuery, SearchResult, VolumeLoad};

use crate::search::ai::{self, query_builder as ai_query_builder};
use crate::search::history::{DEFAULT_MAX_COUNT, HistoryEntry, RECENT_SEARCHES};

/// The translation DTOs are defined in `search::ai::types` (business logic owns
/// its own data) and re-exported here so the IPC surface stays at this path.
pub use crate::search::ai::types::{TranslateDisplay, TranslatedQuery};

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PrepareResult {
    pub ready: bool,
    pub entry_count: u64,
    /// Whether a background load is in flight, so a `search-index-ready` naming this
    /// volume is coming. `false` alongside `ready: false` is the terminal answer
    /// "there is no index to load here": the dialog must NOT wait on it, or a machine
    /// that declined indexing never searches at all.
    pub loading: bool,
}

/// Emitted once a volume's in-memory search index finishes loading, so the dialog
/// can flip from "loading" to ready, show the indexed entry count, and re-run
/// whatever the user has typed.
///
/// The event NAMES its volume rather than implying root: a search targets one volume
/// (`search/execute.rs`), so "ready" is only ever true of a particular one. Today
/// only root's dialog pre-load emits; the frontend's readiness gate is per target and
/// consumes `volumeId` (`src/lib/search/coverage-note.ts::isTargetIndexReady`).
#[derive(Debug, Clone, serde::Deserialize, Serialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "search-index-ready")]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexReadyEvent {
    /// The volume whose arena just landed.
    pub volume_id: String,
    /// That volume's indexed entry count.
    pub entry_count: u64,
}

/// Called when the search dialog opens. Pre-loads the ROOT index in the background
/// (the common case; scoped volumes load lazily on their first query). Returns
/// immediately with `{ ready, entryCount, loading }`; the dialog flips to ready on the
/// emitted `search-index-ready` event when `loading` says one is coming.
///
/// `loading: false` with `ready: false` means root has no index to load, so no event
/// will follow. Saying so is what lets a search still run on a machine that declined
/// indexing: the dialog stops waiting and asks the question, and the answer comes back
/// with its coverage gap named.
#[tauri::command]
#[specta::specta]
pub async fn prepare_search_index(app: tauri::AppHandle) -> Result<PrepareResult, String> {
    use cmdr_index::ROOT_VOLUME_ID;

    search::touch_activity();
    search::DIALOG_OPEN.store(true, Ordering::Relaxed);
    search::cancel_idle_timer();

    // Fast path: root already warm and fresh.
    if let Some(v) = search::get_loaded(ROOT_VOLUME_ID) {
        // A prior session's backstop timer may still be ticking; reset it so it
        // can't fire while the dialog is open.
        search::reset_backstop_timer();
        return Ok(PrepareResult {
            ready: true,
            entry_count: v.index.entries.len() as u64,
            loading: false,
        });
    }

    // Nothing to load: indexing declined, or the first scan hasn't produced a
    // searchable index yet. Say so instead of spawning a load whose event never
    // arrives, which would leave the dialog waiting on an index that isn't coming.
    if !search::has_searchable_index(ROOT_VOLUME_ID) {
        return Ok(PrepareResult {
            ready: false,
            entry_count: 0,
            loading: false,
        });
    }

    // Load root in the background so the dialog doesn't block on a multi-second scan.
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        match tokio::task::spawn_blocking(|| search::ensure_volume(ROOT_VOLUME_ID)).await {
            Ok(VolumeLoad::Loaded(v)) => emit_index_ready(&app_clone, ROOT_VOLUME_ID, v.index.entries.len() as u64),
            Ok(VolumeLoad::NotIndexed) => log::debug!("prepare_search_index: root index not available yet"),
            Ok(VolumeLoad::Failed(e)) => log::warn!("prepare_search_index: root load failed: {e}"),
            Err(e) => log::warn!("prepare_search_index: load task panicked: {e}"),
        }
    });

    Ok(PrepareResult {
        ready: false,
        entry_count: 0,
        loading: true,
    })
}

/// Search the scope's volume, or the boot volume when the query has no scope.
/// Returns empty (with an honest coverage gap) when that volume has no index yet.
///
/// A search covers at most ONE volume, so a scope naming paths on two of them is
/// refused rather than answered for one (`search/execute.rs`).
#[tauri::command]
#[specta::specta]
pub async fn search_files(query: SearchQuery) -> Result<SearchResult, String> {
    search::touch_activity();
    search::cancel_idle_timer();

    // Route + load + scan on a blocking thread (opens a DB, rayon scan).
    tokio::task::spawn_blocking(move || search::run_blocking(query))
        .await
        .map_err(|e| format!("Search task failed: {e}"))?
}

/// Search the scope's volume, walking whatever its index can't answer for yet.
///
/// Returns as soon as routing has picked a volume; everything else arrives as
/// `search-progress` / `search-complete` / `search-cancelled` / `search-error`
/// events stamped with `run_id`. Starting a run supersedes the previous one (its
/// events stop, its walk carries on), and `cancel_search` stops one outright.
///
/// `run_id` comes from the caller so no event can arrive against an id the
/// frontend hasn't seen yet, exactly as `listing_id` does for a streaming
/// listing.
#[tauri::command]
#[specta::specta]
pub async fn search_files_streaming(
    app: tauri::AppHandle,
    query: SearchQuery,
    run_id: String,
    order: u64,
) -> Result<search::LiveSearchStart, String> {
    search::start_live(app, query, run_id, order)
}

/// Stop a live search and the walk behind it. Returns whether there was one.
#[tauri::command]
#[specta::specta]
pub async fn cancel_search(run_id: String) -> Result<bool, String> {
    Ok(search::cancel_live_run(&run_id))
}

/// Nudge the dialog that a volume's index is now searchable, naming the volume and
/// its entry count.
fn emit_index_ready(app: &tauri::AppHandle, volume_id: &str, entry_count: u64) {
    use tauri_specta::Event;

    let _ = SearchIndexReadyEvent {
        volume_id: volume_id.to_string(),
        entry_count,
    }
    .emit(app);
}

/// Called when the search dialog closes. Starts the idle timer, cancels any
/// in-progress index load, and stops every live search of the DIALOG's but the
/// one the caller asked to keep. An MCP call's run carries on: nobody watching
/// the dialog says nothing about an agent waiting on its own answer.
///
/// A walk outlives its dialog only through "Open in pane"
/// (the handoff, `src/lib/search/walk-handoff.svelte.ts`), which is what `keep_run_id` names:
/// those results are on screen in a pane and still growing. Closing the dialog
/// otherwise means nobody is waiting. What a stopped walk already read stays in
/// the index, so the next search over that ground starts from where it stopped.
#[tauri::command]
#[specta::specta]
pub async fn release_search_index(keep_run_id: Option<String>) -> Result<(), String> {
    search::DIALOG_OPEN.store(false, Ordering::Relaxed);
    search::cancel_dialog_runs_except(keep_run_id.as_deref());
    search::cancel_active_loads();
    search::start_idle_timer();
    Ok(())
}

/// Parse a scope string into structured include/exclude data.
#[tauri::command]
#[specta::specta]
pub fn parse_search_scope(scope: String) -> ParsedScope {
    search::parse_scope(&scope)
}

/// Returns the list of system/build/cache directory names excluded by default,
/// for display in the UI tooltip.
#[tauri::command]
#[specta::specta]
pub fn get_system_dir_excludes() -> &'static [&'static str] {
    search::SYSTEM_DIR_EXCLUDES
}

// ============================================================================
// AI search query translation
// ============================================================================

/// Human-readable field values returned alongside the structured query.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    pub query: TranslatedQuery,
    pub display: TranslateDisplay,
    pub caveat: Option<String>,
    /// Short, breadcrumb-friendly title for this search (max 40 chars, sentence
    /// case). The LLM produces it; the frontend stores it on the snapshot and
    /// renders it in the search-results pane breadcrumb. `None` when the LLM
    /// omitted the label or the fallback path ran (raw-keywords retry); the
    /// frontend falls back to the original natural-language prompt.
    pub label: Option<String>,
}

/// Translates a natural language search query into structured filters using the configured LLM.
///
/// Single-pass flow: call LLM with classification prompt → parse key-value response →
/// build deterministic SearchQuery via `ai_query_builder`.
/// `current_type` is the dialog's `Both | Files | Folders` toggle as context (`Some(true)` =
/// folders, `Some(false)` = files, `None` = both). The model maps it to the `folders: yes|no`
/// field, or omits the field to keep the user's current choice. First step toward the
/// "agent sees app state" model; structured to grow into the full filter set later.
#[tauri::command]
#[specta::specta]
pub async fn translate_search_query(
    natural_query: String,
    current_type: Option<bool>,
) -> Result<TranslateResult, AiTranslateError> {
    let backend = crate::ai::manager::resolve_translate_backend(false)?
        .with_log_context(crate::ai::llm_log::LlmLogContext::translate_search());
    let system_prompt = ai::build_classification_prompt(current_type);

    log::debug!(
        "AI search: classification prompt ({} chars), query={natural_query:?}",
        system_prompt.len()
    );

    // 300 tokens (not 200): reasoning models spend the budget thinking before any visible
    // answer, so a tight cap returns an empty response. See ai/CLAUDE.md § reasoning-model
    // token budget.
    let options = ChatOptions::default()
        .with_temperature(0.3)
        .with_max_tokens(300)
        .with_top_p(0.9);

    let response =
        crate::ai::translate::translate_once(&backend, &system_prompt, &natural_query, &options, "AI search").await?;

    // Parse key-value response
    let parsed = ai::parse_llm_response(&response);

    // Fallback: if parser returned nothing useful, use raw query keywords
    let parsed = if parsed.is_empty() {
        log::info!("AI search: LLM returned empty/garbage response, falling back to raw keywords");
        let fallback_kw = ai::fallback_keywords(&natural_query);
        if fallback_kw.is_empty() {
            parsed
        } else {
            ai::ParsedLlmResponse {
                keywords: Some(fallback_kw),
                ..Default::default()
            }
        }
    } else {
        parsed
    };

    // Build deterministic query
    let query = ai_query_builder::build_search_query(&parsed);
    let display = ai_query_builder::build_translate_display(&parsed, &query);
    let caveat = ai_query_builder::generate_caveat(&parsed, &query);
    let label = ai_query_builder::build_label(&parsed);
    let translated_query = ai_query_builder::build_translated_query(&query);

    Ok(TranslateResult {
        query: translated_query,
        display,
        caveat,
        label,
    })
}

// ============================================================================
// Recent searches (history) IPC
// ============================================================================

/// Reads the latest persisted recent-searches entries. `limit = None` returns all.
#[tauri::command]
#[specta::specta]
pub fn get_recent_searches(limit: Option<u32>) -> Vec<HistoryEntry> {
    RECENT_SEARCHES.entries(limit.map(|n| n as usize))
}

/// Adds a recent-search entry. Dedupes against existing entries by canonical key,
/// moves the matching one to the top, and trims to `max_count`.
#[tauri::command]
#[specta::specta]
pub fn add_recent_search(app: tauri::AppHandle, entry: HistoryEntry, max_count: Option<u32>) -> Result<(), String> {
    let cap = max_count.map(|n| n as usize).unwrap_or(DEFAULT_MAX_COUNT);
    RECENT_SEARCHES.add(&app, entry, cap);
    Ok(())
}

/// Removes a recent-search entry by id. No-op when the id isn't present.
#[tauri::command]
#[specta::specta]
pub fn remove_recent_search(app: tauri::AppHandle, id: String) -> Result<(), String> {
    RECENT_SEARCHES.remove(&app, &id);
    Ok(())
}

/// Clears every recent-search entry.
#[tauri::command]
#[specta::specta]
pub fn clear_recent_searches(app: tauri::AppHandle) -> Result<(), String> {
    RECENT_SEARCHES.clear(&app);
    Ok(())
}

/// Live-applies a new `search.recentSearches.maxCount` value. Trims the in-memory
/// store and rewrites disk only when entries actually drop.
#[tauri::command]
#[specta::specta]
pub fn apply_recent_searches_max_count(app: tauri::AppHandle, max_count: u32) -> Result<(), String> {
    RECENT_SEARCHES.apply_max_count(&app, max_count as usize);
    Ok(())
}

/// One search-results row, carrying only what ordering it needs.
///
/// A deliberate subset of `SearchResultEntry`: the path, parent path, and icon id
/// decide nothing about order, and leaving them out keeps a full 10,000-row
/// snapshot's round trip small. The frontend holds the rows and re-orders them by
/// the index list this command answers with, so nothing is shipped back.
#[derive(Debug, Clone, serde::Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchSortRow {
    /// The file's own name (the last path component), which the Name and
    /// Extension columns order by. Not the full path the pane DISPLAYS: a row's
    /// name is its name everywhere else in the pane too (type-to-jump, the
    /// context menu, the MCP rows), and one notion of it stays true here.
    pub name: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
}

impl SortableEntry for SearchSortRow {
    fn name(&self) -> &str {
        &self.name
    }
    fn is_directory(&self) -> bool {
        self.is_directory
    }
    fn size(&self) -> Option<u64> {
        self.size
    }
    fn modified_at(&self) -> Option<u64> {
        self.modified_at
    }
    /// A search result carries no creation time, so ordering by Created lands on
    /// the comparator's both-unknown arm and falls back to the name.
    fn created_at(&self) -> Option<u64> {
        None
    }
    /// Nor a recursive size: the search never walked into the directories it
    /// matched, which is also why the pane renders `<dir>` in their Size cell.
    /// Both directories being "unknown" orders them by name among themselves.
    fn recursive_size(&self) -> Option<u64> {
        None
    }
    fn recursive_size_complete(&self) -> Option<bool> {
        None
    }
}

/// Orders a search-results pane's rows, answering with `rows`' own indices in the
/// order they should render.
///
/// The pane's rows arrive ranked by the search engine and the user can re-order
/// them by column, exactly like a directory listing. It runs through
/// [`entry_comparator`], the SAME comparator every directory listing sorts by, so
/// the two can never drift: natural number ordering, case folding, directories
/// first, and the user's `directorySortMode` all come along for free.
///
/// Indices rather than rows because the frontend already holds the full entries;
/// shipping them back would double the round trip for no new information. The sort
/// is STABLE, so rows equal under the chosen column keep the engine's ranked order
/// between them, which makes a re-sort reproducible instead of shuffling ties.
#[tauri::command]
#[specta::specta]
pub fn sort_search_results(
    rows: Vec<SearchSortRow>,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> Vec<u32> {
    let comparator = entry_comparator::<SearchSortRow>(sort_by, sort_order, dir_sort_mode);
    let mut order: Vec<u32> = (0..rows.len() as u32).collect();
    order.sort_by(|a, b| comparator(&rows[*a as usize], &rows[*b as usize]));
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_result_serialization() {
        let result = TranslateResult {
            query: TranslatedQuery {
                name_pattern: Some("*.pdf".to_string()),
                pattern_type: "glob".to_string(),
                min_size: Some(1_048_576),
                max_size: None,
                modified_after: Some(1_735_689_600),
                modified_before: None,
                is_directory: None,
                include_paths: None,
                exclude_dir_names: None,
                case_sensitive: None,
                exclude_system_dirs: None,
            },
            display: TranslateDisplay {
                name_pattern: Some("*.pdf".to_string()),
                pattern_type: Some("glob".to_string()),
                min_size: Some(1_048_576),
                max_size: None,
                modified_after: Some("2025-01-01".to_string()),
                modified_before: None,
                is_directory: None,
                include_paths: None,
                exclude_dir_names: None,
                case_sensitive: None,
            },
            caveat: None,
            label: Some("Big PDFs from 2025".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("namePattern"));
        assert!(json.contains("patternType"));
        assert!(json.contains("2025-01-01"));
        assert!(json.contains("\"label\":\"Big PDFs from 2025\""));
    }

    // ── `sort_search_results` ────────────────────────────────────────────

    use crate::commands::search::{SearchSortRow, sort_search_results};
    use crate::file_system::listing::{DirectorySortMode, SortColumn, SortOrder};

    fn row(name: &str, is_directory: bool, size: Option<u64>, modified_at: Option<u64>) -> SearchSortRow {
        SearchSortRow {
            name: name.to_string(),
            is_directory,
            size,
            modified_at,
        }
    }

    /// The rows a search-results pane holds, in the engine's ranked order.
    fn ranked_rows() -> Vec<SearchSortRow> {
        vec![
            row("img_10.png", false, Some(300), Some(30)),
            row("Notes", true, None, Some(10)),
            row("img_2.png", false, Some(100), Some(20)),
            row("archive", true, None, Some(50)),
            row("README", false, Some(200), Some(40)),
        ]
    }

    fn names_in_order(rows: &[SearchSortRow], order: &[u32]) -> Vec<String> {
        order.iter().map(|i| rows[*i as usize].name.clone()).collect()
    }

    #[test]
    fn sorting_search_results_by_name_puts_directories_first_and_orders_numbers_naturally() {
        let rows = ranked_rows();
        let order = sort_search_results(
            rows.clone(),
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        );
        assert_eq!(
            names_in_order(&rows, &order),
            // Case-folded, so `img_…` sorts before `readme`, and `img_2` before
            // `img_10` because the digit run compares numerically.
            vec!["archive", "Notes", "img_2.png", "img_10.png", "README"]
        );
    }

    #[test]
    fn sorting_search_results_by_size_descending_keeps_directories_ahead_of_files() {
        let rows = ranked_rows();
        let order = sort_search_results(
            rows.clone(),
            SortColumn::Size,
            SortOrder::Descending,
            DirectorySortMode::LikeFiles,
        );
        // Directories lead whatever the column; a snapshot row carries no
        // recursive size, so the two dirs are both unknown and fall back to name.
        assert_eq!(
            names_in_order(&rows, &order),
            vec!["Notes", "archive", "img_10.png", "README", "img_2.png"]
        );
    }

    #[test]
    fn sorting_search_results_leaves_ties_in_the_order_they_arrived() {
        // Two rows that are equal under the column: the engine's ranked order
        // decides, because the sort is stable. That is what makes a re-sort
        // reproducible instead of shuffling equal rows on every click.
        let rows = vec![
            row("b.txt", false, Some(10), Some(1)),
            row("a.txt", false, Some(10), Some(2)),
            row("c.txt", false, Some(10), Some(3)),
        ];
        let order = sort_search_results(
            rows.clone(),
            SortColumn::Size,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        );
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn sorting_an_empty_search_result_set_answers_an_empty_order() {
        let order = sort_search_results(
            vec![],
            SortColumn::Modified,
            SortOrder::Descending,
            DirectorySortMode::LikeFiles,
        );
        assert!(order.is_empty());
    }
}
