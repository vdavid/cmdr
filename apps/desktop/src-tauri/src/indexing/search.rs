//! In-memory search index and search execution.
//!
//! Loads all entries from the index DB into a `Vec<SearchEntry>` for fast
//! parallel scanning with rayon. The index is loaded lazily when the search
//! dialog opens and dropped after an idle timeout.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use rayon::prelude::*;
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};

use super::enrichment::ReadPool;
#[cfg(any(not(target_os = "macos"), test))]
use super::store;
use super::store::{IndexStore, ROOT_ID};
use super::writer::WRITER_GENERATION;

// ── Search entry (in-memory representation) ──────────────────────────

#[derive(Debug)]
pub struct SearchEntry {
    pub id: i64,
    pub parent_id: i64,
    pub name: String,
    pub name_folded: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
}

// ── Search index ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SearchIndex {
    pub entries: Vec<SearchEntry>,
    pub id_to_index: HashMap<i64, usize>,
    pub generation: u64,
}

pub(crate) struct SearchIndexState {
    pub index: Arc<SearchIndex>,
    pub idle_timer: Option<tauri::async_runtime::JoinHandle<()>>,
    pub backstop_timer: Option<tauri::async_runtime::JoinHandle<()>>,
    pub load_cancel: Option<Arc<AtomicBool>>,
}

pub(crate) static SEARCH_INDEX: LazyLock<Mutex<Option<SearchIndexState>>> = LazyLock::new(|| Mutex::new(None));

/// Timestamp of the last search-related IPC call, for backstop timeout.
static LAST_SEARCH_ACTIVITY: AtomicU64 = AtomicU64::new(0);

/// Whether the search dialog is currently open. Timers check this before dropping.
pub(crate) static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);

/// Idle timeout: drop the index 5 minutes after `release_search_index`.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Backstop timeout: drop the index if no search calls arrive within 10 minutes.
const BACKSTOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Rows between cancellation checks during load.
const CANCEL_CHECK_INTERVAL: usize = 100_000;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Record a search activity timestamp (resets backstop timer logic).
pub(crate) fn touch_activity() {
    LAST_SEARCH_ACTIVITY.store(now_secs(), Ordering::Relaxed);
}

// ── Index loading ────────────────────────────────────────────────────

/// Load all entries from the index DB into an in-memory `SearchIndex`.
///
/// Uses platform-conditional SQL: macOS reads `name_folded` from the DB,
/// Linux computes it via `store::normalize_for_comparison`.
pub fn load_search_index(pool: &ReadPool, cancel: &AtomicBool) -> Result<SearchIndex, String> {
    pool.with_conn(|conn| {
        let t = std::time::Instant::now();
        let generation = WRITER_GENERATION.load(Ordering::Relaxed);

        #[cfg(target_os = "macos")]
        let sql = "SELECT id, parent_id, name, name_folded, is_directory, size, modified_at FROM entries";
        #[cfg(not(target_os = "macos"))]
        let sql = "SELECT id, parent_id, name, is_directory, size, modified_at FROM entries";

        let mut stmt = conn.prepare(sql).map_err(|e| format!("Prepare failed: {e}"))?;

        let mut entries = Vec::with_capacity(5_000_000);
        let mut id_to_index = HashMap::with_capacity(5_000_000);

        let mut rows = stmt.query([]).map_err(|e| format!("Query failed: {e}"))?;
        let mut row_count = 0usize;

        while let Some(row) = rows.next().map_err(|e| format!("Row read failed: {e}"))? {
            if row_count.is_multiple_of(CANCEL_CHECK_INTERVAL) && cancel.load(Ordering::Relaxed) {
                return Err("Load cancelled".to_string());
            }

            #[cfg(target_os = "macos")]
            let entry = {
                let id: i64 = row.get(0).map_err(|e| format!("{e}"))?;
                let parent_id: i64 = row.get(1).map_err(|e| format!("{e}"))?;
                let name: String = row.get(2).map_err(|e| format!("{e}"))?;
                let name_folded: String = row.get(3).map_err(|e| format!("{e}"))?;
                let is_directory: bool = row.get(4).map_err(|e| format!("{e}"))?;
                let size: Option<u64> = row.get(5).map_err(|e| format!("{e}"))?;
                let modified_at: Option<u64> = row.get(6).map_err(|e| format!("{e}"))?;
                SearchEntry {
                    id,
                    parent_id,
                    name,
                    name_folded,
                    is_directory,
                    size,
                    modified_at,
                }
            };

            #[cfg(not(target_os = "macos"))]
            let entry = {
                let id: i64 = row.get(0).map_err(|e| format!("{e}"))?;
                let parent_id: i64 = row.get(1).map_err(|e| format!("{e}"))?;
                let name: String = row.get(2).map_err(|e| format!("{e}"))?;
                let is_directory: bool = row.get(3).map_err(|e| format!("{e}"))?;
                let size: Option<u64> = row.get(4).map_err(|e| format!("{e}"))?;
                let modified_at: Option<u64> = row.get(5).map_err(|e| format!("{e}"))?;
                let name_folded = store::normalize_for_comparison(&name);
                SearchEntry {
                    id,
                    parent_id,
                    name,
                    name_folded,
                    is_directory,
                    size,
                    modified_at,
                }
            };

            let idx = entries.len();
            id_to_index.insert(entry.id, idx);
            entries.push(entry);
            row_count += 1;
        }

        log::debug!("Search index loaded: {row_count} entries, generation {generation}, took {:?}", t.elapsed());
        Ok(SearchIndex {
            entries,
            id_to_index,
            generation,
        })
    })?
}

// ── Query types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub name_pattern: Option<String>,
    #[serde(default)]
    pub pattern_type: PatternType,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<u64>,
    pub modified_before: Option<u64>,
    pub is_directory: Option<bool>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    30
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PatternType {
    #[default]
    Glob,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub entries: Vec<SearchResultEntry>,
    pub total_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultEntry {
    pub name: String,
    pub path: String,
    pub parent_path: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
    pub icon_id: String,
    /// Internal entry ID for batch dir_stats lookup. Not sent to frontend.
    #[serde(skip)]
    pub entry_id: i64,
}

// ── Glob to regex conversion ─────────────────────────────────────────

/// Convert a glob pattern to a regex pattern.
///
/// Escapes regex metacharacters, converts `*` to `.*` and `?` to `.`,
/// wraps in `^...$` for full-match semantics.
pub fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::with_capacity(glob.len() * 2 + 2);
    regex.push('^');
    for c in glob.chars() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' | '+' | '(' | ')' | '{' | '}' | '[' | ']' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
    }
    regex.push('$');
    regex
}

// ── Search execution ─────────────────────────────────────────────────

/// Execute a search query against the in-memory index. Pure function.
pub fn search(index: &SearchIndex, query: &SearchQuery) -> Result<SearchResult, String> {
    let t = std::time::Instant::now();
    // Compile pattern
    let compiled_pattern = match &query.name_pattern {
        Some(pattern) if !pattern.is_empty() => {
            let regex_str = match query.pattern_type {
                PatternType::Glob => glob_to_regex(pattern),
                PatternType::Regex => pattern.clone(),
            };
            let re = RegexBuilder::new(&regex_str)
                .case_insensitive(cfg!(target_os = "macos"))
                .build()
                .map_err(|e| format!("Invalid pattern: {e}"))?;
            Some(re)
        }
        _ => None,
    };

    // Parallel scan: collect matching indices
    let matching_indices: Vec<usize> = index
        .entries
        .par_iter()
        .enumerate()
        .filter(|(_, entry)| {
            // Skip root sentinel
            if entry.id == ROOT_ID {
                return false;
            }

            // Name pattern filter
            if let Some(ref re) = compiled_pattern
                && !re.is_match(&entry.name_folded)
            {
                return false;
            }

            // Directory filter
            if let Some(is_dir) = query.is_directory
                && entry.is_directory != is_dir
            {
                return false;
            }

            // Size filters (for files only; directories get sizes later)
            if !entry.is_directory {
                if let Some(min) = query.min_size {
                    match entry.size {
                        Some(s) if s >= min => {}
                        _ => return false,
                    }
                }
                if let Some(max) = query.max_size {
                    match entry.size {
                        Some(s) if s <= max => {}
                        _ => return false,
                    }
                }
            }

            // Date filters
            if let Some(after) = query.modified_after {
                match entry.modified_at {
                    Some(t) if t >= after => {}
                    _ => return false,
                }
            }
            if let Some(before) = query.modified_before {
                match entry.modified_at {
                    Some(t) if t <= before => {}
                    _ => return false,
                }
            }

            true
        })
        .map(|(i, _)| i)
        .collect();

    let total_count = matching_indices.len() as u32;

    // Sort by recency (most recently modified first)
    let mut sorted = matching_indices;
    sorted.sort_unstable_by(|&a, &b| {
        let ma = index.entries[a].modified_at.unwrap_or(0);
        let mb = index.entries[b].modified_at.unwrap_or(0);
        mb.cmp(&ma)
    });

    // Take first `limit` entries. When size filters are active and directories
    // are included, collect extra candidates because some directories may be
    // filtered out later in fill_directory_sizes (directory sizes come from
    // dir_stats, not the entries table).
    let base_limit = query.limit.min(1000) as usize;
    let has_size_filter = query.min_size.is_some() || query.max_size.is_some();
    let dirs_included = query.is_directory != Some(false);
    let limit = if has_size_filter && dirs_included {
        (base_limit * 3).max(base_limit + 100)
    } else {
        base_limit
    };
    sorted.truncate(limit);

    // Reconstruct paths and build result entries
    let home_dir = dirs::home_dir().map(|p| p.to_string_lossy().to_string());
    let entries: Vec<SearchResultEntry> = sorted
        .iter()
        .map(|&idx| {
            let entry = &index.entries[idx];
            let path = reconstruct_path_from_index(index, entry.id);
            let parent_path = match path.rfind('/') {
                Some(0) => "/".to_string(),
                Some(pos) => {
                    let parent = &path[..pos];
                    // Replace home dir prefix with ~
                    if let Some(ref home) = home_dir {
                        if let Some(rest) = parent.strip_prefix(home.as_str()) {
                            format!("~{rest}")
                        } else {
                            parent.to_string()
                        }
                    } else {
                        parent.to_string()
                    }
                }
                None => path.clone(),
            };
            let icon_id = derive_icon_id(&entry.name, entry.is_directory);
            SearchResultEntry {
                name: entry.name.clone(),
                path,
                parent_path,
                is_directory: entry.is_directory,
                size: entry.size,
                modified_at: entry.modified_at,
                icon_id,
                entry_id: entry.id,
            }
        })
        .collect();

    log::debug!("Search completed: {} matches (returning {}), took {:?}", total_count, entries.len(), t.elapsed());
    Ok(SearchResult { entries, total_count })
}

/// Fetch directory sizes for directory entries in the search results.
/// Mutates the result entries in place, setting `size` for directories.
/// Uses batch lookup via entry IDs stored in `SearchResultEntry`.
pub fn fill_directory_sizes(result: &mut SearchResult, pool: &ReadPool) {
    let dir_indices: Vec<usize> = result
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_directory)
        .map(|(i, _)| i)
        .collect();

    if dir_indices.is_empty() {
        return;
    }

    let t = std::time::Instant::now();
    let entry_ids: Vec<i64> = dir_indices.iter().map(|&idx| result.entries[idx].entry_id).collect();

    let _ = pool.with_conn(|conn| {
        if let Ok(stats_batch) = IndexStore::get_dir_stats_batch_by_ids(conn, &entry_ids) {
            for (i, &idx) in dir_indices.iter().enumerate() {
                if let Some(Some(stats)) = stats_batch.get(i) {
                    result.entries[idx].size = Some(stats.recursive_size);
                }
            }
        }
    });
    log::debug!("Filled directory sizes for {} dirs, took {:?}", dir_indices.len(), t.elapsed());
}

// ── Path reconstruction ──────────────────────────────────────────────

/// Reconstruct the full path for an entry by walking the parent_id chain
/// in the in-memory index. O(depth) per entry.
fn reconstruct_path_from_index(index: &SearchIndex, entry_id: i64) -> String {
    if entry_id == ROOT_ID {
        return "/".to_string();
    }

    let mut components = Vec::new();
    let mut current_id = entry_id;

    loop {
        if current_id == ROOT_ID || current_id == 0 {
            break;
        }
        match index.id_to_index.get(&current_id) {
            Some(&idx) => {
                let entry = &index.entries[idx];
                if entry.name.is_empty() {
                    break; // root sentinel
                }
                components.push(entry.name.as_str());
                current_id = entry.parent_id;
            }
            None => break, // orphan or missing parent
        }
    }

    components.reverse();
    format!("/{}", components.join("/"))
}

/// Derive an icon ID from filename and directory flag.
fn derive_icon_id(name: &str, is_directory: bool) -> String {
    if is_directory {
        return "dir".to_string();
    }
    match name.rfind('.') {
        Some(pos) if pos > 0 => {
            let ext = &name[pos + 1..];
            if ext.is_empty() {
                "file".to_string()
            } else {
                format!("ext:{}", ext.to_lowercase())
            }
        }
        _ => "file".to_string(),
    }
}

// ── Backstop timer ───────────────────────────────────────────────────

/// Start the backstop timer. Drops the index if no search activity within `BACKSTOP_TIMEOUT`.
pub(crate) fn start_backstop_timer() -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(BACKSTOP_TIMEOUT).await;
            let last = LAST_SEARCH_ACTIVITY.load(Ordering::Relaxed);
            let elapsed = now_secs().saturating_sub(last);
            if elapsed >= BACKSTOP_TIMEOUT.as_secs() {
                if DIALOG_OPEN.load(Ordering::Relaxed) {
                    log::debug!("Search index backstop timer deferred, dialog still open");
                    continue;
                }
                log::debug!("Search index backstop timeout reached, dropping index");
                drop_search_index();
                break;
            }
            // Activity happened recently — loop and check again
        }
    })
}

/// Start the idle timer (5 min). Called when the search dialog closes.
pub(crate) fn start_idle_timer() -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(IDLE_TIMEOUT).await;
            if DIALOG_OPEN.load(Ordering::Relaxed) {
                log::debug!("Search index idle timer deferred, dialog still open");
                continue;
            }
            log::debug!("Search index idle timeout reached, dropping index");
            drop_search_index();
            break;
        }
    })
}

/// Drop the search index and cancel any timers.
pub(crate) fn drop_search_index() {
    let mut guard = match SEARCH_INDEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if let Some(state) = guard.take() {
        if let Some(h) = state.idle_timer {
            h.abort();
        }
        if let Some(h) = state.backstop_timer {
            h.abort();
        }
        log::debug!("Search index dropped");
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── glob_to_regex ────────────────────────────────────────────────

    #[test]
    fn glob_to_regex_star() {
        assert_eq!(glob_to_regex("*.pdf"), r"^.*\.pdf$");
    }

    #[test]
    fn glob_to_regex_question() {
        assert_eq!(glob_to_regex("file?.txt"), r"^file.\.txt$");
    }

    #[test]
    fn glob_to_regex_escapes_metacharacters() {
        assert_eq!(glob_to_regex("a+b(c)"), r"^a\+b\(c\)$");
    }

    #[test]
    fn glob_to_regex_literal() {
        assert_eq!(glob_to_regex("readme"), "^readme$");
    }

    // ── Helper: build a small in-memory index ────────────────────────

    fn make_test_index() -> SearchIndex {
        let entries = vec![
            SearchEntry {
                id: 1,
                parent_id: 0,
                name: String::new(),
                name_folded: String::new(),
                is_directory: true,
                size: None,
                modified_at: None,
            },
            SearchEntry {
                id: 2,
                parent_id: 1,
                name: "Users".to_string(),
                name_folded: store::normalize_for_comparison("Users"),
                is_directory: true,
                size: None,
                modified_at: Some(1000),
            },
            SearchEntry {
                id: 3,
                parent_id: 2,
                name: "alice".to_string(),
                name_folded: store::normalize_for_comparison("alice"),
                is_directory: true,
                size: None,
                modified_at: Some(2000),
            },
            SearchEntry {
                id: 4,
                parent_id: 3,
                name: "report.pdf".to_string(),
                name_folded: store::normalize_for_comparison("report.pdf"),
                is_directory: false,
                size: Some(1_000_000),
                modified_at: Some(3000),
            },
            SearchEntry {
                id: 5,
                parent_id: 3,
                name: "photo.jpg".to_string(),
                name_folded: store::normalize_for_comparison("photo.jpg"),
                is_directory: false,
                size: Some(5_000_000),
                modified_at: Some(4000),
            },
            SearchEntry {
                id: 6,
                parent_id: 3,
                name: "notes.txt".to_string(),
                name_folded: store::normalize_for_comparison("notes.txt"),
                is_directory: false,
                size: Some(500),
                modified_at: Some(5000),
            },
            SearchEntry {
                id: 7,
                parent_id: 2,
                name: "Documents".to_string(),
                name_folded: store::normalize_for_comparison("Documents"),
                is_directory: true,
                size: None,
                modified_at: Some(1500),
            },
            SearchEntry {
                id: 8,
                parent_id: 7,
                name: "Q1-report.pdf".to_string(),
                name_folded: store::normalize_for_comparison("Q1-report.pdf"),
                is_directory: false,
                size: Some(2_000_000),
                modified_at: Some(6000),
            },
        ];
        let mut id_to_index = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            id_to_index.insert(e.id, i);
        }
        SearchIndex {
            entries,
            id_to_index,
            generation: 1,
        }
    }

    // ── Glob matching ────────────────────────────────────────────────

    #[test]
    fn search_glob_star_pdf() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: Some("*.pdf".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        assert_eq!(result.total_count, 2);
        assert!(result.entries.iter().all(|e| e.name.ends_with(".pdf")));
    }

    #[test]
    fn search_glob_question_mark() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: Some("Q?-report.pdf".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "Q1-report.pdf");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn search_glob_case_insensitive_macos() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: Some("REPORT.PDF".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        // On macOS, matching is case-insensitive
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "report.pdf");
    }

    // ── Regex matching ───────────────────────────────────────────────

    #[test]
    fn search_regex_alternation() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: Some(r"Q[1-4].*\.pdf".to_string()),
            pattern_type: PatternType::Regex,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "Q1-report.pdf");
    }

    #[test]
    fn search_invalid_regex() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: Some("[unclosed".to_string()),
            pattern_type: PatternType::Regex,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid pattern"));
    }

    // ── Size filters ─────────────────────────────────────────────────

    #[test]
    fn search_min_size() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: Some(2_000_000),
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: Some(false),
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        // photo.jpg (5M) and Q1-report.pdf (2M)
        assert_eq!(result.total_count, 2);
        assert!(result.entries.iter().all(|e| e.size.unwrap() >= 2_000_000));
    }

    #[test]
    fn search_max_size() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: Some(1000),
            modified_after: None,
            modified_before: None,
            is_directory: Some(false),
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "notes.txt");
    }

    #[test]
    fn search_size_range() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: Some(500_000),
            max_size: Some(3_000_000),
            modified_after: None,
            modified_before: None,
            is_directory: Some(false),
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        // report.pdf (1M) and Q1-report.pdf (2M)
        assert_eq!(result.total_count, 2);
    }

    // ── Date filters ─────────────────────────────────────────────────

    #[test]
    fn search_modified_after() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: Some(4000),
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        // photo.jpg (4000), notes.txt (5000), Q1-report.pdf (6000)
        assert_eq!(result.total_count, 3);
    }

    #[test]
    fn search_modified_before() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: Some(2000),
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        // Users (1000), alice (2000), Documents (1500)
        assert_eq!(result.total_count, 3);
    }

    #[test]
    fn search_date_range() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: Some(3000),
            modified_before: Some(5000),
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        // report.pdf (3000), photo.jpg (4000), notes.txt (5000)
        assert_eq!(result.total_count, 3);
    }

    // ── Combined filters ─────────────────────────────────────────────

    #[test]
    fn search_combined_name_and_size() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: Some("*.pdf".to_string()),
            pattern_type: PatternType::Glob,
            min_size: Some(1_500_000),
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "Q1-report.pdf");
    }

    // ── Empty query (returns all by recency) ─────────────────────────

    #[test]
    fn search_empty_query_returns_by_recency() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        // All entries except root sentinel (7 entries)
        assert_eq!(result.total_count, 7);
        // First result should be most recent (Q1-report.pdf, modified_at=6000)
        assert_eq!(result.entries[0].name, "Q1-report.pdf");
    }

    // ── Limit and total_count ────────────────────────────────────────

    #[test]
    fn search_limit_and_total_count() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            limit: 3,
        };
        let result = search(&index, &query).unwrap();
        assert_eq!(result.entries.len(), 3);
        assert_eq!(result.total_count, 7); // total matches, not limited
    }

    // ── Directory filter ─────────────────────────────────────────────

    #[test]
    fn search_directories_only() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: Some(true),
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        // Users, alice, Documents (root excluded)
        assert_eq!(result.total_count, 3);
        assert!(result.entries.iter().all(|e| e.is_directory));
    }

    #[test]
    fn search_files_only() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: Some(false),
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        assert_eq!(result.total_count, 4);
        assert!(result.entries.iter().all(|e| !e.is_directory));
    }

    // ── Path reconstruction ──────────────────────────────────────────

    #[test]
    fn path_reconstruction() {
        let index = make_test_index();
        let path = reconstruct_path_from_index(&index, 4); // report.pdf
        assert_eq!(path, "/Users/alice/report.pdf");
    }

    #[test]
    fn path_reconstruction_root() {
        let index = make_test_index();
        let path = reconstruct_path_from_index(&index, 1);
        assert_eq!(path, "/");
    }

    #[test]
    fn path_reconstruction_top_level_dir() {
        let index = make_test_index();
        let path = reconstruct_path_from_index(&index, 2); // Users
        assert_eq!(path, "/Users");
    }

    // ── Icon ID derivation ───────────────────────────────────────────

    #[test]
    fn icon_id_directory() {
        assert_eq!(derive_icon_id("Documents", true), "dir");
    }

    #[test]
    fn icon_id_file_with_extension() {
        assert_eq!(derive_icon_id("report.pdf", false), "ext:pdf");
    }

    #[test]
    fn icon_id_file_without_extension() {
        assert_eq!(derive_icon_id("Makefile", false), "file");
    }

    #[test]
    fn icon_id_uppercase_extension() {
        assert_eq!(derive_icon_id("Photo.JPG", false), "ext:jpg");
    }

    // ── Serde round-trip ─────────────────────────────────────────────

    #[test]
    fn serde_roundtrip_search_query() {
        let query = SearchQuery {
            name_pattern: Some("*.pdf".to_string()),
            pattern_type: PatternType::Glob,
            min_size: Some(1024),
            max_size: None,
            modified_after: Some(1000),
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("namePattern"));
        assert!(json.contains("patternType"));
        assert!(json.contains("minSize"));
        assert!(json.contains("modifiedAfter"));

        let deserialized: SearchQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name_pattern, Some("*.pdf".to_string()));
        assert_eq!(deserialized.pattern_type, PatternType::Glob);
        assert_eq!(deserialized.min_size, Some(1024));
        assert_eq!(deserialized.max_size, None);
    }

    #[test]
    fn serde_roundtrip_search_result() {
        let result = SearchResult {
            entries: vec![SearchResultEntry {
                name: "test.pdf".to_string(),
                path: "/Users/alice/test.pdf".to_string(),
                parent_path: "~/alice".to_string(),
                is_directory: false,
                size: Some(1024),
                modified_at: Some(1000),
                icon_id: "ext:pdf".to_string(),
                entry_id: 42,
            }],
            total_count: 1,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("totalCount"));
        assert!(json.contains("isDirectory"));
        assert!(json.contains("modifiedAt"));
        assert!(json.contains("iconId"));
        assert!(json.contains("parentPath"));
        // entry_id is #[serde(skip)] — must not appear in JSON
        assert!(!json.contains("entryId"));

        let deserialized: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_count, 1);
        assert_eq!(deserialized.entries[0].name, "test.pdf");
    }

    #[test]
    fn serde_query_optional_fields_null() {
        let json = r#"{"limit":30}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.name_pattern, None);
        assert_eq!(query.pattern_type, PatternType::Glob);
        assert_eq!(query.min_size, None);
        assert_eq!(query.limit, 30);
    }

    #[test]
    fn serde_query_camel_case_from_frontend() {
        let json = r#"{"namePattern":"*.pdf","patternType":"glob","minSize":1024,"maxSize":null,"modifiedAfter":null,"modifiedBefore":null,"isDirectory":false,"limit":50}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.name_pattern, Some("*.pdf".to_string()));
        assert_eq!(query.min_size, Some(1024));
        assert_eq!(query.is_directory, Some(false));
        assert_eq!(query.limit, 50);
    }

    // ── Integration test: load from real SQLite DB ───────────────────

    #[test]
    fn integration_load_and_search() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Insert test entries
        let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();
        let alice_id = IndexStore::insert_entry_v2(&conn, users_id, "alice", true, false, None, None).unwrap();
        let _pdf_id = IndexStore::insert_entry_v2(
            &conn,
            alice_id,
            "report.pdf",
            false,
            false,
            Some(1_000_000),
            Some(1700000000),
        )
        .unwrap();
        let _txt_id =
            IndexStore::insert_entry_v2(&conn, alice_id, "notes.txt", false, false, Some(500), Some(1700000100))
                .unwrap();

        // Load the index using ReadPool
        let pool = ReadPool::new(db_path).unwrap();
        let cancel = AtomicBool::new(false);
        let index = load_search_index(&pool, &cancel).unwrap();

        // Root sentinel + 4 entries
        assert_eq!(index.entries.len(), 5);
        assert_eq!(index.id_to_index.len(), 5);

        // Search for PDFs
        let query = SearchQuery {
            name_pattern: Some("*.pdf".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            limit: 30,
        };
        let result = search(&index, &query).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "report.pdf");
        assert_eq!(result.entries[0].path, "/Users/alice/report.pdf");
    }

    #[test]
    fn load_cancellation() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");

        let pool = ReadPool::new(db_path).unwrap();
        let cancel = AtomicBool::new(true); // Pre-cancelled
        let result = load_search_index(&pool, &cancel);
        // With only the root sentinel, cancellation check happens at row 0, but CANCEL_CHECK_INTERVAL
        // is 100K so the first check is at row 0 (0 % 100K == 0). The load should be cancelled.
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cancelled"));
    }
}
