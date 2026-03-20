//! IPC commands for drive search.
//!
//! Thin wrappers around `indexing::search` module functions, exposed to the frontend via Tauri commands.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::ai::client::{AiBackend, ChatCompletionOptions};
use crate::indexing::get_read_pool;
use crate::indexing::search::{
    self, DIALOG_OPEN, ParsedScope, SEARCH_INDEX, SearchIndexState, SearchQuery, SearchResult, drop_search_index,
    fill_directory_sizes, start_backstop_timer, start_idle_timer, touch_activity,
};
use crate::indexing::writer::WRITER_GENERATION;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareResult {
    pub ready: bool,
    pub entry_count: u64,
}

/// Called when the search dialog opens. Starts loading the index in the background.
/// Returns immediately with `{ ready, entryCount }`.
#[tauri::command]
pub async fn prepare_search_index(app: tauri::AppHandle) -> Result<PrepareResult, String> {
    touch_activity();
    DIALOG_OPEN.store(true, Ordering::Relaxed);

    // Check if already loaded and fresh
    {
        let mut guard = SEARCH_INDEX.lock().map_err(|e| format!("{e}"))?;
        if let Some(ref mut state) = *guard {
            let current_gen = WRITER_GENERATION.load(Ordering::Relaxed);
            if state.index.generation == current_gen {
                // Cancel any pending idle timer
                if let Some(ref h) = state.idle_timer {
                    h.abort();
                }
                state.idle_timer = None;
                // Reset backstop timer — the previous session's timer may still
                // be ticking and could fire while the dialog is open.
                if let Some(ref h) = state.backstop_timer {
                    h.abort();
                }
                state.backstop_timer = Some(start_backstop_timer());
                return Ok(PrepareResult {
                    ready: true,
                    entry_count: state.index.entries.len() as u64,
                });
            }
            // Stale — drop and reload below
        }
    }

    // Drop stale index if any
    drop_search_index();

    let pool = get_read_pool().ok_or_else(|| "Index not available".to_string())?;
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    // Store a "loading" sentinel with the cancel flag BEFORE spawning the task.
    // This closes the race window where release_search_index can't cancel the
    // load between checking the lock and the background task starting.
    {
        let mut guard = SEARCH_INDEX.lock().map_err(|e| format!("{e}"))?;
        if guard.is_some() {
            return Ok(PrepareResult {
                ready: false,
                entry_count: 0,
            });
        }
        *guard = Some(SearchIndexState {
            index: Arc::new(search::SearchIndex::empty()),
            idle_timer: None,
            backstop_timer: None,
            load_cancel: Some(cancel.clone()),
        });
    }

    // Spawn the load in a background task
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = tokio::task::spawn_blocking(move || search::load_search_index(&pool, &cancel_clone)).await;

        match result {
            Ok(Ok(index)) => {
                let entry_count = index.entries.len() as u64;
                let backstop = start_backstop_timer();
                let mut guard = match SEARCH_INDEX.lock() {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
                *guard = Some(SearchIndexState {
                    index: Arc::new(index),
                    idle_timer: None,
                    backstop_timer: Some(backstop),
                    load_cancel: Some(cancel),
                });
                log::debug!("Search index ready: {entry_count} entries");
                // Emit event to frontend
                use tauri::Emitter;
                let _ = app_clone.emit(
                    "search-index-ready",
                    serde_json::json!({
                        "entryCount": entry_count,
                    }),
                );
            }
            Ok(Err(e)) => {
                if e.contains("cancelled") {
                    log::debug!("Search index load cancelled");
                } else {
                    log::warn!("Search index load failed: {e}");
                }
            }
            Err(e) => {
                log::warn!("Search index load task panicked: {e}");
            }
        }
    });

    Ok(PrepareResult {
        ready: false,
        entry_count: 0,
    })
}

/// Search the in-memory index. Returns empty if not loaded yet.
#[tauri::command]
pub async fn search_files(mut query: SearchQuery) -> Result<SearchResult, String> {
    touch_activity();

    let index = {
        let guard = SEARCH_INDEX.lock().map_err(|e| format!("{e}"))?;
        match guard.as_ref() {
            Some(state) => {
                // Cancel any idle timer since we're actively searching
                if let Some(ref h) = state.idle_timer {
                    h.abort();
                }
                state.index.clone()
            }
            None => {
                return Ok(SearchResult {
                    entries: Vec::new(),
                    total_count: 0,
                });
            }
        }
    };

    // Resolve include paths to entry IDs via SQLite (microseconds, not 20s)
    if query.include_paths.as_ref().is_some_and(|p| !p.is_empty())
        && let Some(pool) = get_read_pool()
    {
        search::resolve_include_paths(&mut query, &pool);
    }

    // Run search on a blocking thread (rayon parallel scan)
    let query_clone = query.clone();
    let index_clone = index.clone();
    let mut result = tokio::task::spawn_blocking(move || search::search(&index_clone, &query_clone))
        .await
        .map_err(|e| format!("Search task failed: {e}"))??;

    // Fill directory sizes from the DB
    if result.entries.iter().any(|e| e.is_directory)
        && let Some(pool) = get_read_pool()
    {
        fill_directory_sizes(&mut result, &pool);
    }

    // Post-filter: remove directories that don't match size criteria.
    // Directory sizes come from dir_stats (not the entries table), so the
    // main search() can't filter them. We over-fetch candidates and trim here.
    let has_size_filter = query.min_size.is_some() || query.max_size.is_some();
    if has_size_filter {
        result.entries.retain(|e| {
            if !e.is_directory {
                return true; // files already filtered in search()
            }
            if let Some(min) = query.min_size {
                match e.size {
                    Some(s) if s >= min => {}
                    _ => return false,
                }
            }
            if let Some(max) = query.max_size {
                match e.size {
                    Some(s) if s <= max => {}
                    _ => return false,
                }
            }
            true
        });
        // total_count is approximate after post-filtering — the true count
        // would require fetching dir_stats for ALL matching directories, which
        // is too expensive. The displayed count may overestimate slightly.
        result.total_count = result.entries.len() as u32;
    }

    // Truncate to the originally requested limit
    let limit = query.limit.min(1000) as usize;
    if result.entries.len() > limit {
        result.entries.truncate(limit);
    }

    // Check generation staleness — trigger background reload if needed
    let current_gen = WRITER_GENERATION.load(Ordering::Relaxed);
    if index.generation != current_gen {
        log::debug!(
            "Search index stale (gen {} vs {}), will reload on next prepare",
            index.generation,
            current_gen
        );
    }

    Ok(result)
}

/// Called when the search dialog closes. Starts the idle timer and
/// cancels any in-progress load.
#[tauri::command]
pub async fn release_search_index() -> Result<(), String> {
    DIALOG_OPEN.store(false, Ordering::Relaxed);
    let mut guard = SEARCH_INDEX.lock().map_err(|e| format!("{e}"))?;

    // Set cancellation flag on any in-progress load
    if let Some(ref state) = *guard
        && let Some(ref cancel) = state.load_cancel
    {
        cancel.store(true, Ordering::Relaxed);
    }

    // Start idle timer
    if guard.is_some() {
        let idle_handle = start_idle_timer();
        if let Some(ref mut state) = *guard {
            // Cancel previous idle timer if any
            if let Some(ref h) = state.idle_timer {
                h.abort();
            }
            state.idle_timer = Some(idle_handle);
        }
    }

    Ok(())
}

/// Parse a scope string into structured include/exclude data.
#[tauri::command]
pub fn parse_search_scope(scope: String) -> ParsedScope {
    search::parse_scope(&scope)
}

/// Returns the list of system/build/cache directory names excluded by default,
/// for display in the UI tooltip.
#[tauri::command]
pub fn get_system_dir_excludes() -> &'static [&'static str] {
    search::SYSTEM_DIR_EXCLUDES
}

// ============================================================================
// AI search query translation
// ============================================================================

/// Intermediate struct for LLM output — uses ISO date strings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSearchQuery {
    pub(crate) name_pattern: Option<String>,
    pub(crate) pattern_type: Option<String>,
    pub(crate) min_size: Option<u64>,
    pub(crate) max_size: Option<u64>,
    pub(crate) modified_after: Option<String>,
    pub(crate) modified_before: Option<String>,
    pub(crate) is_directory: Option<bool>,
    pub(crate) search_paths: Option<Vec<String>>,
    pub(crate) exclude_dirs: Option<Vec<String>>,
    pub(crate) case_sensitive: Option<bool>,
    pub(crate) include_system_dirs: Option<bool>,
    pub(crate) caveat: Option<String>,
}

/// Preflight context from pass 1 results, sent back in pass 2 for refinement.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightContext {
    pub total_count: u32,
    pub sample_entries: Vec<PreflightEntry>,
    /// The raw JSON query from pass 1, so the refinement prompt can show the LLM what it produced.
    #[serde(default)]
    pub pass1_query_json: Option<String>,
}

/// A single entry from the preflight results.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightEntry {
    pub name: String,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
    pub is_directory: bool,
}

/// Human-readable field values returned alongside the structured query.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    pub query: TranslatedQuery,
    pub display: TranslateDisplay,
    pub preflight_summary: Option<String>,
    pub caveat: Option<String>,
}

/// The structured query with unix timestamps, ready for `search_files`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslatedQuery {
    pub name_pattern: Option<String>,
    pub pattern_type: String,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<u64>,
    pub modified_before: Option<u64>,
    pub is_directory: Option<bool>,
    pub include_paths: Option<Vec<String>>,
    pub exclude_dir_names: Option<Vec<String>>,
    pub case_sensitive: Option<bool>,
    pub exclude_system_dirs: Option<bool>,
}

/// Human-readable values so the frontend can populate filter UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateDisplay {
    pub name_pattern: Option<String>,
    pub pattern_type: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
    pub is_directory: Option<bool>,
    pub include_paths: Option<Vec<String>>,
    pub exclude_dir_names: Option<Vec<String>>,
    pub case_sensitive: Option<bool>,
}

/// Converts an ISO date string (YYYY-MM-DD) to a unix timestamp (seconds since epoch).
pub(crate) fn iso_date_to_timestamp(date_str: &str) -> Result<u64, String> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let date = time::Date::parse(date_str, &format).map_err(|e| format!("Invalid date '{date_str}': {e}"))?;
    let datetime = date.with_hms(0, 0, 0).expect("midnight is always valid");
    let timestamp = datetime.assume_utc().unix_timestamp();
    if timestamp < 0 {
        return Err(format!("Date '{date_str}' is before unix epoch"));
    }
    Ok(timestamp as u64)
}

// ── AI search prompt templates ──────────────────────────────────────

/// Core system prompt for translating natural language → search JSON.
/// Dynamic parts (`{TODAY}`, `{ONE_YEAR_AGO}`) are replaced at runtime.
const SEARCH_PROMPT_TEMPLATE: &str = "\
Translate the user's file search query into a JSON object. Return ONLY JSON, no explanation.
This is pass 1 (discovery) — prefer broader patterns that may include false positives over \
precise ones that miss files. A second pass will refine using real results.

Fields (all optional):
  namePattern (string)     Glob (* and ? only) or regex. patternType: \"glob\" or \"regex\".
  minSize / maxSize        Bytes. biggest→500MB, big→100MB, huge→1GB, taking up space→50MB.
  modifiedAfter/Before     ISO date YYYY-MM-DD.
  isDirectory              true=folders, false=files, omit=both.
  excludeDirs              Dir names to skip, e.g. [\"node_modules\",\".git\"].
  searchPaths              Folder paths, ONLY when the user names a specific folder.
                           Never guess paths like ~/projects — omit if unsure.
  caseSensitive            true when exact casing matters. Omit for platform default.
  includeSystemDirs        true when targeting normally-excluded dirs (Logs, Caches, .Trash,
                           node_modules, .git, build output, etc.). Default: excluded.
  caveat                   If the query involves content/semantics not determinable from
                           filename/size/date, explain what was dropped and how to narrow.

NEVER use lookahead (?=), lookbehind (?<=), or backreferences — Rust `regex` crate doesn't \
support them. No \\d either — use [0-9]. Regex is case-insensitive, unanchored unless you add ^ or $.

Categories → extensions:
  docs/resume/CV/report → \\.(pdf|doc|docx|txt|odt|xls|xlsx)$
  photos/images         → \\.(jpg|jpeg|png|heic|webp|gif)$
  screenshots           → ^Screenshot.*\\.(png|jpg|heic)$ (macOS naming)
  videos                → \\.(mp4|mov|avi|mkv|webm)$
  music/audio           → \\.(mp3|m4a|flac|wav|ogg|aac)$
  env files/.env        → ^\\.env(\\..+)?$
  config files          → \\.(json|ya?ml|toml|ini|conf|cfg)$

Rules:
- \"I name them X\" / \"I mark them as X\" → use X as pattern, not descriptive words.
- \"not in X\" / \"excluding X\" → add X to excludeDirs.
- Concepts (\"ssh keys\", \"env files\") → match typical filenames, not query words.
- Code queries → auto-add excludeDirs: node_modules, .git, __pycache__, vendor, .venv, target, build, dist.
- Date math: yesterday=day range, this week=since Monday, last month=since 1st of prev month.
- Results sort by recency, not size. For \"biggest\"/\"largest\", add a size caveat.
- ALWAYS include a namePattern when possible. Only omit it for pure size/date/directory queries.

Examples:
\"invoices I mark as rymd\" → {\"namePattern\":\"*rymd*\",\"patternType\":\"glob\"}
\"log files eating disk\" → {\"namePattern\":\"\\\\.(log|out|err)$\",\"patternType\":\"regex\",\"minSize\":52428800,\"includeSystemDirs\":true}
\"package.json not in node_modules\" → {\"namePattern\":\"^package\\\\.json$\",\"patternType\":\"regex\",\"excludeDirs\":[\"node_modules\"]}
\"photos of my cat\" → {\"namePattern\":\"\\\\.(jpg|jpeg|png|heic|webp|gif)$\",\"patternType\":\"regex\",\"caveat\":\"Can't filter by photo content — add your naming convention (e.g. 'cat-*')\"}
\"documents older than a year\" → {\"namePattern\":\"\\\\.(pdf|doc|docx|txt|odt|xls|xlsx)$\",\"patternType\":\"regex\",\"modifiedBefore\":\"{ONE_YEAR_AGO}\"}
\"screenshots from today\" → {\"namePattern\":\"^Screenshot.*\\\\.(png|jpg|heic)$\",\"patternType\":\"regex\",\"modifiedAfter\":\"{TODAY}\"}
\"node_modules taking up space\" → {\"namePattern\":\"^node_modules$\",\"patternType\":\"regex\",\"isDirectory\":true,\"minSize\":52428800,\"includeSystemDirs\":true}
\"empty folders\" → {\"isDirectory\":true,\"maxSize\":0}
\"anything related to kubernetes\" → {\"namePattern\":\"(k8s|kube|kubectl|helm|kubernetes)\",\"patternType\":\"regex\"}

Today: {TODAY}.";

/// Rules appended to the system prompt for the refinement (pass 2) call.
const REFINEMENT_RULES: &str = "\
This is the REFINEMENT pass. The broad query already ran. Your job: NARROW only, never broaden.

Rules:
- Study result names. Tighten the pattern or add excludeDirs to remove false positives.
- If results are already precise, return the query unchanged.
- Never drop the core search term (e.g. don't generalize \"websocket\" to all server files).
- Never remove constraints that were filtering correctly.
- PRESERVE from pass 1: includeSystemDirs, excludeDirs (add more, don't remove), \
searchPaths (keep unless 0 results), caseSensitive, caveat.
- NEVER use lookahead (?=), lookbehind (?<=), or backreferences — they cause errors.

Return ONLY the refined JSON.";

pub(crate) fn build_search_system_prompt() -> String {
    let today = time::OffsetDateTime::now_utc().date();
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let today_str = today.format(&format).expect("date format always succeeds");

    let one_year_ago = today.replace_year(today.year() - 1).unwrap_or(today);
    let one_year_ago_str = one_year_ago.format(&format).expect("date format always succeeds");

    SEARCH_PROMPT_TEMPLATE
        .replace("{TODAY}", &today_str)
        .replace("{ONE_YEAR_AGO}", &one_year_ago_str)
}

/// Strips markdown code fences from an LLM response and parses it as JSON.
///
/// LLMs sometimes produce invalid JSON escape sequences in regex patterns (e.g. `\.` instead of
/// `\\.`). Before parsing, we fix these by doubling any backslash that precedes a character that
/// isn't a valid JSON escape target (`"`, `\`, `/`, `b`, `f`, `n`, `r`, `t`, `u`).
pub(crate) fn parse_ai_response(response: &str) -> Result<AiSearchQuery, String> {
    let json_str = response.trim();
    let json_str = json_str
        .strip_prefix("```json")
        .or_else(|| json_str.strip_prefix("```"))
        .unwrap_or(json_str);
    let json_str = json_str.strip_suffix("```").unwrap_or(json_str).trim();

    let fixed = fix_json_backslash_escapes(json_str);

    serde_json::from_str(&fixed)
        .map_err(|_| "Couldn't understand that query. Try rephrasing or use the manual filters.".to_string())
}

/// Fix invalid JSON backslash escapes inside string values.
///
/// Scans character by character, tracking whether we're inside a JSON string (between unescaped
/// `"`). When inside a string and we encounter `\` followed by a character that isn't a valid
/// JSON escape (`"`, `\`, `/`, `b`, `f`, `n`, `r`, `t`, `u`), we insert an extra `\` to produce
/// the valid escape `\\`.
fn fix_json_backslash_escapes(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + 16);
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(ch) = chars.next() {
        if !in_string {
            result.push(ch);
            if ch == '"' {
                in_string = true;
            }
        } else {
            // Inside a JSON string
            if ch == '"' {
                // Unescaped quote ends the string
                result.push(ch);
                in_string = false;
            } else if ch == '\\' {
                // Look at what follows the backslash
                if let Some(&next) = chars.peek() {
                    if matches!(next, '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' | 'u') {
                        // Valid JSON escape — emit as-is
                        result.push('\\');
                        result.push(chars.next().unwrap());
                    } else {
                        // Invalid JSON escape (e.g. `\.`, `\d`, `\w`) — double the backslash
                        result.push('\\');
                        result.push('\\');
                        // Don't consume `next` — it's a normal character
                    }
                } else {
                    // Trailing backslash at end of input — emit as-is
                    result.push('\\');
                }
            } else {
                result.push(ch);
            }
        }
    }

    result
}

/// Generates a human-readable one-line summary of an AI search query.
///
/// Format: `*pattern* · size ≥ X · after YYYY-MM-DD` — only non-null fields, separated by ` · `.
pub(crate) fn summarize_ai_query(q: &AiSearchQuery) -> String {
    let mut parts = Vec::new();

    if let Some(ref pat) = q.name_pattern {
        let is_regex = q.pattern_type.as_deref().is_some_and(|t| t == "regex");
        if is_regex {
            parts.push(format!("{pat} (regex)"));
        } else {
            parts.push(pat.clone());
        }
    }

    if let Some(min) = q.min_size {
        parts.push(format!("size \u{2265} {}", search::format_size(min)));
    }
    if let Some(max) = q.max_size {
        parts.push(format!("size \u{2264} {}", search::format_size(max)));
    }

    if let Some(ref after) = q.modified_after {
        parts.push(format!("after {after}"));
    }
    if let Some(ref before) = q.modified_before {
        parts.push(format!("before {before}"));
    }

    if let Some(true) = q.is_directory {
        parts.push("dirs only".to_string());
    } else if let Some(false) = q.is_directory {
        parts.push("files only".to_string());
    }

    if parts.is_empty() {
        "(all entries)".to_string()
    } else {
        parts.join(" \u{00b7} ")
    }
}

/// Formats preflight entries into a compact text table for the LLM refinement prompt.
pub(crate) fn format_preflight_table(ctx: &PreflightContext) -> String {
    let mut lines = Vec::with_capacity(ctx.sample_entries.len() + 2);

    lines.push(format!(
        "Your initial query returned {} results. Here are the top {} by recency:",
        ctx.total_count,
        ctx.sample_entries.len()
    ));
    lines.push(String::new());

    for entry in &ctx.sample_entries {
        let name = if entry.is_directory {
            let mut n = entry.name.clone();
            n.push('/');
            n
        } else {
            entry.name.clone()
        };

        // Truncate name at 45 chars (char-boundary-safe)
        let display_name = if name.chars().count() > 45 {
            let truncated: String = name.chars().take(42).collect();
            format!("{truncated}...")
        } else {
            name
        };

        let size_str = entry.size.map(search::format_size).unwrap_or_default();

        let date_str = entry.modified_at.map(search::format_timestamp).unwrap_or_default();

        lines.push(format!("  {:<45} {:>8}   {}", display_name, size_str, date_str));
    }

    lines.join("\n")
}

/// Builds the refinement system prompt for pass 2 (with preflight context).
pub(crate) fn build_refinement_system_prompt(natural_query: &str, ctx: &PreflightContext) -> String {
    let base_prompt = build_search_system_prompt();
    let table = format_preflight_table(ctx);

    let pass1_section = match &ctx.pass1_query_json {
        Some(json) => format!("Your pass 1 query was:\n```json\n{json}\n```\n\n"),
        None => String::new(),
    };

    format!("{base_prompt}\n\n---\n\n{pass1_section}{table}\n\nUser query: \"{natural_query}\"\n\n{REFINEMENT_RULES}")
}

/// Converts a parsed `AiSearchQuery` into the final `TranslateResult`.
pub(crate) fn build_translate_result(
    ai_query: AiSearchQuery,
    preflight_summary: Option<String>,
) -> Result<TranslateResult, String> {
    let modified_after_ts = ai_query
        .modified_after
        .as_deref()
        .map(iso_date_to_timestamp)
        .transpose()?;
    let modified_before_ts = ai_query
        .modified_before
        .as_deref()
        .map(iso_date_to_timestamp)
        .transpose()?;

    let pattern_type = ai_query.pattern_type.clone().unwrap_or_else(|| "glob".to_string());

    // Expand ~ in search paths
    let include_paths = ai_query.search_paths.map(|paths| {
        paths
            .into_iter()
            .map(|p| crate::commands::file_system::expand_tilde(&p))
            .collect::<Vec<_>>()
    });

    let exclude_dir_names = ai_query.exclude_dirs.clone();

    Ok(TranslateResult {
        query: TranslatedQuery {
            name_pattern: ai_query.name_pattern.clone(),
            pattern_type: pattern_type.clone(),
            min_size: ai_query.min_size,
            max_size: ai_query.max_size,
            modified_after: modified_after_ts,
            modified_before: modified_before_ts,
            is_directory: ai_query.is_directory,
            include_paths: include_paths.clone(),
            exclude_dir_names: exclude_dir_names.clone(),
            case_sensitive: ai_query.case_sensitive,
            exclude_system_dirs: if ai_query.include_system_dirs == Some(true) {
                Some(false)
            } else {
                None
            },
        },
        display: TranslateDisplay {
            name_pattern: ai_query.name_pattern,
            pattern_type: Some(pattern_type),
            min_size: ai_query.min_size,
            max_size: ai_query.max_size,
            modified_after: ai_query.modified_after,
            modified_before: ai_query.modified_before,
            is_directory: ai_query.is_directory,
            include_paths,
            exclude_dir_names,
            case_sensitive: ai_query.case_sensitive,
        },
        preflight_summary,
        caveat: ai_query.caveat,
    })
}

/// If the AI returned a regex pattern, validates it against the `regex` crate.
/// Returns `Ok(())` if valid or not a regex, `Err(message)` with the compile error otherwise.
pub(crate) fn validate_regex_pattern(ai_query: &AiSearchQuery) -> Result<(), String> {
    let is_regex = ai_query.pattern_type.as_deref().is_some_and(|t| t == "regex");
    if !is_regex {
        return Ok(());
    }
    if let Some(ref pattern) = ai_query.name_pattern {
        regex::Regex::new(pattern).map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

/// Resolves the AI backend from the current provider configuration.
fn resolve_ai_backend() -> Result<AiBackend, String> {
    let provider = crate::ai::manager::get_provider();
    match provider.as_str() {
        "off" => Err("AI is not configured. Enable an AI provider in settings.".to_string()),
        "local" => {
            let port = crate::ai::manager::get_port()
                .ok_or_else(|| "Local AI server isn't running. Start it in settings.".to_string())?;
            Ok(AiBackend::Local { port })
        }
        "openai-compatible" => {
            let (api_key, base_url, model) = crate::ai::manager::get_openai_config();
            if api_key.is_empty() {
                return Err("OpenAI API key not configured. Add it in settings.".to_string());
            }
            Ok(AiBackend::OpenAi {
                api_key,
                base_url,
                model,
            })
        }
        _ => Err(format!("Unknown AI provider: {provider}")),
    }
}

/// Calls the LLM to translate a natural language query into search filters.
/// Used by both the IPC command and the MCP executor.
///
/// Pass 1 (no `preflight_context`): broad query using the standard system prompt.
/// Pass 2 (with `preflight_context`): refinement prompt that includes real results from pass 1.
/// Returns `(ai_query, preflight_summary)` — summary is `Some` only for pass 1.
pub(crate) async fn call_ai_translate(
    natural_query: &str,
    preflight_context: Option<&PreflightContext>,
) -> Result<(AiSearchQuery, Option<String>), String> {
    let pass_label = if preflight_context.is_some() {
        "pass 2"
    } else {
        "pass 1"
    };
    log::debug!("MCP ai_search: call_ai_translate ({pass_label}) entered, query={natural_query:?}");

    let backend = match resolve_ai_backend() {
        Ok(b) => {
            log::debug!("MCP ai_search: AI backend resolved successfully");
            b
        }
        Err(e) => {
            log::error!("MCP ai_search: resolve_ai_backend failed: {e}");
            return Err(e);
        }
    };

    let system_prompt = match preflight_context {
        Some(ctx) => build_refinement_system_prompt(natural_query, ctx),
        None => build_search_system_prompt(),
    };

    log::debug!(
        "AI search ({pass_label}): full system prompt ({} chars):\n{system_prompt}",
        system_prompt.len()
    );
    log::debug!("AI search ({pass_label}): user message: {natural_query:?}");

    let options = ChatCompletionOptions {
        system_prompt,
        temperature: 0.3,
        max_tokens: 200,
        top_p: 0.9,
    };

    let t0 = std::time::Instant::now();
    let response = match crate::ai::client::chat_completion(&backend, natural_query, &options).await {
        Ok(r) => {
            log::info!(
                "AI search: chat_completion ({pass_label}) returned {} chars in {:.1}s",
                r.len(),
                t0.elapsed().as_secs_f64()
            );
            log::debug!("MCP ai_search: chat_completion ({pass_label}) raw response: {r:?}");
            r
        }
        Err(e) => {
            log::warn!(
                "AI search: chat_completion ({pass_label}) failed after {:.1}s for query={natural_query:?}: {e}",
                t0.elapsed().as_secs_f64()
            );
            return Err(format!("{e}"));
        }
    };

    log::debug!("MCP ai_search: parsing AI response ({pass_label})...");
    let mut ai_query = match parse_ai_response(&response) {
        Ok(q) => {
            log::debug!(
                "MCP ai_search: parse_ai_response ({pass_label}) succeeded, pattern={:?}",
                q.name_pattern
            );
            q
        }
        Err(e) => {
            log::error!("MCP ai_search: parse_ai_response ({pass_label}) failed: {e}, raw response was: {response:?}");
            return Err(e);
        }
    };

    // Validate regex patterns — retry once if invalid
    if let Err(regex_error) = validate_regex_pattern(&ai_query) {
        let pattern = ai_query.name_pattern.as_deref().unwrap_or("");
        let retry_prompt = format!(
            "You gave me this pattern: `{pattern}`, but it's not valid regex: {regex_error}. \
             Please fix it using Rust `regex` crate syntax (no lookahead/lookbehind, no backreferences). \
             Return the same JSON object with the corrected pattern."
        );

        log::warn!("MCP ai_search: regex validation failed ({pass_label}), retrying: {regex_error}");

        let retry_response = match crate::ai::client::chat_completion(&backend, &retry_prompt, &options).await {
            Ok(r) => {
                log::debug!("MCP ai_search: regex retry chat_completion returned {} chars", r.len());
                r
            }
            Err(e) => {
                log::warn!("AI search: regex retry chat_completion failed for query={natural_query:?}: {e}");
                return Err(format!("{e}"));
            }
        };

        ai_query = match parse_ai_response(&retry_response) {
            Ok(q) => {
                log::debug!(
                    "MCP ai_search: regex retry parse succeeded, pattern={:?}",
                    q.name_pattern
                );
                q
            }
            Err(e) => {
                log::error!("MCP ai_search: regex retry parse failed: {e}, raw response was: {retry_response:?}");
                return Err(e);
            }
        };

        // Validate the retry — if still invalid, return the error
        if let Err(retry_error) = validate_regex_pattern(&ai_query) {
            log::error!("MCP ai_search: regex still invalid after retry: {retry_error}");
            return Err(format!("AI generated invalid regex pattern: {retry_error}"));
        }
    }

    // Generate preflight summary only for pass 1 (no preflight context)
    let preflight_summary = if preflight_context.is_none() {
        Some(summarize_ai_query(&ai_query))
    } else {
        None
    };

    log::debug!("MCP ai_search: call_ai_translate ({pass_label}) completed successfully");
    Ok((ai_query, preflight_summary))
}

/// Translates a natural language search query into structured filters using the configured LLM.
///
/// Pass 1 (no `preflight_context`): broad query using the standard system prompt.
/// Pass 2 (with `preflight_context`): refinement prompt that includes real results from pass 1.
#[tauri::command]
pub async fn translate_search_query(
    natural_query: String,
    preflight_context: Option<PreflightContext>,
) -> Result<TranslateResult, String> {
    let (ai_query, preflight_summary) = call_ai_translate(&natural_query, preflight_context.as_ref()).await?;

    build_translate_result(ai_query, preflight_summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iso_date_to_timestamp() {
        // 2025-01-01 00:00:00 UTC = 1735689600
        let ts = iso_date_to_timestamp("2025-01-01").unwrap();
        assert_eq!(ts, 1_735_689_600);
    }

    #[test]
    fn test_iso_date_to_timestamp_mid_year() {
        // 2026-06-15 00:00:00 UTC = 1781481600
        let ts = iso_date_to_timestamp("2026-06-15").unwrap();
        assert_eq!(ts, 1_781_481_600);
    }

    #[test]
    fn test_iso_date_to_timestamp_invalid() {
        assert!(iso_date_to_timestamp("not-a-date").is_err());
        assert!(iso_date_to_timestamp("2025-13-01").is_err());
        assert!(iso_date_to_timestamp("2025-01-32").is_err());
    }

    #[test]
    fn test_ai_search_query_deserialization_all_fields() {
        let json = r#"{
            "namePattern": "*.pdf",
            "patternType": "glob",
            "minSize": 1048576,
            "maxSize": 10485760,
            "modifiedAfter": "2025-01-01",
            "modifiedBefore": "2025-12-31",
            "isDirectory": false
        }"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("*.pdf"));
        assert_eq!(q.pattern_type.as_deref(), Some("glob"));
        assert_eq!(q.min_size, Some(1_048_576));
        assert_eq!(q.max_size, Some(10_485_760));
        assert_eq!(q.modified_after.as_deref(), Some("2025-01-01"));
        assert_eq!(q.modified_before.as_deref(), Some("2025-12-31"));
        assert_eq!(q.is_directory, Some(false));
    }

    #[test]
    fn test_ai_search_query_deserialization_minimal() {
        let json = r#"{"namePattern": "*.jpg"}"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("*.jpg"));
        assert!(q.pattern_type.is_none());
        assert!(q.min_size.is_none());
        assert!(q.max_size.is_none());
        assert!(q.modified_after.is_none());
        assert!(q.modified_before.is_none());
        assert!(q.is_directory.is_none());
    }

    #[test]
    fn test_ai_search_query_deserialization_empty_object() {
        let json = r#"{}"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert!(q.name_pattern.is_none());
        assert!(q.pattern_type.is_none());
    }

    #[test]
    fn test_ai_search_query_deserialization_regex_type() {
        let json = r#"{"namePattern": "Q[1-4].*\\.pdf", "patternType": "regex"}"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.pattern_type.as_deref(), Some("regex"));
    }

    #[test]
    fn test_ai_search_query_deserialization_directory_only() {
        let json = r#"{"isDirectory": true, "minSize": 1073741824}"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.is_directory, Some(true));
        assert_eq!(q.min_size, Some(1_073_741_824));
    }

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
            preflight_summary: Some("*.pdf \u{00b7} size \u{2265} 1 MB".to_string()),
            caveat: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("namePattern"));
        assert!(json.contains("patternType"));
        assert!(json.contains("2025-01-01"));
        assert!(json.contains("preflightSummary"));
    }

    #[test]
    fn test_build_search_system_prompt_contains_date() {
        let prompt = build_search_system_prompt();
        assert!(prompt.contains("Today:"));
        assert!(prompt.contains("Return ONLY JSON"));
        // Should contain a date in YYYY-MM-DD format
        assert!(prompt.contains("20")); // Year starts with 20
    }

    #[test]
    fn test_build_search_system_prompt_contains_regex_flavor() {
        let prompt = build_search_system_prompt();
        assert!(prompt.contains("Rust `regex` crate"));
        assert!(prompt.contains("NEVER use lookahead"));
    }

    #[test]
    fn test_parse_ai_response_plain_json() {
        let response = r#"{"namePattern": "*.pdf", "patternType": "glob"}"#;
        let q = parse_ai_response(response).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("*.pdf"));
        assert_eq!(q.pattern_type.as_deref(), Some("glob"));
    }

    #[test]
    fn test_parse_ai_response_with_code_fences() {
        let response = "```json\n{\"namePattern\": \"*.txt\"}\n```";
        let q = parse_ai_response(response).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("*.txt"));
    }

    #[test]
    fn test_parse_ai_response_invalid_json() {
        assert!(parse_ai_response("not json at all").is_err());
    }

    #[test]
    fn test_validate_regex_pattern_valid() {
        let q = AiSearchQuery {
            name_pattern: Some("[0-9]+\\.pdf".to_string()),
            pattern_type: Some("regex".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        assert!(validate_regex_pattern(&q).is_ok());
    }

    #[test]
    fn test_validate_regex_pattern_invalid() {
        let q = AiSearchQuery {
            name_pattern: Some("[unclosed".to_string()),
            pattern_type: Some("regex".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        assert!(validate_regex_pattern(&q).is_err());
    }

    #[test]
    fn test_validate_regex_pattern_glob_skips_validation() {
        let q = AiSearchQuery {
            name_pattern: Some("[unclosed".to_string()),
            pattern_type: Some("glob".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        // Glob patterns aren't validated as regex
        assert!(validate_regex_pattern(&q).is_ok());
    }

    #[test]
    fn test_validate_regex_pattern_none_skips_validation() {
        let q = AiSearchQuery {
            name_pattern: None,
            pattern_type: Some("regex".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        assert!(validate_regex_pattern(&q).is_ok());
    }

    #[test]
    fn test_build_translate_result_with_regex() {
        let q = AiSearchQuery {
            name_pattern: Some("Q[1-4].*\\.pdf".to_string()),
            pattern_type: Some("regex".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        let result = build_translate_result(q, None).unwrap();
        assert_eq!(result.query.pattern_type, "regex");
        assert_eq!(result.display.pattern_type.as_deref(), Some("regex"));
    }

    #[test]
    fn test_ai_search_query_deserialization_with_scope_fields() {
        let json = r#"{
            "namePattern": "*.py",
            "patternType": "glob",
            "searchPaths": ["~/projects", "~/work"],
            "excludeDirs": ["node_modules", ".git", "__pycache__"]
        }"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("*.py"));
        let paths = q.search_paths.unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "~/projects");
        assert_eq!(paths[1], "~/work");
        let excludes = q.exclude_dirs.unwrap();
        assert_eq!(excludes.len(), 3);
        assert_eq!(excludes[0], "node_modules");
    }

    #[test]
    fn test_build_translate_result_with_search_paths_and_excludes() {
        let q = AiSearchQuery {
            name_pattern: Some("*.py".to_string()),
            pattern_type: Some("glob".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: Some(vec!["~/projects".to_string()]),
            exclude_dirs: Some(vec!["node_modules".to_string(), ".git".to_string()]),
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        let result = build_translate_result(q, None).unwrap();

        // search_paths should have ~ expanded
        let paths = result.query.include_paths.unwrap();
        assert!(!paths[0].starts_with('~'), "~ should be expanded");
        assert!(paths[0].contains("projects"));

        // exclude_dirs passed through
        let excludes = result.query.exclude_dir_names.unwrap();
        assert_eq!(excludes, vec!["node_modules", ".git"]);

        // display should also have the values
        assert!(result.display.include_paths.is_some());
        assert!(result.display.exclude_dir_names.is_some());
    }

    #[test]
    fn test_build_search_system_prompt_contains_scope_fields() {
        let prompt = build_search_system_prompt();
        assert!(prompt.contains("searchPaths"));
        assert!(prompt.contains("excludeDirs"));
        assert!(prompt.contains("node_modules"));
        assert!(prompt.contains("caseSensitive"));
    }

    // ── Preflight / two-pass tests ──────────────────────────────────────

    #[test]
    fn test_summarize_ai_query_name_only() {
        let q = AiSearchQuery {
            name_pattern: Some("*resume*".to_string()),
            pattern_type: None,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        assert_eq!(summarize_ai_query(&q), "*resume*");
    }

    #[test]
    fn test_summarize_ai_query_pattern_with_size() {
        let q = AiSearchQuery {
            name_pattern: Some("*.pdf".to_string()),
            pattern_type: Some("glob".to_string()),
            min_size: Some(10_485_760),
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        assert_eq!(summarize_ai_query(&q), "*.pdf \u{00b7} size \u{2265} 10 MB");
    }

    #[test]
    fn test_summarize_ai_query_regex_with_date() {
        let q = AiSearchQuery {
            name_pattern: Some("Screenshot.*".to_string()),
            pattern_type: Some("regex".to_string()),
            min_size: None,
            max_size: None,
            modified_after: Some("2026-03-16".to_string()),
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        assert_eq!(summarize_ai_query(&q), "Screenshot.* (regex) \u{00b7} after 2026-03-16");
    }

    #[test]
    fn test_summarize_ai_query_size_and_dirs_only() {
        let q = AiSearchQuery {
            name_pattern: None,
            pattern_type: None,
            min_size: Some(1_073_741_824),
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: Some(true),
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        assert_eq!(summarize_ai_query(&q), "size \u{2265} 1 GB \u{00b7} dirs only");
    }

    #[test]
    fn test_summarize_ai_query_empty() {
        let q = AiSearchQuery {
            name_pattern: None,
            pattern_type: None,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        assert_eq!(summarize_ai_query(&q), "(all entries)");
    }

    #[test]
    fn test_summarize_ai_query_all_fields() {
        let q = AiSearchQuery {
            name_pattern: Some("*.log".to_string()),
            pattern_type: Some("glob".to_string()),
            min_size: Some(1_048_576),
            max_size: Some(104_857_600),
            modified_after: Some("2025-01-01".to_string()),
            modified_before: Some("2025-12-31".to_string()),
            is_directory: Some(false),
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        let summary = summarize_ai_query(&q);
        assert!(summary.contains("*.log"));
        assert!(summary.contains("size \u{2265} 1 MB"));
        assert!(summary.contains("size \u{2264} 100 MB"));
        assert!(summary.contains("after 2025-01-01"));
        assert!(summary.contains("before 2025-12-31"));
        assert!(summary.contains("files only"));
    }

    #[test]
    fn test_format_preflight_table_basic() {
        let ctx = PreflightContext {
            total_count: 806,
            pass1_query_json: None,
            sample_entries: vec![
                PreflightEntry {
                    name: ".fastresume".to_string(),
                    size: Some(4096),
                    modified_at: Some(1_774_000_000),
                    is_directory: false,
                },
                PreflightEntry {
                    name: "Resume_2025_final.pdf".to_string(),
                    size: Some(91_136),
                    modified_at: Some(1_730_592_000),
                    is_directory: false,
                },
                PreflightEntry {
                    name: "reports".to_string(),
                    size: Some(1024),
                    modified_at: Some(1_724_000_000),
                    is_directory: true,
                },
            ],
        };

        let table = format_preflight_table(&ctx);
        assert!(table.contains("806 results"));
        assert!(table.contains("top 3 by recency"));
        assert!(table.contains(".fastresume"));
        assert!(table.contains("Resume_2025_final.pdf"));
        assert!(table.contains("reports/")); // directory gets trailing /
    }

    #[test]
    fn test_format_preflight_table_name_truncation() {
        let ctx = PreflightContext {
            total_count: 1,
            pass1_query_json: None,
            sample_entries: vec![PreflightEntry {
                name: "a_very_long_filename_that_definitely_exceeds_45_characters_limit.pdf".to_string(),
                size: Some(1024),
                modified_at: Some(1_700_000_000),
                is_directory: false,
            }],
        };

        let table = format_preflight_table(&ctx);
        assert!(table.contains("...")); // truncated
    }

    #[test]
    fn test_build_refinement_system_prompt_includes_context() {
        let ctx = PreflightContext {
            total_count: 100,
            pass1_query_json: Some(r#"{"namePattern":"*resume*","patternType":"glob"}"#.to_string()),
            sample_entries: vec![PreflightEntry {
                name: "test.pdf".to_string(),
                size: Some(1024),
                modified_at: Some(1_700_000_000),
                is_directory: false,
            }],
        };

        let prompt = build_refinement_system_prompt("find my resume", &ctx);
        // Contains the base prompt
        assert!(prompt.contains("Return ONLY JSON"));
        // Contains the preflight table
        assert!(prompt.contains("100 results"));
        assert!(prompt.contains("test.pdf"));
        // Contains the refinement instruction
        assert!(prompt.contains("find my resume"));
        assert!(prompt.contains("REFINEMENT pass"));
        // Contains the pass 1 query JSON
        assert!(prompt.contains("Your pass 1 query was:"));
        assert!(prompt.contains("*resume*"));
        // Contains the flag preservation rule
        assert!(prompt.contains("includeSystemDirs"));
        assert!(prompt.contains("PRESERVE from pass 1"));
    }

    #[test]
    fn test_preflight_context_serde_roundtrip() {
        let json = r#"{
            "totalCount": 42,
            "sampleEntries": [
                {
                    "name": "test.txt",
                    "size": 1024,
                    "modifiedAt": 1700000000,
                    "isDirectory": false
                },
                {
                    "name": "docs",
                    "size": null,
                    "modifiedAt": null,
                    "isDirectory": true
                }
            ]
        }"#;
        let ctx: PreflightContext = serde_json::from_str(json).unwrap();
        assert_eq!(ctx.total_count, 42);
        assert_eq!(ctx.sample_entries.len(), 2);
        assert_eq!(ctx.sample_entries[0].name, "test.txt");
        assert_eq!(ctx.sample_entries[0].size, Some(1024));
        assert!(ctx.sample_entries[0].modified_at.is_some());
        assert!(!ctx.sample_entries[0].is_directory);
        assert_eq!(ctx.sample_entries[1].name, "docs");
        assert!(ctx.sample_entries[1].size.is_none());
        assert!(ctx.sample_entries[1].modified_at.is_none());
        assert!(ctx.sample_entries[1].is_directory);
    }

    #[test]
    fn test_build_translate_result_with_preflight_summary() {
        let q = AiSearchQuery {
            name_pattern: Some("*.pdf".to_string()),
            pattern_type: Some("glob".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        let summary = "*.pdf".to_string();
        let result = build_translate_result(q, Some(summary.clone())).unwrap();
        assert_eq!(result.preflight_summary, Some(summary));
    }

    #[test]
    fn test_build_translate_result_without_preflight_summary() {
        let q = AiSearchQuery {
            name_pattern: Some("*.txt".to_string()),
            pattern_type: None,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            search_paths: None,
            exclude_dirs: None,
            case_sensitive: None,
            include_system_dirs: None,
            caveat: None,
        };
        let result = build_translate_result(q, None).unwrap();
        assert!(result.preflight_summary.is_none());
    }

    // ── JSON backslash escape fix tests ────────────────────────────────

    #[test]
    fn test_parse_ai_response_invalid_backslash_dot() {
        // LLM returns `\.` which is not a valid JSON escape
        let response =
            r#"{"namePattern": "(presentation|slides|deck).*\.(pdf|ppt|pptx|key)$", "patternType": "regex"}"#;
        let q = parse_ai_response(response).unwrap();
        assert_eq!(
            q.name_pattern.as_deref(),
            Some(r"(presentation|slides|deck).*\.(pdf|ppt|pptx|key)$")
        );
        assert_eq!(q.pattern_type.as_deref(), Some("regex"));
    }

    #[test]
    fn test_parse_ai_response_valid_double_backslash_dot() {
        // Already valid JSON: `\\.` decodes to `\.`
        let response = r#"{"namePattern": "\\.(ttf|otf|woff|woff2)$", "patternType": "regex"}"#;
        let q = parse_ai_response(response).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some(r"\.(ttf|otf|woff|woff2)$"));
    }

    #[test]
    fn test_parse_ai_response_invalid_backslash_d() {
        // `\d` is a common regex escape but invalid in JSON
        let response = r#"{"namePattern": "\d{4}-\d{2}-\d{2}", "patternType": "regex"}"#;
        let q = parse_ai_response(response).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some(r"\d{4}-\d{2}-\d{2}"));
    }

    #[test]
    fn test_parse_ai_response_valid_escapes_not_modified() {
        // Valid JSON escapes: `\n`, `\"`, `\\` should NOT be doubled
        let response = r#"{"namePattern": "line1\\nline2"}"#;
        let q = parse_ai_response(response).unwrap();
        // `\\n` in JSON source → `\n` in the parsed string (literal backslash + n)
        assert_eq!(q.name_pattern.as_deref(), Some("line1\\nline2"));
    }

    #[test]
    fn test_fix_json_backslash_escapes_preserves_valid() {
        // All valid JSON escapes should pass through unchanged
        let input = r#"{"a": "quote:\" backslash:\\ slash:\/ bs:\b ff:\f nl:\n cr:\r tab:\t uni:\u0041"}"#;
        let fixed = fix_json_backslash_escapes(input);
        assert_eq!(fixed, input);
    }

    #[test]
    fn test_fix_json_backslash_escapes_fixes_invalid() {
        // `\.` and `\w` are invalid JSON escapes → should become `\\.` and `\\w`
        let input = r#"{"p": "\.\w"}"#;
        let fixed = fix_json_backslash_escapes(input);
        assert_eq!(fixed, r#"{"p": "\\.\\w"}"#);
        // Verify the fixed string parses as valid JSON
        let v: serde_json::Value = serde_json::from_str(&fixed).unwrap();
        assert_eq!(v["p"].as_str().unwrap(), r"\.\w");
    }

    #[test]
    fn test_fix_json_backslash_escapes_outside_strings() {
        // Backslashes outside strings should not be touched (though unusual in JSON)
        let input = r#"{"k": "v"}"#;
        let fixed = fix_json_backslash_escapes(input);
        assert_eq!(fixed, input);
    }

    #[test]
    fn test_ai_search_query_serde_roundtrip_with_caveat() {
        let json = r#"{
            "namePattern": "\\.(jpg|jpeg|png|heic)$",
            "patternType": "regex",
            "caveat": "Can't filter by photo content — add your naming convention if you have one"
        }"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("\\.(jpg|jpeg|png|heic)$"));
        assert_eq!(q.pattern_type.as_deref(), Some("regex"));
        assert_eq!(
            q.caveat.as_deref(),
            Some("Can't filter by photo content — add your naming convention if you have one")
        );

        // Without caveat — field should be None
        let json_no_caveat = r#"{"namePattern": "*.pdf"}"#;
        let q2: AiSearchQuery = serde_json::from_str(json_no_caveat).unwrap();
        assert!(q2.caveat.is_none());
    }
}
