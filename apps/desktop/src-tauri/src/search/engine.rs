//! Pure search execution: no I/O, no DB access.
//!
//! Takes an `&SearchIndex` + `&SearchQuery`, scans in-memory with rayon, and returns results.
//!
//! The per-entry predicates (name pattern, type, size, date) are NOT here: they're a
//! [`CompiledQuery`](super::matcher::CompiledQuery), so a live walk over unindexed
//! ground evaluates the same rules this scan does. What stays here is arena-shaped:
//! the scope filter's ancestor walk, ranking, and path reconstruction.

use std::collections::HashSet;

use rayon::prelude::*;
use regex::{Regex, RegexBuilder};

use cmdr_index::store::{self, ROOT_ID};

use super::index::SearchIndex;
use super::matcher::{Candidate, CompiledQuery, Evaluator};
use super::query::{SYSTEM_DIR_EXCLUDES, glob_to_regex, summarize_query};
use super::ranking::{self, ImportanceWeights};
use super::types::{SearchQuery, SearchResultEntry};

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

/// Pre-resolve scope filter data from the query.
///
/// `case_insensitive` comes from the compiled query rather than being derived again:
/// excluding under a different alphabet than the pattern matched with is the kind of
/// silent disagreement `matcher.rs` exists to prevent.
fn prepare_scope_filter(query: &SearchQuery, case_insensitive: bool) -> ScopeFilter {
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

    // User-specified excludes: wildcards → regex, plain names → exact HashSet
    if let Some(ref patterns) = query.exclude_dir_names {
        for pattern in patterns {
            if pattern.contains('/') {
                exclude_path_prefixes.push(pattern.clone());
            } else if pattern.contains('*') || pattern.contains('?') {
                let regex_str = glob_to_regex(pattern);
                // `dot_matches_new_line`: the same glob the query bar compiles, so it
                // has to mean the same thing. See `matcher::compile_pattern`.
                if let Ok(re) = RegexBuilder::new(&regex_str)
                    .case_insensitive(case_insensitive)
                    .dot_matches_new_line(true)
                    .build()
                {
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

/// Execute a search query against ONE in-memory index, returning a `SearchResult`.
///
/// A thin wrapper over [`search_ranked`] (no path prefix, no coverage gaps) that the
/// pure-engine tests assert against. Production runs [`search_ranked`] directly so it
/// can prefix mount paths; this wrapper isn't on any production path, hence
/// `#[cfg(test)]`.
#[cfg(test)]
pub(crate) fn search(
    index: &SearchIndex,
    query: &SearchQuery,
    weights: &ImportanceWeights,
) -> Result<super::types::SearchResult, String> {
    let (entries, total_count) = search_ranked(index, query, weights, "")?;
    Ok(super::types::SearchResult {
        entries,
        total_count,
        uncovered_scopes: Vec::new(),
        unresolved_scopes: Vec::new(),
        target_volume_id: String::new(),
    })
}

/// Execute a search against ONE volume's index and return the ranked, path-built
/// results (best-first) plus the total match count.
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
) -> Result<(Vec<SearchResultEntry>, u32), String> {
    let t = std::time::Instant::now();

    // The per-entry predicates, plus the broad-query guard this arena's size earns.
    let compiled = CompiledQuery::compile(
        query,
        Evaluator::Arena {
            entries: index.entries.len(),
        },
    )
    .map_err(|e| e.to_string())?;
    let case_insensitive = compiled.case_insensitive();

    // Pre-resolve scope filter
    let scope_filter = prepare_scope_filter(query, case_insensitive);

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

            if !compiled.matches(&Candidate {
                name: index.name(entry),
                is_directory: entry.is_directory,
                size: entry.size,
                modified_at: entry.modified_at,
            }) {
                return false;
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
        let entries: Vec<SearchResultEntry> = if has_size_filter && dirs_included {
            let home_dir = dirs::home_dir().map(|p| p.to_string_lossy().to_string());
            let dir_indices: Vec<usize> = matching_indices
                .iter()
                .copied()
                .filter(|&idx| index.entries[idx].is_directory)
                .collect();
            let stem = ranking::stem_for(query);
            // `usize::MAX`: the caller subtracts the out-of-range directories from the
            // volume total, so it needs every matching directory, not a top-k slice.
            ranking::rank_indices(index, &dir_indices, &stem, case_insensitive, weights, usize::MAX)
                .into_iter()
                .map(|idx| build_result_entry(index, idx, path_prefix, home_dir.as_deref()))
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

    // Keep only `limit` entries. When size filters are active and directories are
    // included, keep extra candidates because some directories may be filtered out
    // later in fill_directory_sizes (directory sizes come from dir_stats, not the
    // entries table).
    let base_limit = query.limit.min(1000) as usize;
    let limit = if has_size_filter && dirs_included {
        (base_limit * 3).max(base_limit + 100)
    } else {
        base_limit
    };

    // Rank by match-quality band first, then importance-boosted recency within a
    // band (empty weights ⇒ pure recency, today's order). See `ranking.rs`. The
    // returned order IS the result order; the caller only truncates it.
    let stem = ranking::stem_for(query);
    let ranked = ranking::rank_indices(index, &matching_indices, &stem, case_insensitive, weights, limit);

    // Reconstruct paths and build result entries (prefixed into the volume's mount
    // space, so a non-root volume's mount-relative index paths become absolute).
    let home_dir = dirs::home_dir().map(|p| p.to_string_lossy().to_string());
    let entries: Vec<SearchResultEntry> = ranked
        .iter()
        .map(|&idx| build_result_entry(index, idx, path_prefix, home_dir.as_deref()))
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

/// Materialize one ranked hit into a `SearchResultEntry`: reconstruct its full path
/// (prefixed into the volume's mount space, so a non-root volume's mount-relative
/// index paths become absolute), derive the `~`-relative parent path, and pick an
/// icon. `home_dir` is the absolute home directory (for the `~` substitution), passed
/// in so a batch reconstructs it once.
fn build_result_entry(index: &SearchIndex, idx: usize, path_prefix: &str, home_dir: Option<&str>) -> SearchResultEntry {
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

/// The [`hash_path`](ranking::hash_path) of an entry's full path, without ever
/// building that path.
///
/// The ranking blend needs a folder's importance weight, and a weight lookup is a
/// hash lookup — the path `String` [`reconstruct_path_from_index`] would build exists
/// only to be hashed and dropped. A broad query ranks millions of candidates, so this
/// walks the same parent chain and streams the bytes into a [`PathHasher`] instead.
/// Byte-identical to hashing the reconstructed path (pinned by
/// `streamed_hash_matches_whole_path_hash`).
pub(crate) fn hash_path_from_index(index: &SearchIndex, entry_id: i64) -> u64 {
    let mut hasher = ranking::PathHasher::new();
    if entry_id == ROOT_ID {
        hasher.write(b"/");
        return hasher.finish();
    }

    // The chain yields components leaf-first; the path needs them root-first, so
    // collect the (borrowed, non-allocating) names before hashing.
    let mut components: Vec<&str> = Vec::new();
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

    if components.is_empty() {
        // `format!("/{}", "")` — the reconstructed path for an empty chain.
        hasher.write(b"/");
        return hasher.finish();
    }
    for name in components.iter().rev() {
        hasher.write(b"/");
        hasher.write(name.as_bytes());
    }
    hasher.finish()
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
