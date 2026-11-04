//! Ordering a search-results pane: the rows a search found, re-ordered by column
//! with the SAME comparator every directory listing sorts by, so the two can't
//! drift. Both hosts answer `sort_search_results` through [`sort_search_rows`]: the
//! desktop over its panes' collator, the browser demo over its own.

use std::sync::Arc;

use cmdr_fs::volume::SupportedDates;
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};
use serde::{Deserialize, Serialize};

use crate::NameCollator;
use crate::sorting::{EffectiveSort, SortableEntry, entry_comparator};

/// One search-results row, carrying only what ordering it needs.
///
/// A deliberate subset of `SearchResultEntry`: the path, parent path, and icon id
/// decide nothing about order, and leaving them out keeps a full 10,000-row
/// snapshot's round trip small. The frontend holds the rows and re-orders them by
/// the index list this command answers with, so nothing is shipped back.
#[derive(Debug, Clone, Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchSortRow {
    /// The file's own name (the last path component), which the Name and
    /// Extension columns order by. Not the full path the pane DISPLAYS: a row's
    /// name is its name everywhere else in the pane too (type-to-jump, the
    /// context menu, the MCP rows), and one notion of it stays true here.
    pub name: String,
    #[allow(
        missing_docs,
        reason = "a `///` here would land in `bindings.ts`, which this move keeps unchanged"
    )]
    pub is_directory: bool,
    #[allow(
        missing_docs,
        reason = "a `///` here would land in `bindings.ts`, which this move keeps unchanged"
    )]
    pub size: Option<u64>,
    #[allow(
        missing_docs,
        reason = "a `///` here would land in `bindings.ts`, which this move keeps unchanged"
    )]
    pub modified_at: Option<u64>,
}

impl SortableEntry for SearchSortRow {
    fn name(&self) -> &str {
        &self.name
    }
    fn is_directory(&self) -> bool {
        self.is_directory
    }
    fn size(&self) -> Option<u64> {
        self.size
    }
    fn modified_at(&self) -> Option<u64> {
        self.modified_at
    }
    /// The index stores no creation time. Never read for ordering: search
    /// results claim Modified only, so a Created sort resolves to Modified.
    fn created_at(&self) -> Option<u64> {
        None
    }
    /// Nor an added time, for the same reason.
    fn added_at(&self) -> Option<u64> {
        None
    }
    /// Nor a recursive size: the search never walked into the directories it
    /// matched, which is also why the pane renders `<dir>` in their Size cell.
    /// Both directories being "unknown" orders them by name among themselves.
    fn recursive_size(&self) -> Option<u64> {
        None
    }
    fn recursive_size_complete(&self) -> Option<bool> {
        None
    }
}

/// How a search-results pane's rows should render: `rows`' own indices in order,
/// and the column that order follows.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchSortAnswer {
    /// `rows`' indices, in the order they should render.
    pub order: Vec<u32>,
    /// The column the order follows: the requested one, or Modified for a date
    /// the index doesn't store (every date but Modified).
    pub effective_sort_by: SortColumn,
}

/// Orders a search-results pane's rows, answering with `rows`' own indices in the
/// order they should render.
///
/// The sort is STABLE, so rows equal under the chosen column keep the engine's
/// ranked order between them, which makes a re-sort reproducible instead of
/// shuffling ties. The rows come from a drive index, which stores no creation or
/// added time, so they resolve as a volume recording Modified only: a date sort
/// falls back exactly as it does on such a volume, and the answer says so.
pub fn sort_search_rows(
    rows: &[SearchSortRow],
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
    collator: &Arc<NameCollator>,
) -> SearchSortAnswer {
    let sort = EffectiveSort::resolve(sort_by, sort_order, dir_sort_mode, SupportedDates::NONE);
    let comparator = entry_comparator::<SearchSortRow>(Arc::clone(collator), sort);
    let mut order: Vec<u32> = (0..rows.len() as u32).collect();
    order.sort_by(|a, b| comparator(&rows[*a as usize], &rows[*b as usize]));
    SearchSortAnswer {
        order,
        effective_sort_by: sort.column(),
    }
}
