//! Pure search execution: no I/O, no DB access.
//!
//! Takes an `&SearchIndex` + `&SearchQuery`, scans in-memory with rayon, and returns results.
//!
//! The per-entry predicates (name pattern, type, size, date) are NOT here: they're a
//! [`CompiledQuery`], so a live walk over unindexed
//! ground evaluates the same rules this scan does. What stays here is arena-shaped:
//! the scope filter's ancestor walk, ranking, and path reconstruction.

use std::collections::HashSet;

use rayon::prelude::*;

use cmdr_index::store::ROOT_ID;

use super::excludes::ExcludeRules;
use super::index::SearchIndex;
use super::matcher::{Candidate, CompiledQuery, Evaluator};
use super::query::summarize_query;
use super::ranking::{self, ImportanceWeights};
use super::types::{SearchQuery, SearchResultEntry, SearchSort};

// ── Scope filter (pre-resolved for the hot loop) ─────────────────────

/// Pre-resolved scope filter for efficient ancestor-walk filtering during search.
///
/// The arena EVALUATOR for the exclusions; the rules themselves are
/// [`ExcludeRules`], shared with the live walk so the two can't disagree
/// (`excludes.rs`). What stays here is arena-shaped: the include roots are entry
/// ids, and the ancestor walk is a `parent_id` chain.
struct ScopeFilter {
    /// Entry IDs that represent the include path roots. An entry passes if
    /// any of its ancestors (including itself) is in this set.
    include_ids: Option<HashSet<i64>>,
    /// The directory names and path prefixes this query excludes.
    excludes: ExcludeRules,
    /// The volume's mount root, prepended before a path-prefix comparison: the
    /// index stores mount-relative paths, and a user's path exclude is absolute.
    path_prefix: String,
}

impl ScopeFilter {
    fn is_active(&self) -> bool {
        self.include_ids.is_some() || !self.excludes.is_empty()
    }

    /// Check if an entry at `entry_idx` passes the scope filter by walking
    /// the ancestor chain in the in-memory index.
    ///
    /// Three-way, not a bool: an entry the EXCLUDES dropped is a match the user
    /// would have seen with different settings, and saying how many were dropped
    /// is the difference between "no results" and "27 results, 400 more inside
    /// caches". An entry outside the include roots was never in scope and is not
    /// worth counting.
    fn verdict(&self, index: &SearchIndex, entry_idx: usize) -> ScopeVerdict {
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
                return ScopeVerdict::OutsideRoots;
            }
        }

        // Exclude check: walk ancestors, check each name against exclusions
        if self.excludes.has_path_prefixes() {
            // For path-prefix excludes, reconstruct the path lazily — and in the
            // MOUNT space, because a user's path exclude is absolute while the
            // index stores mount-relative paths.
            let path = apply_path_prefix(&self.path_prefix, &reconstruct_path_from_index(index, entry.id));
            if self.excludes.excludes_path(&path) {
                return ScopeVerdict::Excluded;
            }
        }

        // For bare-name excludes, walk ancestors and check directory names
        if self.excludes.has_name_rules() {
            let mut current_id = entry.parent_id;
            loop {
                if current_id == ROOT_ID || current_id == 0 {
                    break;
                }
                match index.id_to_index.get(&current_id) {
                    Some(&idx) => {
                        let ancestor = &index.entries[idx];
                        if ancestor.is_directory && self.excludes.excludes_dir_name(index.name(ancestor)) {
                            return ScopeVerdict::Excluded;
                        }
                        current_id = ancestor.parent_id;
                    }
                    None => break,
                }
            }
        }

        ScopeVerdict::Inside
    }
}

/// Why the scope filter kept or dropped an entry that already matched the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeVerdict {
    /// In scope, not excluded: a result.
    Inside,
    /// An exclusion rule dropped it — the system/cache tier, or a `!` exclude in
    /// the scope. Counted, because the user can turn the first one off.
    Excluded,
    /// Outside the include roots. The user asked about somewhere else, so this
    /// isn't a hidden result.
    OutsideRoots,
}

/// Pre-resolve scope filter data from the query.
///
/// `case_insensitive` comes from the compiled query rather than being derived again:
/// excluding under a different alphabet than the pattern matched with is the kind of
/// silent disagreement `matcher.rs` exists to prevent.
fn prepare_scope_filter(query: &SearchQuery, case_insensitive: bool, path_prefix: &str) -> ScopeFilter {
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

    ScopeFilter {
        include_ids,
        excludes: ExcludeRules::from_query(query, case_insensitive),
        path_prefix: path_prefix.to_string(),
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
    let ranked = search_ranked(index, query, weights, "", None)?;
    Ok(super::types::SearchResult {
        entries: ranked.entries,
        total_count: ranked.total_count,
        uncovered_scopes: Vec::new(),
        unresolved_scopes: Vec::new(),
        target_volume_id: String::new(),
        hidden_by_excludes: ranked.hidden_by_excludes,
    })
}

/// Directory sizes from `dir_stats`, keyed by entry id.
///
/// A directory's size lives in `dir_stats`, not in the arena, so the engine can't
/// know it on its own. `execute.rs` reads the passing set up front and hands it
/// in, which is what makes a directory size filter EXACT: applying it after the
/// ranked cut instead drops the biggest folders on the drive long before anything
/// reads their size, because they lose a recency-weighted ranking against
/// hundreds of thousands of freshly-touched ones.
pub(crate) struct DirSizes {
    by_id: std::collections::HashMap<i64, u64>,
    /// Whether absence from the map means "outside the size filter" (a filter) or
    /// merely "size unknown" (built only to sort by size). Getting this backwards
    /// would silently delete every directory the index has no `dir_stats` row for.
    is_filter: bool,
}

impl DirSizes {
    pub(crate) fn new(by_id: std::collections::HashMap<i64, u64>, is_filter: bool) -> Self {
        Self { by_id, is_filter }
    }

    /// Whether a directory passes. Always true when the map is only a sort key.
    fn passes(&self, entry_id: i64) -> bool {
        !self.is_filter || self.by_id.contains_key(&entry_id)
    }

    /// A directory's recursive size, if known.
    fn get(&self, entry_id: i64) -> Option<u64> {
        self.by_id.get(&entry_id).copied()
    }
}

/// What one volume's scan produced: the ranked rows, how many matched, and how
/// many matches an exclusion rule kept out of that count.
pub(crate) struct Ranked {
    /// The ranked rows, already truncated to the query's effective limit.
    pub(crate) entries: Vec<SearchResultEntry>,
    /// Every entry that matched and survived the filters.
    pub(crate) total_count: u32,
    /// Matches an exclusion rule dropped: the system/build/cache tier (on unless
    /// the query turns it off) plus any `!` excludes in the scope. Reported rather
    /// than swallowed — a disk-usage question asked with the default exclusions is
    /// answered mostly by the folders they hide.
    pub(crate) hidden_by_excludes: u32,
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
    dir_sizes: Option<&DirSizes>,
) -> Result<Ranked, String> {
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
    let scope_filter = prepare_scope_filter(query, case_insensitive, path_prefix);

    // How many query-matching entries an exclusion rule dropped. Counted with a
    // relaxed atomic rather than a fold, because `filter().collect()` on an
    // indexed parallel iterator preserves arena order and the ranking's tie-break
    // rides on it; a fold/reduce would silently make equal-ranked results
    // non-deterministic. The increment only fires on an excluded match.
    let hidden_by_excludes = std::sync::atomic::AtomicU32::new(0);

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
                size: entry.size.get(),
                modified_at: entry.modified_at.get(),
            }) {
                return false;
            }

            // A directory's size filter, applied HERE rather than after ranking:
            // its size isn't in the arena, so `compiled` couldn't judge it, and a
            // filter applied to the ranked top-k answers from a recency-ordered
            // sample instead of from the drive.
            if entry.is_directory
                && let Some(sizes) = dir_sizes
                && !sizes.passes(entry.id)
            {
                return false;
            }

            // Scope filter (ancestor walk): only for entries passing all other filters
            if scope_filter.is_active() {
                match scope_filter.verdict(index, *i) {
                    ScopeVerdict::Inside => {}
                    ScopeVerdict::Excluded => {
                        hidden_by_excludes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        return false;
                    }
                    ScopeVerdict::OutsideRoots => return false,
                }
            }

            true
        })
        .map(|(i, _)| i)
        .collect();

    let total_count = matching_indices.len() as u32;
    let hidden_by_excludes = hidden_by_excludes.into_inner();

    // Count-only: skip ranking, truncation, and per-entry path materialization —
    // the expensive parts — and return just the total. It's exact including
    // directory size filters, because `dir_sizes` applied those inside the scan.
    if query.count_only {
        log::debug!(
            "Count-only search: {} → {} matches, took {:?}",
            summarize_query(query),
            total_count,
            t.elapsed()
        );
        return Ok(Ranked {
            entries: Vec::new(),
            total_count,
            hidden_by_excludes,
        });
    }

    // Every candidate here already passed every filter, directory sizes included,
    // so the cut is exactly the caller's limit — no over-fetch to absorb a
    // post-ranking correction.
    let limit = query.limit.min(1000) as usize;

    // Order the survivors. Relevance is the default and what `ranking.rs` owns:
    // match-quality band first, then importance-boosted recency within a band
    // (empty weights ⇒ pure recency). An explicit `sort_by` replaces that
    // wholesale — a caller who asked for the biggest matches wants the biggest on
    // the drive, not the best-ranked few reordered among themselves.
    let ranked = match query.sort_by {
        None | Some(SearchSort::Relevance) => {
            let stem = ranking::stem_for(query);
            ranking::rank_indices(index, &matching_indices, &stem, case_insensitive, weights, limit)
        }
        Some(sort) => sort_indices(index, matching_indices.clone(), sort, dir_sizes, limit),
    };

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
    Ok(Ranked {
        entries,
        total_count,
        hidden_by_excludes,
    })
}

/// Take the top `limit` matches by an explicit sort key.
///
/// Unknown keys sort LAST in both directions: a directory the index has no
/// `dir_stats` row for is unknown, not zero-sized, and leading either direction
/// with it would claim "biggest" or "smallest" on no evidence. Ties break on entry
/// id so a page is stable.
///
/// `select_nth_unstable_by` first, so a broad query pays a partition over the
/// candidates rather than a full sort of them, and only the surviving prefix is
/// ordered.
fn sort_indices(
    index: &SearchIndex,
    mut candidates: Vec<usize>,
    sort: SearchSort,
    dir_sizes: Option<&DirSizes>,
    limit: usize,
) -> Vec<usize> {
    let key = |idx: usize| -> Option<u64> {
        let entry = &index.entries[idx];
        match sort {
            SearchSort::Size => {
                if entry.is_directory {
                    dir_sizes.and_then(|sizes| sizes.get(entry.id))
                } else {
                    entry.size.get()
                }
            }
            SearchSort::Modified => entry.modified_at.get(),
            SearchSort::Relevance => None,
        }
    };
    let compare = |a: &usize, b: &usize| {
        let (ka, kb) = (key(*a), key(*b));
        let ordering = match (ka, kb) {
            (Some(ka), Some(kb)) => kb.cmp(&ka), // biggest / newest first
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        ordering.then_with(|| index.entries[*a].id.cmp(&index.entries[*b].id))
    };
    if candidates.len() > limit {
        candidates.select_nth_unstable_by(limit, compare);
        candidates.truncate(limit);
    }
    candidates.sort_unstable_by(compare);
    candidates
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
    let parent_path = home_relative_parent(&path, home_dir);
    let entry_name = index.name(entry);
    let icon_id = derive_icon_id(entry_name, entry.is_directory);
    SearchResultEntry {
        name: entry_name.to_string(),
        path,
        parent_path,
        is_directory: entry.is_directory,
        size: entry.size.get(),
        modified_at: entry.modified_at.get(),
        icon_id,
        entry_id: entry.id,
    }
}

/// A result row's parent path, with the home directory written as `~`.
///
/// Shared with the live walk, which builds its rows from a walked path rather
/// than from the arena: a result should read the same however it was found.
pub(crate) fn home_relative_parent(path: &str, home_dir: Option<&str>) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => {
            let parent = &path[..pos];
            // Replace home dir prefix with ~ (a no-op for a prefixed non-root path,
            // whose mount root is never the home dir).
            match home_dir.and_then(|home| parent.strip_prefix(home)) {
                Some(rest) => format!("~{rest}"),
                None => parent.to_string(),
            }
        }
        None => path.to_string(),
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
/// walks the same parent chain and streams the bytes into a [`PathHasher`](super::ranking::PathHasher) instead.
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
