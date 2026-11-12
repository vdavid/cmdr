//! Pure search execution: no I/O, no DB access.
//!
//! Takes an `&SearchIndex` + `&SearchQuery`, scans in-memory with rayon, and returns results.
//!
//! The per-entry predicates (name pattern, type, size, date) are NOT here: they're a
//! [`CompiledQuery`], so a live walk over unindexed
//! ground evaluates the same rules this scan does. What stays here is arena-shaped:
//! the scope filter's ancestor walk, ranking, and building result rows (the path walk
//! itself is `arena/paths.rs`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rayon::prelude::*;

use super::arena::{ARENA_ROOT_ID, SearchIndex, reconstruct_path_from_index};
use super::context::SearchContext;
use super::excludes::ExcludeRules;
use super::matcher::{Candidate, CompileError, CompiledQuery, Evaluator};
use super::query::summarize_query_for_diagnostics;
use super::ranking;
use super::types::{SearchQuery, SearchResultEntry, SearchSort};

// ── The scan's inputs and output ─────────────────────────────────────

/// Which part of a volume a search covers: the scan's scope input.
///
/// It lives here, below the seam that resolves it (`seam::resolve_scope`), so the scan
/// never reaches up into the host's door for its own input type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedScope {
    /// The whole volume: no scope, or a scope at its mount root.
    Whole,
    /// Under any of these entries (an entry counts as under itself).
    Ids(Vec<i64>),
    /// Nothing: every scope path was outside the volume or not in it.
    Nothing,
}

// ── Scope filter (pre-resolved for the hot loop) ─────────────────────

/// Pre-resolved scope filter for efficient ancestor-walk filtering during search.
///
/// The arena EVALUATOR for the exclusions; the rules themselves are
/// [`ExcludeRules`], shared with the live walk so the two can't disagree
/// (`excludes.rs`). What stays here is arena-shaped: the include roots are entry
/// ids, and the ancestor walk is a `parent_id` chain.
///
/// Cloned once per scan chunk, like the compiled query, and cheap to: everything
/// but the exclude regexes is shared.
#[derive(Clone)]
struct ScopeFilter {
    /// Entry IDs that represent the include path roots. An entry passes if
    /// any of its ancestors (including itself) is in this set.
    include_ids: Option<Arc<HashSet<i64>>>,
    /// The directory names and path prefixes this query excludes.
    excludes: ExcludeRules,
    /// The volume's mount root, prepended before a path-prefix comparison: the
    /// index stores mount-relative paths, and a user's path exclude is absolute.
    path_prefix: Arc<str>,
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
    ///
    /// `memo` is the calling scan chunk's own (see [`AncestorVerdicts`]).
    fn verdict(&self, index: &SearchIndex, entry_idx: usize, memo: &mut AncestorVerdicts) -> ScopeVerdict {
        let (entry_id, entry_parent_id) = (index.id(entry_idx), index.parent_id(entry_idx));

        // Include check: walk ancestors and check if any is in include_ids
        if let Some(ref ids) = self.include_ids {
            let mut found = false;
            let mut current_id = entry_id;
            loop {
                if ids.contains(&current_id) {
                    found = true;
                    break;
                }
                if current_id == ARENA_ROOT_ID || current_id == 0 {
                    break;
                }
                match index.index_of_id(current_id) {
                    Some(idx) => current_id = index.parent_id(idx),
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
            let path = apply_path_prefix(&self.path_prefix, &reconstruct_path_from_index(index, entry_id));
            if self.excludes.excludes_path(&path) {
                return ScopeVerdict::Excluded;
            }
        }

        // For bare-name excludes, walk ancestors and check directory names
        if self.excludes.has_name_rules() && self.excluded_by_name_at_or_above(index, entry_parent_id, memo) {
            return ScopeVerdict::Excluded;
        }

        ScopeVerdict::Inside
    }

    /// Whether a name rule excludes the folder `folder_id` or any folder above it.
    ///
    /// Walks up until the root or a folder `memo` already judged, then records the
    /// answer for every folder it passed: they all share it, since each one's
    /// verdict is its own name OR its parent's verdict.
    fn excluded_by_name_at_or_above(&self, index: &SearchIndex, folder_id: i64, memo: &mut AncestorVerdicts) -> bool {
        memo.walked.clear();
        let mut current_id = folder_id;
        let excluded = loop {
            if current_id == ARENA_ROOT_ID || current_id == 0 {
                break false;
            }
            if let Some(&known) = memo.by_folder.get(&current_id) {
                break known;
            }
            let Some(idx) = index.index_of_id(current_id) else {
                break false;
            };
            memo.walked.push(current_id);
            if index.is_directory(idx) && self.excludes.excludes_dir_name(index.name(idx)) {
                break true;
            }
            current_id = index.parent_id(idx);
        };
        for id in memo.walked.drain(..) {
            memo.by_folder.insert(id, excluded);
        }
        excluded
    }
}

/// One scan chunk's memo of the name-exclusion walk: folder id → whether a name
/// rule excludes that folder or anything above it.
///
/// Matches cluster in folders, so without it a broad query re-walked (a binary
/// search per ancestor) and re-judged (a non-ASCII name folds through `String`s)
/// the same folders for every match: ~1.9 M matches for a one-letter query.
/// ❌ Keep it per chunk, never shared across rayon workers: a shared map is a lock
/// on the hot path, the exact contention the per-chunk clones exist to avoid.
#[derive(Default)]
struct AncestorVerdicts {
    by_folder: HashMap<i64, bool>,
    /// Scratch for the folders one walk passed before it found an answer.
    walked: Vec<i64>,
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

/// Pre-resolve scope filter data from the resolved scope and the query's exclusions.
///
/// `case_insensitive` comes from the compiled query rather than being derived again:
/// excluding under a different alphabet than the pattern matched with is the kind of
/// silent disagreement `matcher.rs` exists to prevent.
fn prepare_scope_filter(
    query: &SearchQuery,
    scope: &ResolvedScope,
    case_insensitive: bool,
    ctx: &SearchContext<'_>,
    path_prefix: &str,
) -> Result<ScopeFilter, CompileError> {
    let include_ids = match scope {
        ResolvedScope::Whole => None,
        ResolvedScope::Ids(ids) => Some(Arc::new(ids.iter().copied().collect::<HashSet<i64>>())),
        // An include set holding no id admits no entry: every one is outside the roots.
        ResolvedScope::Nothing => Some(Arc::new(HashSet::new())),
    };

    Ok(ScopeFilter {
        include_ids,
        excludes: ExcludeRules::from_query(query, case_insensitive, ctx.rules.fold)?,
        path_prefix: Arc::from(path_prefix),
    })
}

// ── Search execution ─────────────────────────────────────────────────

/// Rows one scan task takes. Each task clones the compiled query and the scope
/// filter (see the scan in [`search_ranked`] for why), so this sets how many clones
/// a query pays: ~160 on a 5 M-row arena, against 16 workers to balance.
const SCAN_CHUNK_ROWS: usize = 32_768;

/// Execute a search query against ONE in-memory index, returning a `SearchResult`.
///
/// A thin wrapper over [`search_ranked`] (no path prefix, no coverage gaps) that the
/// pure-engine tests assert against. Production runs [`search_ranked`] directly so it
/// can prefix mount paths; this wrapper isn't on any production path, hence
/// `#[cfg(test)]`.
#[cfg(any(test, feature = "testing"))]
pub fn search(
    index: &SearchIndex,
    query: &SearchQuery,
    weights: &ranking::ImportanceWeights,
) -> Result<super::types::SearchResult, String> {
    let scope = match &query.include_path_ids {
        Some(ids) if !ids.is_empty() => ResolvedScope::Ids(ids.clone()),
        _ => ResolvedScope::Whole,
    };
    let ranked =
        search_ranked(index, query, &scope, &SearchContext::for_test(weights), "", None).map_err(|e| e.to_string())?;
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
pub struct DirSizes {
    by_id: HashMap<i64, u64>,
    /// Whether absence from the map means "outside the size filter" (a filter) or
    /// merely "size unknown" (built only to sort by size). Getting this backwards
    /// would silently delete every directory the index has no `dir_stats` row for.
    is_filter: bool,
}

impl DirSizes {
    pub fn new(by_id: HashMap<i64, u64>, is_filter: bool) -> Self {
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
pub struct Ranked {
    /// The ranked rows, already truncated to the query's effective limit.
    pub entries: Vec<SearchResultEntry>,
    /// Every entry that matched and survived the filters.
    pub total_count: u32,
    /// Matches an exclusion rule dropped: the system/build/cache tier (on unless
    /// the query turns it off) plus any `!` excludes in the scope. Reported rather
    /// than swallowed — a disk-usage question asked with the default exclusions is
    /// answered mostly by the folders they hide.
    pub hidden_by_excludes: u32,
}

/// Execute a search against ONE volume's index and return the ranked, path-built
/// results (best-first) plus the total match count.
///
/// `path_prefix` is prepended to every reconstructed path: empty for the `root`
/// volume (its index is `/`-rooted, paths are already absolute), the mount root
/// (`/Volumes/testshare`) for a mount-rooted volume whose index stores mount-relative
/// paths — so a NAS result reports `/Volumes/testshare/sub/file`, not the bare `/sub/file`
/// its index holds, and opens in a pane. The returned entries are already truncated
/// to the query's effective limit, so path reconstruction stays bounded even on a
/// multi-million-entry index.
pub fn search_ranked(
    index: &SearchIndex,
    query: &SearchQuery,
    scope: &ResolvedScope,
    ctx: &SearchContext<'_>,
    path_prefix: &str,
    dir_sizes: Option<&DirSizes>,
) -> Result<Ranked, CompileError> {
    let t = cmdr_fs::rt::now();

    // The per-entry predicates, plus the broad-query guard this arena's size earns.
    let compiled = CompiledQuery::compile(query, Evaluator::Arena { entries: index.len() }, &ctx.rules)?;
    let case_insensitive = compiled.case_insensitive();

    // Pre-resolve scope filter
    let scope_filter = prepare_scope_filter(query, scope, case_insensitive, ctx, path_prefix)?;

    // How many query-matching entries an exclusion rule dropped. Counted with a
    // relaxed atomic rather than a fold, because `collect()` preserves arena order
    // and the ranking's tie-break rides on it; a fold/reduce would silently make
    // equal-ranked results non-deterministic. The increment only fires on an
    // excluded match.
    let hidden_by_excludes = std::sync::atomic::AtomicU32::new(0);
    let hidden = &hidden_by_excludes;

    // Parallel scan in fixed chunks, each with its OWN clones of the compiled query
    // and the scope filter. A cloned `Regex` gets a cache pool of its own, and the
    // one thread scanning the chunk becomes its owner, so every `is_match` takes the
    // pool's lock-free path. ❌ Don't share one `Regex` across the workers: all but
    // one of them then go through the pool's eight mutex-guarded stacks, and a worker
    // that loses a `try_lock` there builds a whole throwaway cache. That was ~380,000
    // allocations for a query matching nothing on a 5 M-row arena, and 30–45% of the
    // system allocator's search penalty
    // (`docs/notes/performance/search-loop-allocations-2026-09-27.md`).
    let rows = index.len();
    let matching_indices: Vec<usize> = (0..rows.div_ceil(SCAN_CHUNK_ROWS))
        .into_par_iter()
        .flat_map_iter(|chunk_no| {
            let compiled = compiled.clone();
            let scope_filter = scope_filter.clone();
            let mut ancestor_verdicts = AncestorVerdicts::default();
            let first = chunk_no * SCAN_CHUNK_ROWS;
            (first..(first + SCAN_CHUNK_ROWS).min(rows)).filter(move |&i| {
                let id = index.id(i);

                // Skip root sentinel
                if id == ARENA_ROOT_ID {
                    return false;
                }

                let is_directory = index.is_directory(i);
                if !compiled.matches(&Candidate {
                    name: index.name(i),
                    is_directory,
                    size: index.size(i),
                    modified_at: index.modified_at(i),
                }) {
                    return false;
                }

                // A directory's size filter, applied HERE rather than after ranking:
                // its size isn't in the arena, so `compiled` couldn't judge it, and a
                // filter applied to the ranked top-k answers from a recency-ordered
                // sample instead of from the drive.
                if is_directory
                    && let Some(sizes) = dir_sizes
                    && !sizes.passes(id)
                {
                    return false;
                }

                // Scope filter (ancestor walk): only for entries passing all other filters
                if scope_filter.is_active() {
                    match scope_filter.verdict(index, i, &mut ancestor_verdicts) {
                        ScopeVerdict::Inside => {}
                        ScopeVerdict::Excluded => {
                            hidden.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            return false;
                        }
                        ScopeVerdict::OutsideRoots => return false,
                    }
                }

                true
            })
        })
        .collect();

    let total_count = matching_indices.len() as u32;
    let hidden_by_excludes = hidden_by_excludes.into_inner();

    // Count-only: skip ranking, truncation, and per-entry path materialization —
    // the expensive parts — and return just the total. It's exact including
    // directory size filters, because `dir_sizes` applied those inside the scan.
    if query.count_only {
        log::debug!(
            "Count-only search: {} → {} matches ({} hidden), took {:?}",
            // allowed-log-private: the summary logs the query's shape, never its words
            summarize_query_for_diagnostics(query),
            total_count,
            hidden_by_excludes,
            cmdr_fs::rt::since(t)
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
    let limit = query.effective_limit();

    // Order the survivors. Relevance is the default and what `ranking.rs` owns:
    // match-quality band first, then importance-boosted recency within a band
    // (empty weights ⇒ pure recency). An explicit `sort_by` replaces that
    // wholesale — a caller who asked for the biggest matches wants the biggest on
    // the drive, not the best-ranked few reordered among themselves.
    let ranked = match query.sort_by {
        None | Some(SearchSort::Relevance) => {
            let stem = ranking::stem_for(query, &ctx.rules);
            ranking::rank_indices(index, &matching_indices, &stem, case_insensitive, ctx.weights, limit)
        }
        Some(sort) => sort_indices(index, matching_indices.clone(), sort, dir_sizes, limit),
    };

    // Reconstruct paths and build result entries (prefixed into the volume's mount
    // space, so a non-root volume's mount-relative index paths become absolute).
    let entries: Vec<SearchResultEntry> = ranked
        .iter()
        .map(|&idx| build_result_entry(index, idx, path_prefix, ctx.home_dir))
        .collect();
    let (entries, repeats) = one_row_per_path(entries);
    // The count is of files, and a repeat is a file counted twice. Only the repeats
    // among the returned rows are known, so past the limit the total can still run
    // high by the ones nobody looked at: the bounded error `ResultStream` accepts.
    let total_count = total_count.saturating_sub(repeats);

    log::debug!(
        "Search completed: {} → {} matches (returning {}, {} hidden), took {:?}",
        // allowed-log-private: the summary logs the query's shape, never its words
        summarize_query_for_diagnostics(query),
        total_count,
        entries.len(),
        hidden_by_excludes,
        cmdr_fs::rt::since(t)
    );
    Ok(Ranked {
        entries,
        total_count,
        hidden_by_excludes,
    })
}

/// `entries` with each path once, and how many rows that dropped.
///
/// The arena can hold one path under two ids: its load reads the id ranges on
/// separate connections rather than in one snapshot, and a warm arena keeps the rows
/// the index has deleted since, so a file deleted and created again is there as both
/// the row that went and the row that replaced it. A path is a row's identity for
/// every consumer (the dialog's list throws on a repeated key), so the repeat stops
/// here, the one place every answer passes.
///
/// The row kept is the one with the highest id, which is the one created last, and it
/// takes the place of the path's first row, so the ranking of everything else stands.
/// Judged on the returned rows only: a path is a string the arena doesn't hold, and
/// building one per candidate is what the limit exists to avoid.
fn one_row_per_path(entries: Vec<SearchResultEntry>) -> (Vec<SearchResultEntry>, u32) {
    // Per path: where it first sits in the ranking, and which row is the newest.
    let mut newest_at: HashMap<&str, (usize, usize)> = HashMap::with_capacity(entries.len());
    for (at, entry) in entries.iter().enumerate() {
        newest_at
            .entry(entry.path.as_str())
            .and_modify(|(_, newest)| {
                if entry.entry_id > entries[*newest].entry_id {
                    *newest = at;
                }
            })
            .or_insert((at, at));
    }
    if newest_at.len() == entries.len() {
        return (entries, 0);
    }

    let mut kept: Vec<(usize, usize)> = newest_at.into_values().collect();
    kept.sort_unstable();
    let repeats = (entries.len() - kept.len()) as u32;
    let mut rows: Vec<Option<SearchResultEntry>> = entries.into_iter().map(Some).collect();
    let entries = kept.into_iter().filter_map(|(_, newest)| rows[newest].take()).collect();
    (entries, repeats)
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
        match sort {
            SearchSort::Size => {
                if index.is_directory(idx) {
                    dir_sizes.and_then(|sizes| sizes.get(index.id(idx)))
                } else {
                    index.size(idx)
                }
            }
            SearchSort::Modified => index.modified_at(idx),
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
        ordering.then_with(|| index.id(*a).cmp(&index.id(*b)))
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
/// root itself), so join them into the mount-absolute path (`/Volumes/testshare/sub/file`).
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
    let entry_id = index.id(idx);
    let is_directory = index.is_directory(idx);
    let path = apply_path_prefix(path_prefix, &reconstruct_path_from_index(index, entry_id));
    let parent_path = home_relative_parent(&path, home_dir);
    let entry_name = index.name(idx);
    let icon_id = derive_icon_id(entry_name, is_directory);
    SearchResultEntry {
        name: entry_name.to_string(),
        path,
        parent_path,
        is_directory,
        size: index.size(idx),
        modified_at: index.modified_at(idx),
        icon_id,
        entry_id,
    }
}

/// A result row's parent path, with the home directory written as `~`.
///
/// Shared with the live walk, which builds its rows from a walked path rather
/// than from the arena: a result should read the same however it was found.
pub fn home_relative_parent(path: &str, home_dir: Option<&str>) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => {
            let parent = &path[..pos];
            // Replace home dir prefix with ~ (a no-op for a prefixed non-root path,
            // whose mount root is never the home dir). Only at a component
            // boundary: home `/Users/test_user` is no ancestor of `/Users/test_userid`.
            match home_dir
                .and_then(|home| parent.strip_prefix(home))
                .filter(|rest| rest.is_empty() || rest.starts_with('/'))
            {
                Some(rest) => format!("~{rest}"),
                None => parent.to_string(),
            }
        }
        None => path.to_string(),
    }
}

/// Derive an icon ID from filename and directory flag.
pub fn derive_icon_id(name: &str, is_directory: bool) -> String {
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
