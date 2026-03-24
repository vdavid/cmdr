//! Pure search execution: no I/O, no DB access.
//!
//! Takes an `&SearchIndex` + `&SearchQuery`, scans in-memory with rayon, and returns results.

use std::collections::HashSet;

use rayon::prelude::*;
use regex::{Regex, RegexBuilder};

use crate::indexing::store::{self, ROOT_ID};

use super::index::SearchIndex;
use super::query::{SYSTEM_DIR_EXCLUDES, glob_to_regex, summarize_query};
use super::types::{PatternType, SearchQuery, SearchResult, SearchResultEntry};

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
fn prepare_scope_filter(query: &SearchQuery) -> ScopeFilter {
    // Use pre-resolved include path IDs (resolved via SQLite before search())
    let include_ids = if let Some(ref ids) = query.include_path_ids {
        if ids.is_empty() {
            None
        } else {
            Some(ids.iter().copied().collect::<HashSet<i64>>())
        }
    } else if query.include_paths.as_ref().is_some_and(|p| !p.is_empty()) {
        // include_paths present but include_path_ids not set — this shouldn't happen.
        // Resolve was expected to happen at the call site before search().
        log::warn!("search: include_paths present but include_path_ids not pre-resolved; scope will be ignored");
        None
    } else {
        None
    };

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

// ── Search execution ─────────────────────────────────────────────────

/// Execute a search query against the in-memory index. Pure function.
pub(crate) fn search(index: &SearchIndex, query: &SearchQuery) -> Result<SearchResult, String> {
    let t = std::time::Instant::now();

    // Guard: reject unfiltered scans on large indexes. Without a namePattern,
    // size filter, or directory filter, we'd scan every entry (~60s for 5M entries).
    let has_name = query.name_pattern.as_ref().is_some_and(|p| !p.is_empty());
    let has_size = query.min_size.is_some() || query.max_size.is_some();
    let has_date = query.modified_after.is_some() || query.modified_before.is_some();
    let has_dir_filter = query.is_directory.is_some();
    if !has_name && !has_size && !has_dir_filter && !has_date && index.entries.len() > 100_000 {
        return Err(
            "Query too broad — add a filename pattern, size, date, or type filter to narrow results.".to_string(),
        );
    }

    // Pre-resolve scope filter
    let scope_filter = prepare_scope_filter(query);

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

// ── Path reconstruction ──────────────────────────────────────────────

/// Reconstruct the full path for an entry by walking the parent_id chain
/// in the in-memory index. O(depth) per entry.
pub(crate) fn reconstruct_path_from_index(index: &SearchIndex, entry_id: i64) -> String {
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
pub(crate) fn derive_icon_id(name: &str, is_directory: bool) -> String {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::search::index::SearchEntry;
    use crate::search::types::PatternType;

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
            generation: 1,
        }
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        // "report*" matches "report.pdf" but NOT "Q1-report.pdf"
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "report.pdf");
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: None,
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
            include_path_ids: Some(vec![4]),
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
            include_path_ids: None,
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
            include_path_ids: Some(vec![4]),
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
            include_path_ids: None,
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
}
