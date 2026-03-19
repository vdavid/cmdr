//! In-memory search index and search execution.
//!
//! Loads all entries from the index DB into a `Vec<SearchEntry>` for fast
//! parallel scanning with rayon. The index is loaded lazily when the search
//! dialog opens and dropped after an idle timeout.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use rayon::prelude::*;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use super::enrichment::ReadPool;
use super::store::{self, IndexStore, ROOT_ID};
use super::writer::WRITER_GENERATION;

// ── Search entry (in-memory representation) ──────────────────────────

#[derive(Debug)]
pub struct SearchEntry {
    pub id: i64,
    pub parent_id: i64,
    pub name_offset: u32, // byte offset into SearchIndex.names
    pub name_len: u16,    // byte length (max filename 255 chars = up to 765 bytes UTF-8)
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
}

// ── Search index ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SearchIndex {
    pub names: String, // arena: all filenames concatenated
    pub entries: Vec<SearchEntry>,
    pub id_to_index: HashMap<i64, usize>,
    /// Lazily-built lookup for O(1) scope path resolution.
    /// Built on first scoped search (adds ~20s for 5M entries), then cached.
    parent_name_lookup: OnceLock<HashMap<(i64, String), i64>>,
    pub generation: u64,
}

impl SearchIndex {
    /// Empty sentinel index used during async load.
    pub fn empty() -> Self {
        Self {
            names: String::new(),
            entries: Vec::new(),
            id_to_index: HashMap::new(),
            parent_name_lookup: OnceLock::new(),
            generation: 0,
        }
    }

    /// Get the filename for an entry from the arena buffer.
    fn name(&self, entry: &SearchEntry) -> &str {
        &self.names[entry.name_offset as usize..entry.name_offset as usize + entry.name_len as usize]
    }

    /// Get or build the parent-name lookup table for scope path resolution.
    fn parent_name_lookup(&self) -> &HashMap<(i64, String), i64> {
        self.parent_name_lookup.get_or_init(|| {
            let t = std::time::Instant::now();
            let map = build_parent_name_lookup(&self.entries, &self.names);
            log::info!(
                "Search index: parent_name_lookup built in {:.1}s ({} entries)",
                t.elapsed().as_secs_f64(),
                map.len()
            );
            map
        })
    }
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
/// `name_folded` is NOT loaded — the search pattern is normalized instead
/// (NFD on macOS) to avoid ~5.1M extra String allocations and ~300 MB of memory.
pub fn load_search_index(pool: &ReadPool, cancel: &AtomicBool) -> Result<SearchIndex, String> {
    pool.with_conn(|conn| {
        let t = std::time::Instant::now();
        let generation = WRITER_GENERATION.load(Ordering::Relaxed);

        let sql = "SELECT id, parent_id, name, is_directory, size, modified_at FROM entries";

        let mut stmt = conn.prepare(sql).map_err(|e| format!("Prepare failed: {e}"))?;

        // Phase 1: Load all entries into Vec (sequential writes to contiguous memory)
        // Arena-allocate all filenames into a single String to avoid per-entry heap allocations.
        let mut names = String::with_capacity(100_000_000); // ~5M entries × ~20 bytes avg
        let mut entries = Vec::with_capacity(5_000_000);

        let mut rows = stmt.query([]).map_err(|e| format!("Query failed: {e}"))?;
        let mut row_count = 0usize;

        while let Some(row) = rows.next().map_err(|e| format!("Row read failed: {e}"))? {
            if row_count.is_multiple_of(CANCEL_CHECK_INTERVAL) && cancel.load(Ordering::Relaxed) {
                return Err("Load cancelled".to_string());
            }

            let id: i64 = row.get(0).map_err(|e| format!("{e}"))?;
            let parent_id: i64 = row.get(1).map_err(|e| format!("{e}"))?;
            // Borrow directly from SQLite's internal buffer via ValueRef — zero heap allocations.
            let name_ref = row.get_ref(2).map_err(|e| format!("{e}"))?;
            let name_str = name_ref.as_str().map_err(|e| format!("{e}"))?;
            let name_offset = names.len() as u32;
            let name_len = name_str.len() as u16;
            names.push_str(name_str);
            let is_directory: bool = row.get(3).map_err(|e| format!("{e}"))?;
            let size: Option<u64> = row.get(4).map_err(|e| format!("{e}"))?;
            let modified_at: Option<u64> = row.get(5).map_err(|e| format!("{e}"))?;
            entries.push(SearchEntry {
                id,
                parent_id,
                name_offset,
                name_len,
                is_directory,
                size,
                modified_at,
            });
            row_count += 1;
        }

        // Phase 2: Build id_to_index from completed Vec (sequential reads + HashMap writes)
        let mut id_to_index = HashMap::with_capacity(entries.len());
        for (i, entry) in entries.iter().enumerate() {
            id_to_index.insert(entry.id, i);
        }

        log::debug!(
            "Search index loaded: {row_count} entries, generation {generation}, took {:?}",
            t.elapsed()
        );
        Ok(SearchIndex {
            names,
            entries,
            id_to_index,
            parent_name_lookup: OnceLock::new(),
            generation,
        })
    })?
}

// ── System directory exclusions ──────────────────────────────────────

/// Common system, build, and cache directory names excluded by default.
/// Applied automatically when `SearchQuery::exclude_system_dirs` is not `Some(false)`.
pub const SYSTEM_DIR_EXCLUDES: &[&str] = &[
    // Package managers & build tools
    "node_modules",
    ".pnpm-store",
    ".npm",
    ".yarn",
    ".cargo",
    ".m2",
    ".gradle",
    // VCS
    ".git",
    ".svn",
    ".hg",
    // Python
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    // JS/TS build output
    "build",
    "dist",
    ".next",
    ".nuxt",
    ".cache",
    ".parcel-cache",
    "target",
    // macOS system & caches
    "Caches",
    "CacheStorage",
    "Cache",
    "GPUCache",
    "ScriptCache",
    "GrShaderCache",
    "ShaderCache",
    "Logs",
    "Cookies",
    "WebKit",
    "Saved Application State",
    ".Trash",
    ".Spotlight-V100",
    ".fseventsd",
    ".DocumentRevisions-V100",
    // IDE workspace caches
    "workspaceStorage",
    "DerivedData",
];

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
    #[serde(default)]
    pub include_paths: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_dir_names: Option<Vec<String>>,
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Per-query case sensitivity override.
    /// `None` = platform default (false on macOS, true on Linux).
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    /// Whether to exclude common system/build/cache directories.
    /// `None` or `Some(true)` = exclude, `Some(false)` = include everything.
    #[serde(default)]
    pub exclude_system_dirs: Option<bool>,
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

// ── Query summary ────────────────────────────────────────────────────

/// Build a dense, human-readable summary of a `SearchQuery` for logging and display.
///
/// Examples: `"tes"`, `"*.pdf", dirs only`, `size >= 2 MB, last mod before 2026-03-01`
pub fn summarize_query(query: &SearchQuery) -> String {
    let mut parts = Vec::new();

    // Name pattern
    if let Some(ref pattern) = query.name_pattern
        && !pattern.is_empty()
    {
        let suffix = if query.pattern_type == PatternType::Regex {
            " (regex)"
        } else {
            ""
        };
        parts.push(format!("\"{pattern}\"{suffix}"));
    }

    // Size filters
    match (query.min_size, query.max_size) {
        (Some(min), Some(max)) => parts.push(format!("size {}–{}", format_size(min), format_size(max))),
        (Some(min), None) => parts.push(format!("size >= {}", format_size(min))),
        (None, Some(max)) => parts.push(format!("size <= {}", format_size(max))),
        (None, None) => {}
    }

    // Date filters
    match (query.modified_after, query.modified_before) {
        (Some(after), Some(before)) => {
            parts.push(format!(
                "last mod {}–{}",
                format_timestamp(after),
                format_timestamp(before)
            ));
        }
        (Some(after), None) => parts.push(format!("last mod after {}", format_timestamp(after))),
        (None, Some(before)) => parts.push(format!("last mod before {}", format_timestamp(before))),
        (None, None) => {}
    }

    // Directory filter
    match query.is_directory {
        Some(true) => parts.push("dirs only".to_string()),
        Some(false) => parts.push("files only".to_string()),
        None => {}
    }

    // Case sensitivity (only show when explicitly set)
    match query.case_sensitive {
        Some(true) => parts.push("case-sensitive".to_string()),
        Some(false) => parts.push("case-insensitive".to_string()),
        None => {}
    }

    if parts.is_empty() {
        "(all entries)".to_string()
    } else {
        parts.join(", ")
    }
}

pub(crate) fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;
    const TB: u64 = 1_024 * GB;

    if bytes >= TB {
        let val = bytes as f64 / TB as f64;
        if val.fract() == 0.0 {
            format!("{} TB", val as u64)
        } else {
            format!("{val:.1} TB")
        }
    } else if bytes >= GB {
        let val = bytes as f64 / GB as f64;
        if val.fract() == 0.0 {
            format!("{} GB", val as u64)
        } else {
            format!("{val:.1} GB")
        }
    } else if bytes >= MB {
        let val = bytes as f64 / MB as f64;
        if val.fract() == 0.0 {
            format!("{} MB", val as u64)
        } else {
            format!("{val:.1} MB")
        }
    } else if bytes >= KB {
        let val = bytes as f64 / KB as f64;
        if val.fract() == 0.0 {
            format!("{} KB", val as u64)
        } else {
            format!("{val:.1} KB")
        }
    } else {
        format!("{bytes} B")
    }
}

pub(crate) fn format_timestamp(ts: u64) -> String {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    time::OffsetDateTime::from_unix_timestamp(ts as i64)
        .map(|dt| dt.format(&format).unwrap_or_else(|_| ts.to_string()))
        .unwrap_or_else(|_| ts.to_string())
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

// ── Scope parsing ────────────────────────────────────────────────────

/// Parsed search scope: which subtrees to include and which directory names/paths to exclude.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedScope {
    pub include_paths: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

/// Parse a comma-separated scope string into include paths and exclude patterns.
///
/// Syntax: `~/projects, !node_modules, !.git`
/// - `~` expands to the user's home directory
/// - `!` prefix means exclude
/// - Quoted segments (single or double quotes) and backslash-escaped commas are supported
pub fn parse_scope(input: &str) -> ParsedScope {
    let segments = split_scope_segments(input);
    let home = dirs::home_dir().map(|p| p.to_string_lossy().to_string());

    let mut include_paths = Vec::new();
    let mut exclude_patterns = Vec::new();

    for seg in segments {
        let trimmed = seg.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (is_exclude, value) = if let Some(rest) = trimmed.strip_prefix('!') {
            (true, rest.trim())
        } else {
            (false, trimmed)
        };

        // Expand ~ prefix
        let expanded = if let Some(rest) = value.strip_prefix('~') {
            if let Some(ref h) = home {
                format!("{h}{rest}")
            } else {
                value.to_string()
            }
        } else {
            value.to_string()
        };

        if is_exclude {
            exclude_patterns.push(expanded);
        } else {
            include_paths.push(expanded);
        }
    }

    ParsedScope {
        include_paths,
        exclude_patterns,
    }
}

/// Split a scope string on commas, respecting quoting and backslash escapes.
fn split_scope_segments(input: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quote: Option<char> = None;

    while let Some(c) = chars.next() {
        match c {
            '\\' if in_quote.is_none() => {
                // Backslash-escaped character: consume next char literally
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '"' | '\'' if in_quote.is_none() => {
                in_quote = Some(c);
            }
            q if in_quote == Some(q) => {
                in_quote = None;
            }
            ',' if in_quote.is_none() => {
                segments.push(current.clone());
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }
    segments.push(current);
    segments
}

// ── Scope filter (pre-resolved for the hot loop) ─────────────────────

/// Pre-resolved scope filter for efficient ancestor-walk filtering during search.
struct ScopeFilter {
    /// Entry IDs that represent the include path roots. An entry passes if
    /// any of its ancestors (including itself) is in this set.
    include_ids: Option<HashSet<i64>>,
    /// Exact directory names to exclude (O(1) HashSet lookup per ancestor level).
    /// Stored normalized when `case_insensitive` is true.
    exclude_exact_names: HashSet<String>,
    /// Whether exclude name matching is case-insensitive.
    case_insensitive: bool,
    /// Compiled regex patterns for glob-based directory name exclusion.
    /// Only used for user-specified patterns containing wildcards (* or ?).
    exclude_name_patterns: Vec<Regex>,
    /// Absolute path prefixes for path-based exclusion.
    exclude_path_prefixes: Vec<String>,
}

impl ScopeFilter {
    fn is_active(&self) -> bool {
        self.include_ids.is_some()
            || !self.exclude_exact_names.is_empty()
            || !self.exclude_name_patterns.is_empty()
            || !self.exclude_path_prefixes.is_empty()
    }

    /// Check if an entry at `entry_idx` passes the scope filter by walking
    /// the ancestor chain in the in-memory index.
    fn matches(&self, index: &SearchIndex, entry_idx: usize) -> bool {
        let entry = &index.entries[entry_idx];

        // Include check: walk ancestors and check if any is in include_ids
        if let Some(ref ids) = self.include_ids {
            let mut found = false;
            let mut current_id = entry.id;
            loop {
                if ids.contains(&current_id) {
                    found = true;
                    break;
                }
                if current_id == ROOT_ID || current_id == 0 {
                    break;
                }
                match index.id_to_index.get(&current_id) {
                    Some(&idx) => current_id = index.entries[idx].parent_id,
                    None => break,
                }
            }
            if !found {
                return false;
            }
        }

        // Exclude check: walk ancestors, check each name against exclusions
        let has_name_excludes = !self.exclude_exact_names.is_empty() || !self.exclude_name_patterns.is_empty();
        if has_name_excludes || !self.exclude_path_prefixes.is_empty() {
            // For path-prefix excludes, reconstruct the path lazily
            if !self.exclude_path_prefixes.is_empty() {
                let path = reconstruct_path_from_index(index, entry.id);
                for prefix in &self.exclude_path_prefixes {
                    if path.starts_with(prefix.as_str()) {
                        return false;
                    }
                }
            }

            // For bare-name excludes, walk ancestors and check directory names
            if has_name_excludes {
                let mut current_id = entry.parent_id;
                loop {
                    if current_id == ROOT_ID || current_id == 0 {
                        break;
                    }
                    match index.id_to_index.get(&current_id) {
                        Some(&idx) => {
                            let ancestor = &index.entries[idx];
                            if ancestor.is_directory {
                                let name = index.name(ancestor);
                                // O(1) exact-name check (system dirs + simple user excludes)
                                if !self.exclude_exact_names.is_empty() {
                                    let excluded = if self.case_insensitive {
                                        self.exclude_exact_names
                                            .contains(&store::normalize_for_comparison(name))
                                    } else {
                                        self.exclude_exact_names.contains(name)
                                    };
                                    if excluded {
                                        return false;
                                    }
                                }
                                // Glob-pattern check (user wildcards only)
                                for pat in &self.exclude_name_patterns {
                                    if pat.is_match(name) {
                                        return false;
                                    }
                                }
                            }
                            current_id = ancestor.parent_id;
                        }
                        None => break,
                    }
                }
            }
        }

        true
    }
}

/// Pre-resolve scope filter data from the query and index.
fn prepare_scope_filter(query: &SearchQuery, index: &SearchIndex) -> ScopeFilter {
    // Resolve include paths to entry IDs using the cached lookup table
    let include_ids = query.include_paths.as_ref().and_then(|paths| {
        if paths.is_empty() {
            return None;
        }
        let mut ids = HashSet::new();
        for path in paths {
            if let Some(id) = resolve_path_in_index(path, index.parent_name_lookup()) {
                ids.insert(id);
            }
        }
        if ids.is_empty() {
            // No valid include paths resolved — include nothing
            // Use a set with an impossible ID to force all entries to fail
            ids.insert(i64::MIN);
        }
        Some(ids)
    });

    // Build exclude filters from user-specified patterns and system dir list
    let mut exclude_exact_names = HashSet::new();
    let mut exclude_name_patterns = Vec::new();
    let mut exclude_path_prefixes = Vec::new();

    let case_insensitive = match query.case_sensitive {
        Some(true) => false,
        Some(false) => true,
        None => cfg!(target_os = "macos"),
    };

    // User-specified excludes: wildcards → regex, plain names → exact HashSet
    if let Some(ref patterns) = query.exclude_dir_names {
        for pattern in patterns {
            if pattern.contains('/') {
                exclude_path_prefixes.push(pattern.clone());
            } else if pattern.contains('*') || pattern.contains('?') {
                let regex_str = glob_to_regex(pattern);
                if let Ok(re) = RegexBuilder::new(&regex_str).case_insensitive(case_insensitive).build() {
                    exclude_name_patterns.push(re);
                }
            } else if case_insensitive {
                exclude_exact_names.insert(store::normalize_for_comparison(pattern));
            } else {
                exclude_exact_names.insert(pattern.clone());
            }
        }
    }

    // System dir excludes (unless explicitly disabled)
    if query.exclude_system_dirs != Some(false) {
        for &name in SYSTEM_DIR_EXCLUDES {
            let key = if case_insensitive {
                store::normalize_for_comparison(name)
            } else {
                name.to_string()
            };
            exclude_exact_names.insert(key);
        }
    }

    ScopeFilter {
        include_ids,
        exclude_exact_names,
        case_insensitive,
        exclude_name_patterns,
        exclude_path_prefixes,
    }
}

/// Build a lookup table mapping `(parent_id, normalized_name) -> entry_id`
/// for O(1) path component resolution. Called once during index load.
fn build_parent_name_lookup(entries: &[SearchEntry], names: &str) -> HashMap<(i64, String), i64> {
    let mut map = HashMap::with_capacity(entries.len());
    for entry in entries {
        let name = &names[entry.name_offset as usize..entry.name_offset as usize + entry.name_len as usize];
        let normalized = store::normalize_for_comparison(name);
        map.insert((entry.parent_id, normalized), entry.id);
    }
    map
}

/// Resolve a filesystem path to an entry ID using a pre-built lookup table
/// for O(1) per component instead of scanning all entries.
fn resolve_path_in_index(path: &str, lookup: &HashMap<(i64, String), i64>) -> Option<i64> {
    let path = path.strip_prefix('/').unwrap_or(path);
    if path.is_empty() {
        return Some(ROOT_ID);
    }

    let mut current_id = ROOT_ID;
    for component in path.split('/') {
        if component.is_empty() {
            continue;
        }
        let normalized = store::normalize_for_comparison(component);
        current_id = *lookup.get(&(current_id, normalized))?;
    }
    Some(current_id)
}

// ── Search execution ─────────────────────────────────────────────────

/// Execute a search query against the in-memory index. Pure function.
pub fn search(index: &SearchIndex, query: &SearchQuery) -> Result<SearchResult, String> {
    let t = std::time::Instant::now();
    // Pre-resolve scope filter
    let scope_filter = prepare_scope_filter(query, index);

    // Compile pattern
    let compiled_pattern = match &query.name_pattern {
        Some(pattern) if !pattern.is_empty() => {
            // On macOS, NFD-normalize the pattern before conversion/compilation.
            // APFS filenames are stored in NFD, so matching a NFD-normalized pattern
            // against `name` with case_insensitive(true) gives correct results
            // without needing a separate `name_folded` field.
            #[cfg(target_os = "macos")]
            let pattern = {
                use unicode_normalization::UnicodeNormalization;
                pattern.nfd().collect::<String>()
            };
            let regex_str = match query.pattern_type {
                PatternType::Glob => {
                    // If the user typed a plain string without wildcards, wrap it
                    // in `*...*` so it behaves as a contains/substring match.
                    // This matches the UX of Total Commander, Double Commander,
                    // and most file-search dialogs: typing "tes" finds "test.rs".
                    let glob = if !pattern.contains('*') && !pattern.contains('?') {
                        format!("*{pattern}*")
                    } else {
                        pattern.to_string()
                    };
                    glob_to_regex(&glob)
                }
                PatternType::Regex => pattern.to_string(),
            };
            let case_insensitive = match query.case_sensitive {
                Some(true) => false,
                Some(false) => true,
                None => cfg!(target_os = "macos"),
            };
            let re = RegexBuilder::new(&regex_str)
                .case_insensitive(case_insensitive)
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
        .filter(|(i, entry)| {
            // Skip root sentinel
            if entry.id == ROOT_ID {
                return false;
            }

            // Name pattern filter
            if let Some(ref re) = compiled_pattern
                && !re.is_match(
                    &index.names[entry.name_offset as usize..entry.name_offset as usize + entry.name_len as usize],
                )
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

            // Scope filter (ancestor walk — only for entries passing all other filters)
            if scope_filter.is_active() && !scope_filter.matches(index, *i) {
                return false;
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
            let entry_name = index.name(entry);
            let icon_id = derive_icon_id(entry_name, entry.is_directory);
            SearchResultEntry {
                name: entry_name.to_string(),
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

    log::debug!(
        "Search completed: {} → {} matches (returning {}), took {:?}",
        summarize_query(query),
        total_count,
        entries.len(),
        t.elapsed()
    );
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
    log::debug!(
        "Filled directory sizes for {} dirs, took {:?}",
        dir_indices.len(),
        t.elapsed()
    );
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
                let name = index.name(entry);
                if name.is_empty() {
                    break; // root sentinel
                }
                components.push(name);
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

    // ── Wildcard-free glob auto-wrapping (contains match) ────────────

    #[test]
    fn search_glob_plain_text_matches_substring() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: Some("ote".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        // "ote" should match "notes.txt" as a substring
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "notes.txt");
    }

    #[test]
    fn search_glob_plain_text_matches_prefix() {
        let index = make_test_index();
        let query = SearchQuery {
            name_pattern: Some("repo".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        // "repo" should match "report.pdf" and "Q1-report.pdf"
        assert_eq!(result.total_count, 2);
        assert!(result.entries.iter().all(|e| e.name.contains("report")));
    }

    #[test]
    fn search_glob_with_wildcards_not_auto_wrapped() {
        let index = make_test_index();
        // Explicit glob with wildcard should NOT be auto-wrapped
        let query = SearchQuery {
            name_pattern: Some("report*".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        // "report*" matches "report.pdf" but NOT "Q1-report.pdf"
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "report.pdf");
    }

    // ── Helper: build a small in-memory index ────────────────────────

    /// Helper: push a name into the arena and return (offset, len).
    fn arena_push(names: &mut String, name: &str) -> (u32, u16) {
        let offset = names.len() as u32;
        let len = name.len() as u16;
        names.push_str(name);
        (offset, len)
    }

    fn make_test_index() -> SearchIndex {
        let mut names = String::new();
        let test_names = [
            "",
            "Users",
            "alice",
            "report.pdf",
            "photo.jpg",
            "notes.txt",
            "Documents",
            "Q1-report.pdf",
        ];
        let offsets: Vec<(u32, u16)> = test_names.iter().map(|n| arena_push(&mut names, n)).collect();

        let entries = vec![
            SearchEntry {
                id: 1,
                parent_id: 0,
                name_offset: offsets[0].0,
                name_len: offsets[0].1,
                is_directory: true,
                size: None,
                modified_at: None,
            },
            SearchEntry {
                id: 2,
                parent_id: 1,
                name_offset: offsets[1].0,
                name_len: offsets[1].1,
                is_directory: true,
                size: None,
                modified_at: Some(1000),
            },
            SearchEntry {
                id: 3,
                parent_id: 2,
                name_offset: offsets[2].0,
                name_len: offsets[2].1,
                is_directory: true,
                size: None,
                modified_at: Some(2000),
            },
            SearchEntry {
                id: 4,
                parent_id: 3,
                name_offset: offsets[3].0,
                name_len: offsets[3].1,
                is_directory: false,
                size: Some(1_000_000),
                modified_at: Some(3000),
            },
            SearchEntry {
                id: 5,
                parent_id: 3,
                name_offset: offsets[4].0,
                name_len: offsets[4].1,
                is_directory: false,
                size: Some(5_000_000),
                modified_at: Some(4000),
            },
            SearchEntry {
                id: 6,
                parent_id: 3,
                name_offset: offsets[5].0,
                name_len: offsets[5].1,
                is_directory: false,
                size: Some(500),
                modified_at: Some(5000),
            },
            SearchEntry {
                id: 7,
                parent_id: 2,
                name_offset: offsets[6].0,
                name_len: offsets[6].1,
                is_directory: true,
                size: None,
                modified_at: Some(1500),
            },
            SearchEntry {
                id: 8,
                parent_id: 7,
                name_offset: offsets[7].0,
                name_len: offsets[7].1,
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
            names,
            entries,
            id_to_index,
            parent_name_lookup: OnceLock::new(),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            // Use a wildcard pattern to test case-insensitivity specifically
            // (without wildcards, auto-wrapping would turn this into a contains match)
            name_pattern: Some("NOTES.*".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        // On macOS, matching is case-insensitive
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "notes.txt");
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 3,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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

    // ── summarize_query ──────────────────────────────────────────────

    fn make_query(
        name_pattern: Option<&str>,
        pattern_type: PatternType,
        min_size: Option<u64>,
        max_size: Option<u64>,
        modified_after: Option<u64>,
        modified_before: Option<u64>,
        is_directory: Option<bool>,
    ) -> SearchQuery {
        SearchQuery {
            name_pattern: name_pattern.map(|s| s.to_string()),
            pattern_type,
            min_size,
            max_size,
            modified_after,
            modified_before,
            is_directory,
            include_paths: None,
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        }
    }

    #[test]
    fn summarize_empty_query() {
        let q = make_query(None, PatternType::Glob, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "(all entries)");
    }

    #[test]
    fn summarize_name_only() {
        let q = make_query(Some("tes"), PatternType::Glob, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "\"tes\"");
    }

    #[test]
    fn summarize_glob_pattern() {
        let q = make_query(Some("*.pdf"), PatternType::Glob, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "\"*.pdf\"");
    }

    #[test]
    fn summarize_regex_pattern() {
        let q = make_query(Some("Q[1-4].*"), PatternType::Regex, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "\"Q[1-4].*\" (regex)");
    }

    #[test]
    fn summarize_size_min() {
        let q = make_query(None, PatternType::Glob, Some(2 * 1024 * 1024), None, None, None, None);
        assert_eq!(summarize_query(&q), "size >= 2 MB");
    }

    #[test]
    fn summarize_size_max() {
        let q = make_query(None, PatternType::Glob, None, Some(500 * 1024), None, None, None);
        assert_eq!(summarize_query(&q), "size <= 500 KB");
    }

    #[test]
    fn summarize_size_range() {
        let q = make_query(
            None,
            PatternType::Glob,
            Some(1024 * 1024),
            Some(5 * 1024 * 1024 * 1024),
            None,
            None,
            None,
        );
        assert_eq!(summarize_query(&q), "size 1 MB\u{2013}5 GB");
    }

    #[test]
    fn summarize_date_after() {
        // 2025-01-01 00:00:00 UTC = 1735689600
        let q = make_query(None, PatternType::Glob, None, None, Some(1_735_689_600), None, None);
        assert_eq!(summarize_query(&q), "last mod after 2025-01-01");
    }

    #[test]
    fn summarize_date_before() {
        // 2026-03-01 00:00:00 UTC = 1772265600
        let q = make_query(None, PatternType::Glob, None, None, None, Some(1_772_323_200), None);
        assert_eq!(summarize_query(&q), "last mod before 2026-03-01");
    }

    #[test]
    fn summarize_date_range() {
        let q = make_query(
            None,
            PatternType::Glob,
            None,
            None,
            Some(1_735_689_600),
            Some(1_772_323_200),
            None,
        );
        assert_eq!(summarize_query(&q), "last mod 2025-01-01\u{2013}2026-03-01");
    }

    #[test]
    fn summarize_dirs_only() {
        let q = make_query(Some("*.pdf"), PatternType::Glob, None, None, None, None, Some(true));
        assert_eq!(summarize_query(&q), "\"*.pdf\", dirs only");
    }

    #[test]
    fn summarize_files_only() {
        let q = make_query(None, PatternType::Glob, None, None, None, None, Some(false));
        assert_eq!(summarize_query(&q), "files only");
    }

    #[test]
    fn summarize_combined() {
        let q = make_query(
            Some("tes"),
            PatternType::Glob,
            Some(2 * 1024 * 1024),
            None,
            None,
            Some(1_772_323_200),
            None,
        );
        assert_eq!(summarize_query(&q), "\"tes\", size >= 2 MB, last mod before 2026-03-01");
    }

    #[test]
    fn summarize_size_bytes() {
        let q = make_query(None, PatternType::Glob, Some(500), None, None, None, None);
        assert_eq!(summarize_query(&q), "size >= 500 B");
    }

    #[test]
    fn summarize_size_gb() {
        let q = make_query(
            None,
            PatternType::Glob,
            Some(1024 * 1024 * 1024),
            None,
            None,
            None,
            None,
        );
        assert_eq!(summarize_query(&q), "size >= 1 GB");
    }

    #[test]
    fn summarize_empty_name_pattern() {
        let q = make_query(Some(""), PatternType::Glob, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "(all entries)");
    }

    // ── parse_scope ─────────────────────────────────────────────────

    #[test]
    fn parse_scope_basic_include() {
        let scope = parse_scope("~/projects");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
        assert!(scope.exclude_patterns.is_empty());
    }

    #[test]
    fn parse_scope_basic_exclude() {
        let scope = parse_scope("!node_modules");
        assert!(scope.include_paths.is_empty());
        assert_eq!(scope.exclude_patterns, vec!["node_modules"]);
    }

    #[test]
    fn parse_scope_mixed() {
        let scope = parse_scope("~/projects, !node_modules, !.git");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
        assert_eq!(scope.exclude_patterns, vec!["node_modules", ".git"]);
    }

    #[test]
    fn parse_scope_multiple_includes() {
        let scope = parse_scope("~/projects, ~/Documents");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(
            scope.include_paths,
            vec![format!("{home}/projects"), format!("{home}/Documents")]
        );
    }

    #[test]
    fn parse_scope_quoted_commas_double() {
        let scope = parse_scope("\"path,with,commas\"");
        assert_eq!(scope.include_paths, vec!["path,with,commas"]);
    }

    #[test]
    fn parse_scope_quoted_commas_single() {
        let scope = parse_scope("'path,with,commas'");
        assert_eq!(scope.include_paths, vec!["path,with,commas"]);
    }

    #[test]
    fn parse_scope_backslash_escaped_commas() {
        let scope = parse_scope("path\\,with\\,commas");
        assert_eq!(scope.include_paths, vec!["path,with,commas"]);
    }

    #[test]
    fn parse_scope_empty_segments() {
        let scope = parse_scope("~/projects, , !.git");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
        assert_eq!(scope.exclude_patterns, vec![".git"]);
    }

    #[test]
    fn parse_scope_bare_exclude_wildcard() {
        let scope = parse_scope("!.*");
        assert_eq!(scope.exclude_patterns, vec![".*"]);
    }

    #[test]
    fn parse_scope_absolute_exclude_path() {
        let scope = parse_scope("!/Users/alice/Downloads");
        assert_eq!(scope.exclude_patterns, vec!["/Users/alice/Downloads"]);
    }

    #[test]
    fn parse_scope_empty_input() {
        let scope = parse_scope("");
        assert!(scope.include_paths.is_empty());
        assert!(scope.exclude_patterns.is_empty());
    }

    #[test]
    fn parse_scope_whitespace_trimming() {
        let scope = parse_scope("  ~/projects  ,  !node_modules  ");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
        assert_eq!(scope.exclude_patterns, vec!["node_modules"]);
    }

    // ── Scope filtering in search ───────────────────────────────────

    /// Build a test index representing:
    /// /Users/alice/projects/app.rs         (id=9)
    /// /Users/alice/projects/node_modules/pkg.json (id=11)
    /// /Users/alice/.git/config             (id=13)
    fn make_scope_test_index() -> SearchIndex {
        let mut names = String::new();
        let test_names = [
            "",             // 0: root
            "Users",        // 1
            "alice",        // 2
            "projects",     // 3
            "app.rs",       // 4
            "node_modules", // 5
            "pkg.json",     // 6
            ".git",         // 7
            "config",       // 8
        ];
        let offsets: Vec<(u32, u16)> = test_names.iter().map(|n| arena_push(&mut names, n)).collect();

        let entries = vec![
            SearchEntry {
                id: 1,
                parent_id: 0,
                name_offset: offsets[0].0,
                name_len: offsets[0].1,
                is_directory: true,
                size: None,
                modified_at: None,
            },
            SearchEntry {
                id: 2,
                parent_id: 1,
                name_offset: offsets[1].0,
                name_len: offsets[1].1,
                is_directory: true,
                size: None,
                modified_at: Some(1000),
            },
            SearchEntry {
                id: 3,
                parent_id: 2,
                name_offset: offsets[2].0,
                name_len: offsets[2].1,
                is_directory: true,
                size: None,
                modified_at: Some(2000),
            },
            SearchEntry {
                id: 4,
                parent_id: 3,
                name_offset: offsets[3].0,
                name_len: offsets[3].1,
                is_directory: true,
                size: None,
                modified_at: Some(3000),
            },
            SearchEntry {
                id: 9,
                parent_id: 4,
                name_offset: offsets[4].0,
                name_len: offsets[4].1,
                is_directory: false,
                size: Some(1000),
                modified_at: Some(4000),
            },
            SearchEntry {
                id: 10,
                parent_id: 4,
                name_offset: offsets[5].0,
                name_len: offsets[5].1,
                is_directory: true,
                size: None,
                modified_at: Some(5000),
            },
            SearchEntry {
                id: 11,
                parent_id: 10,
                name_offset: offsets[6].0,
                name_len: offsets[6].1,
                is_directory: false,
                size: Some(500),
                modified_at: Some(6000),
            },
            SearchEntry {
                id: 12,
                parent_id: 3,
                name_offset: offsets[7].0,
                name_len: offsets[7].1,
                is_directory: true,
                size: None,
                modified_at: Some(7000),
            },
            SearchEntry {
                id: 13,
                parent_id: 12,
                name_offset: offsets[8].0,
                name_len: offsets[8].1,
                is_directory: false,
                size: Some(200),
                modified_at: Some(8000),
            },
        ];
        let mut id_to_index = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            id_to_index.insert(e.id, i);
        }
        SearchIndex {
            names,
            entries,
            id_to_index,
            parent_name_lookup: OnceLock::new(),
            generation: 1,
        }
    }

    #[test]
    fn search_with_include_path_filter() {
        let index = make_scope_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: Some(false),
            include_paths: Some(vec!["/Users/alice/projects".to_string()]),
            exclude_dir_names: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        // Should find app.rs and pkg.json (both under /Users/alice/projects)
        // but NOT config (under /Users/alice/.git)
        assert_eq!(result.total_count, 2);
        let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"app.rs"));
        assert!(names.contains(&"pkg.json"));
    }

    #[test]
    fn search_with_exclude_pattern() {
        let index = make_scope_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: Some(false),
            include_paths: None,
            exclude_dir_names: Some(vec!["node_modules".to_string()]),
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        // Should find app.rs and config, but NOT pkg.json (under node_modules)
        assert_eq!(result.total_count, 2);
        let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"app.rs"));
        assert!(names.contains(&"config"));
        assert!(!names.contains(&"pkg.json"));
    }

    #[test]
    fn search_with_include_and_exclude() {
        let index = make_scope_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: Some(false),
            include_paths: Some(vec!["/Users/alice/projects".to_string()]),
            exclude_dir_names: Some(vec!["node_modules".to_string()]),
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        // Only app.rs: under projects but not under node_modules
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "app.rs");
    }

    #[test]
    fn search_with_wildcard_exclude() {
        let index = make_scope_test_index();
        let query = SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: Some(false),
            include_paths: None,
            exclude_dir_names: Some(vec![".*".to_string()]),
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        // Should exclude config (under .git) but keep app.rs and pkg.json
        assert_eq!(result.total_count, 2);
        let names: Vec<&str> = result.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"app.rs"));
        assert!(names.contains(&"pkg.json"));
        assert!(!names.contains(&"config"));
    }

    // ── Serde: new scope fields ─────────────────────────────────────

    #[test]
    fn serde_query_scope_fields_camel_case() {
        let json = r#"{"namePattern":"*.rs","patternType":"glob","includePaths":["/Users/alice/projects"],"excludeDirNames":["node_modules",".git"],"limit":30}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.include_paths, Some(vec!["/Users/alice/projects".to_string()]));
        assert_eq!(
            query.exclude_dir_names,
            Some(vec!["node_modules".to_string(), ".git".to_string()])
        );
    }

    #[test]
    fn serde_query_scope_fields_omitted() {
        let json = r#"{"limit":30}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.include_paths, None);
        assert_eq!(query.exclude_dir_names, None);
    }

    #[test]
    fn serde_parsed_scope_roundtrip() {
        let scope = ParsedScope {
            include_paths: vec!["/Users/alice/projects".to_string()],
            exclude_patterns: vec!["node_modules".to_string(), ".git".to_string()],
        };
        let json = serde_json::to_string(&scope).unwrap();
        assert!(json.contains("includePaths"));
        assert!(json.contains("excludePatterns"));
        let deserialized: ParsedScope = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.include_paths, scope.include_paths);
        assert_eq!(deserialized.exclude_patterns, scope.exclude_patterns);
    }
}
