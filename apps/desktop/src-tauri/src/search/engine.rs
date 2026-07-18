//! Pure search execution: no I/O, no DB access.
//!
//! Takes an `&SearchIndex` + `&SearchQuery`, scans in-memory with rayon, and returns results.

use std::collections::HashSet;

use rayon::prelude::*;
use regex::{Regex, RegexBuilder};

use crate::indexing::store::{self, ROOT_ID};

use super::index::SearchIndex;
use super::query::{SYSTEM_DIR_EXCLUDES, glob_to_regex, summarize_query};
use super::ranking::{self, ImportanceWeights};
use super::types::{PatternType, SearchQuery, SearchResultEntry};

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
        // include_paths present but include_path_ids not set (this shouldn't happen).
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

/// One ranked result: its cross-volume-comparable [`RankKey`] plus the built
/// result entry. Single-volume callers drop the key immediately; the multi-volume
/// orchestrator keeps it to k-way-merge each volume's slice into one global order.
pub(crate) struct RankedEntry {
    pub(crate) key: ranking::RankKey,
    pub(crate) entry: SearchResultEntry,
}

/// Execute a search query against ONE in-memory index, returning a `SearchResult`.
///
/// A thin wrapper over [`search_ranked`] (no path prefix, keys dropped, no coverage
/// gaps) that the pure-engine tests assert against. Production runs
/// [`search_ranked`] directly so it can k-way-merge across volumes and prefix mount
/// paths; this wrapper isn't on any production path, hence `#[cfg(test)]`.
#[cfg(test)]
pub(crate) fn search(
    index: &SearchIndex,
    query: &SearchQuery,
    weights: &ImportanceWeights,
) -> Result<super::types::SearchResult, String> {
    let (ranked, total_count) = search_ranked(index, query, weights, "")?;
    Ok(super::types::SearchResult {
        entries: ranked.into_iter().map(|r| r.entry).collect(),
        total_count,
        uncovered_scopes: Vec::new(),
        unresolved_scopes: Vec::new(),
    })
}

/// Execute a search against ONE volume's index and return the ranked, path-built
/// results (best-first) with their sort keys, plus the total match count.
///
/// `path_prefix` is prepended to every reconstructed path: empty for the `root`
/// volume (its index is `/`-rooted, paths are already absolute), the mount root
/// (`/Volumes/naspi`) for a mount-rooted volume whose index stores mount-relative
/// paths — so a NAS result reports `/Volumes/naspi/sub/file`, not the bare `/sub/file`
/// its index holds, and opens in a pane. The returned entries are already truncated
/// to the query's effective limit, so path reconstruction stays bounded even on a
/// multi-million-entry index.
pub(crate) fn search_ranked(
    index: &SearchIndex,
    query: &SearchQuery,
    weights: &ImportanceWeights,
    path_prefix: &str,
) -> Result<(Vec<RankedEntry>, u32), String> {
    let t = std::time::Instant::now();

    // Case-folding rule (shared by pattern matching and ranking): platform default
    // is insensitive on macOS, sensitive on Linux, overridable per query.
    let case_insensitive = match query.case_sensitive {
        Some(true) => false,
        Some(false) => true,
        None => cfg!(target_os = "macos"),
    };

    // Guard: reject unfiltered scans on large indexes. Without a namePattern,
    // size filter, or directory filter, we'd scan every entry (~60s for 5M entries).
    let has_name = query.name_pattern.as_ref().is_some_and(|p| !p.is_empty());
    let has_size = query.min_size.is_some() || query.max_size.is_some();
    let has_date = query.modified_after.is_some() || query.modified_before.is_some();
    let has_dir_filter = query.is_directory.is_some();
    if !has_name && !has_size && !has_dir_filter && !has_date && index.entries.len() > 100_000 {
        return Err(
            "Query too broad. Add a filename pattern, size, date, or type filter to narrow results.".to_string(),
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

            // Scope filter (ancestor walk): only for entries passing all other filters
            if scope_filter.is_active() && !scope_filter.matches(index, *i) {
                return false;
            }

            true
        })
        .map(|(i, _)| i)
        .collect();

    let total_count = matching_indices.len() as u32;

    let has_size_filter = query.min_size.is_some() || query.max_size.is_some();
    let dirs_included = query.is_directory != Some(false);

    // Count-only: skip ranking, truncation, and per-entry path materialization —
    // the expensive parts — and return just the total.
    //
    // A size filter on directories is the one case that needs more work. Directory
    // sizes live in `dir_stats` (the DB), not the in-memory index, so the engine
    // can't size-filter directories here (that's why `total_count` still counts
    // every matching directory). When a size filter is set and directories aren't
    // excluded, hand the matching directories back in `entries` so the caller can
    // fetch their sizes and subtract the ones outside the filter (see
    // `query::finalize_count_only`). Files are already size-filtered above.
    if query.count_only {
        // Skip ranking and file materialization — the count is exact as-is. Exception: a
        // size filter on directories needs their dir_stats sizes (the DB, filled by
        // execute.rs), so hand the matching directories back — ranked, so they reuse the
        // same materialization — for the caller to size-check and subtract. Files are
        // already size-filtered above.
        let entries: Vec<RankedEntry> = if has_size_filter && dirs_included {
            let home_dir = dirs::home_dir().map(|p| p.to_string_lossy().to_string());
            let dir_indices: Vec<usize> = matching_indices
                .iter()
                .copied()
                .filter(|&idx| index.entries[idx].is_directory)
                .collect();
            let stem = ranking::stem_for(query);
            ranking::rank_decorated(index, &dir_indices, &stem, case_insensitive, weights)
                .into_iter()
                .map(|(key, idx)| build_ranked_entry(index, key, idx, path_prefix, home_dir.as_deref()))
                .collect()
        } else {
            Vec::new()
        };
        log::debug!(
            "Count-only search: {} → {} matches, took {:?}",
            summarize_query(query),
            total_count,
            t.elapsed()
        );
        return Ok((entries, total_count));
    }

    // Rank by match-quality band first, then importance-boosted recency within a
    // band (empty weights ⇒ pure recency, today's order). See `ranking.rs`. Keep the
    // keys: the multi-volume merge k-way-merges each volume's slice on them.
    let stem = ranking::stem_for(query);
    let mut ranked = ranking::rank_decorated(index, &matching_indices, &stem, case_insensitive, weights);

    // Take first `limit` entries. When size filters are active and directories
    // are included, collect extra candidates because some directories may be
    // filtered out later in fill_directory_sizes (directory sizes come from
    // dir_stats, not the entries table).
    let base_limit = query.limit.min(1000) as usize;
    let limit = if has_size_filter && dirs_included {
        (base_limit * 3).max(base_limit + 100)
    } else {
        base_limit
    };
    ranked.truncate(limit);

    // Reconstruct paths and build result entries (prefixed into the volume's mount
    // space, so a non-root volume's mount-relative index paths become absolute).
    let home_dir = dirs::home_dir().map(|p| p.to_string_lossy().to_string());
    let entries: Vec<RankedEntry> = ranked
        .iter()
        .map(|&(key, idx)| build_ranked_entry(index, key, idx, path_prefix, home_dir.as_deref()))
        .collect();

    log::debug!(
        "Search completed: {} → {} matches (returning {}), took {:?}",
        summarize_query(query),
        total_count,
        entries.len(),
        t.elapsed()
    );
    Ok((entries, total_count))
}

/// Prepend a volume's mount-root prefix to an index-reconstructed path.
///
/// Empty prefix (the `root` volume): return the path unchanged. Otherwise the index
/// is mount-rooted and `path` is mount-relative (`/sub/file`, or `/` for the mount
/// root itself), so join them into the mount-absolute path (`/Volumes/naspi/sub/file`).
fn apply_path_prefix(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        path.to_string()
    } else if path == "/" {
        prefix.to_string()
    } else {
        format!("{prefix}{path}")
    }
}

/// Materialize one ranked hit into a `RankedEntry`: reconstruct its full path
/// (prefixed into the volume's mount space, so a non-root volume's mount-relative
/// index paths become absolute), derive the `~`-relative parent path, and pick an
/// icon. `home_dir` is the absolute home directory (for the `~` substitution), passed
/// in so a batch reconstructs it once.
fn build_ranked_entry(
    index: &SearchIndex,
    key: ranking::RankKey,
    idx: usize,
    path_prefix: &str,
    home_dir: Option<&str>,
) -> RankedEntry {
    let entry = &index.entries[idx];
    let path = apply_path_prefix(path_prefix, &reconstruct_path_from_index(index, entry.id));
    let parent_path = match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => {
            let parent = &path[..pos];
            // Replace home dir prefix with ~ (a no-op for a prefixed non-root path,
            // whose mount root is never the home dir).
            if let Some(home) = home_dir {
                if let Some(rest) = parent.strip_prefix(home) {
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
    RankedEntry {
        key,
        entry: SearchResultEntry {
            name: entry_name.to_string(),
            path,
            parent_path,
            is_directory: entry.is_directory,
            size: entry.size,
            modified_at: entry.modified_at,
            icon_id,
            entry_id: entry.id,
        },
    }
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
mod tests;
