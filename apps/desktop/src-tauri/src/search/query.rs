//! Operations on search types: scope parsing, formatting, query summarization,
//! directory size enrichment, and system directory exclusions.

use crate::indexing::ReadPool;
use crate::indexing::store::{self, IndexStore};

use super::types::{PatternType, SearchQuery, SearchResult};

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

// ── Query summary ────────────────────────────────────────────────────

/// Build a dense, human-readable summary of a `SearchQuery` for logging and display.
///
/// Examples: `"tes"`, `"*.pdf", dirs only`, `size >= 2 MB, last mod before 2026-03-01`
pub(crate) fn summarize_query(query: &SearchQuery) -> String {
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
    const UNITS: &[(u64, &str)] = &[(TB, "TB"), (GB, "GB"), (MB, "MB"), (KB, "KB")];

    for &(threshold, unit) in UNITS {
        if bytes >= threshold {
            let val = bytes as f64 / threshold as f64;
            return if val.fract() == 0.0 {
                format!("{} {unit}", val as u64)
            } else {
                format!("{val:.1} {unit}")
            };
        }
    }
    format!("{bytes} B")
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
pub(crate) fn glob_to_regex(glob: &str) -> String {
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

/// Parse a comma-separated scope string into include paths and exclude patterns.
///
/// Syntax: `~/projects, !node_modules, !.git`
/// - `~` expands to the user's home directory
/// - `!` prefix means exclude
/// - Quoted segments (single or double quotes) and backslash-escaped commas are supported
pub(crate) fn parse_scope(input: &str) -> super::types::ParsedScope {
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

    super::types::ParsedScope {
        include_paths,
        exclude_patterns,
    }
}

/// Split a scope string on commas, respecting quoting and backslash escapes.
pub(crate) fn split_scope_segments(input: &str) -> Vec<String> {
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

// ── Include path resolution ──────────────────────────────────────────

/// Resolve `include_paths` to entry IDs via SQLite and set `include_path_ids`
/// on the query. Call this before `search()` when the query has `include_paths`.
pub(crate) fn resolve_include_paths(query: &mut SearchQuery, pool: &ReadPool) {
    let paths = match query.include_paths.as_ref() {
        Some(p) if !p.is_empty() => p.clone(),
        _ => return,
    };
    let ids = pool
        .with_conn(|conn| {
            let mut resolved = Vec::with_capacity(paths.len());
            for path in &paths {
                match store::resolve_path(conn, path) {
                    Ok(Some(id)) => resolved.push(id),
                    Ok(None) => log::debug!("search: include path not found in index: {path}"),
                    Err(e) => log::warn!("search: failed to resolve include path {path}: {e}"),
                }
            }
            if resolved.is_empty() {
                // No valid include paths resolved — use impossible ID to force all entries to fail
                resolved.push(i64::MIN);
            }
            resolved
        })
        .unwrap_or_else(|e| {
            log::warn!("search: ReadPool error resolving include paths: {e}");
            vec![i64::MIN]
        });
    query.include_path_ids = Some(ids);
}

// ── Directory size enrichment ────────────────────────────────────────

/// Fetch directory sizes for directory entries in the search results.
/// Mutates the result entries in place, setting `size` for directories.
/// Uses batch lookup via entry IDs stored in `SearchResultEntry`.
pub(crate) fn fill_directory_sizes(result: &mut SearchResult, pool: &ReadPool) {
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
                    result.entries[idx].size = Some(stats.recursive_logical_size);
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
            include_path_ids: None,
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
}
