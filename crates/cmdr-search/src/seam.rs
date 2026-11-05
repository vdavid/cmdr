//! The seam between the search core and where its rows come from.
//!
//! Matching never moved into SQL: the scan is an in-memory pass over an arena. What a
//! source does is HAND OVER that arena and answer the two questions it can't: which
//! entry ids a scope path names, and how big a folder is. So the seam is asked a few
//! times per query, ❌ never per row (a per-row trait call would lose `par_chunks` and
//! the per-chunk regex clones). The desktop answers from the index database
//! (`search/index_source.rs`); an in-memory source answers from a tree.

use std::collections::HashMap;

use super::arena::SearchIndex;
use super::context::SearchContext;
use super::engine::{self, DirSizes, Ranked, ResolvedScope};
use super::matcher::CompileError;
use super::query::to_index_relative;
use super::types::{SearchQuery, SearchResultEntry, SearchSort};

/// A volume's rows: the arena, where its paths sit, and what a scope path names.
pub trait SearchRows {
    /// The loaded arena.
    fn arena(&self) -> &SearchIndex;
    /// The mount root a row's index path is prefixed with (`/Volumes/naspi`), empty
    /// for a volume whose index is `/`-rooted.
    fn path_prefix(&self) -> &str;
    /// The entry id each index-relative path (`/photos/2024`) names, or `None` for a
    /// path the source doesn't hold. One answer per path, in order.
    fn resolve_scope(&self, index_paths: &[&str]) -> Result<Vec<Option<i64>>, SourceError>;
}

/// A volume's folder sizes (recursive logical bytes), which the arena doesn't carry.
///
/// ❌ No "sizes not tracked" answer: every host has real folder sizes, and a read
/// that fails is an error, never an empty map (the engine reads "no map" as "no
/// size filter").
pub trait DirSizeSource {
    /// Every folder whose size falls in `[min, max]` (either bound open), by id.
    fn in_range(&self, min: Option<u64>, max: Option<u64>) -> Result<HashMap<i64, u64>, SourceError>;
    /// Each folder's size, one answer per id, in order; `None` for a folder with no
    /// size on record.
    fn of(&self, ids: &[i64]) -> Result<Vec<Option<u64>>, SourceError>;
}

/// Which read a source couldn't make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRead {
    /// Resolving the query's scope paths to entry ids.
    Scope,
    /// The folder sizes a size filter or a size sort needs before ranking.
    SizeRange,
    /// The sizes of the folders in the returned rows.
    SizeFill,
}

/// A source read that failed. `detail` is for logs, never for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError {
    pub read: SourceRead,
    pub detail: String,
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.read {
            SourceRead::Scope => write!(f, "Couldn't resolve the search scope: {}", self.detail),
            SourceRead::SizeRange | SourceRead::SizeFill => {
                write!(f, "Couldn't read directory sizes: {}", self.detail)
            }
        }
    }
}

/// Why a search couldn't answer. Typed so a host branches on the variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    /// The query itself: an invalid pattern or exclusion, or too broad for the arena.
    Query(CompileError),
    /// A source read failed. ❌ Never answered as empty, unresolved, or unsized.
    SourceUnreadable(SourceError),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Query(error) => error.fmt(f),
            Self::SourceUnreadable(error) => error.fmt(f),
        }
    }
}

impl From<CompileError> for SearchError {
    fn from(error: CompileError) -> Self {
        Self::Query(error)
    }
}

impl From<SourceError> for SearchError {
    fn from(error: SourceError) -> Self {
        Self::SourceUnreadable(error)
    }
}

/// A query's scope, resolved: what it covers, and the paths that named nothing here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeResolution {
    pub scope: ResolvedScope,
    /// Scope paths (as the caller wrote them) that routed to this volume but aren't in
    /// it: a typo, a since-deleted folder, or a path outside the mount root. An honest
    /// signal rather than a silent empty result.
    pub unresolved: Vec<String>,
}

/// Resolve `scope_paths`, each an `(original, canonical)` pair: the path as the
/// caller wrote it (what `unresolved` reports) and the same path with `~` expanded and
/// symlinks resolved (what is looked up). The host makes the canonical form, because
/// only it can `realpath`; the order is `~`, then `realpath`, then the mount strip,
/// since an index-relative path can't be canonicalized.
///
/// - **The mount root itself** (stripped to `/`) means the WHOLE volume, whatever
///   the other paths say.
/// - **A path outside the mount root** is unresolved without asking the source.
/// - **Nothing resolved** means the search covers nothing, ❌ never the whole volume.
pub fn resolve_scope(rows: &dyn SearchRows, scope_paths: &[(String, String)]) -> Result<ScopeResolution, SourceError> {
    if scope_paths.is_empty() {
        return Ok(ScopeResolution {
            scope: ResolvedScope::Whole,
            unresolved: Vec::new(),
        });
    }
    let mount_root = Some(rows.path_prefix()).filter(|prefix| !prefix.is_empty());
    let mut unresolved = Vec::new();
    let mut to_resolve: Vec<(&str, String)> = Vec::new();
    for (original, canonical) in scope_paths {
        match to_index_relative(canonical, mount_root) {
            Some(index_path) if index_path == "/" => {
                return Ok(ScopeResolution {
                    scope: ResolvedScope::Whole,
                    unresolved: Vec::new(),
                });
            }
            Some(index_path) => to_resolve.push((original, index_path)),
            None => unresolved.push(original.clone()),
        }
    }

    let index_paths: Vec<&str> = to_resolve.iter().map(|(_, index_path)| index_path.as_str()).collect();
    let found = rows.resolve_scope(&index_paths)?;
    let mut ids = Vec::new();
    for ((original, index_path), id) in to_resolve.iter().zip(found) {
        match id {
            Some(id) => ids.push(id),
            None => {
                log::debug!(
                    "search: include path not found in index: {index_path}",
                    index_path = cmdr_fs::log_private::LogPrivate::dir_path(index_path)
                );
                unresolved.push((*original).to_string());
            }
        }
    }
    Ok(ScopeResolution {
        scope: if ids.is_empty() {
            ResolvedScope::Nothing
        } else {
            ResolvedScope::Ids(ids)
        },
        unresolved,
    })
}

/// What one volume's rows answer for a query, on their own: the whole of an
/// index-only search, and the covered half of a live run.
#[derive(Debug)]
pub struct CoveredHalf {
    /// The ranked rows, cut to the query's limit, folders carrying their sizes.
    pub entries: Vec<SearchResultEntry>,
    /// Every match that survived the filters.
    pub total: u32,
    /// Scope paths that named nothing on this volume ([`ScopeResolution::unresolved`]).
    pub unresolved_scopes: Vec<String>,
    /// Matches the exclusion rules kept out of `total`.
    pub hidden_by_excludes: u32,
}

/// One search over one volume's rows: resolve the scope, read the folder sizes the
/// query needs before ranking, scan and rank, then fill the returned folders' sizes.
///
/// `scope_paths` are `(original, canonical)` pairs ([`resolve_scope`]); empty means
/// the whole volume.
pub fn search_covered(
    rows: &dyn SearchRows,
    sizes: &dyn DirSizeSource,
    query: &SearchQuery,
    scope_paths: &[(String, String)],
    ctx: &SearchContext<'_>,
) -> Result<CoveredHalf, SearchError> {
    let resolution = resolve_scope(rows, scope_paths)?;
    let dir_sizes = dir_sizes_for(query, sizes)?;
    let Ranked {
        mut entries,
        total_count: total,
        hidden_by_excludes,
    } = engine::search_ranked(
        rows.arena(),
        query,
        &resolution.scope,
        ctx,
        rows.path_prefix(),
        dir_sizes.as_ref(),
    )?;

    if query.count_only {
        // The engine's total is already exact (directory size filters included, since
        // `dir_sizes` applied them inside the scan), and count-only returns no rows.
        entries.clear();
    } else {
        // The rows are the right rows; they just don't carry a directory's recursive
        // size yet, because that isn't in the arena.
        fill_dir_sizes(&mut entries, sizes)?;
    }

    Ok(CoveredHalf {
        entries,
        total,
        unresolved_scopes: resolution.unresolved,
        hidden_by_excludes,
    })
}

/// The folder sizes this query needs BEFORE the engine ranks anything, or `None`
/// when it needs none.
///
/// A folder's size isn't in the arena, so the scan can't judge it. Doing it
/// afterwards, over the ranked top-k, is what made `sizeMin: 50 GB` miss a 1.7 TB
/// `~/Library`: it lost a recency-weighted ranking against hundreds of thousands of
/// freshly-touched folders long before anything looked at its size. Handing the
/// passing set in makes both the filter and `total_count` exact.
///
/// Asked only for a query that filters or sorts folders by size, because on the
/// desktop it's a full scan of `dir_stats`.
fn dir_sizes_for(query: &SearchQuery, sizes: &dyn DirSizeSource) -> Result<Option<DirSizes>, SourceError> {
    let dirs_included = query.is_directory != Some(false);
    let has_size_filter = query.min_size.is_some() || query.max_size.is_some();
    let sorts_by_size = query.sort_by == Some(SearchSort::Size);
    if !dirs_included || !(has_size_filter || sorts_by_size) {
        return Ok(None);
    }
    // Without a size filter the range is unbounded, so this is every folder: the map
    // is then a SORT KEY, and a folder missing from it is unknown-sized rather than
    // filtered out.
    let by_id = sizes.in_range(query.min_size, query.max_size)?;
    Ok(Some(DirSizes::new(by_id, has_size_filter)))
}

/// Fill the returned folders' sizes. Files already carry theirs from the arena.
///
/// ❌ A read that fails fails the search, never ships the rows sizeless: a folder
/// with no size reads as "unknown" in the dialog, so a broken read would answer
/// confidently and wrongly, the same trap [`dir_sizes_for`] refuses.
fn fill_dir_sizes(entries: &mut [SearchResultEntry], sizes: &dyn DirSizeSource) -> Result<(), SourceError> {
    let dir_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_directory)
        .map(|(i, _)| i)
        .collect();
    if dir_indices.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = dir_indices.iter().map(|&i| entries[i].entry_id).collect();
    let found = sizes.of(&ids)?;
    for (&idx, size) in dir_indices.iter().zip(found) {
        if let Some(size) = size {
            entries[idx].size = Some(size);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
