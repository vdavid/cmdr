//! The `list_dir` tool, over the drive index.
//!
//! It REUSES the shipped index query cores (`indexing::list_dir_children`,
//! `get_dir_stats`, `get_dir_stats_batch`) — it reads the index and SQLite only,
//! never the disk, so it's safe on a dead mount. It never re-derives listing
//! logic. The one thing no index query does is rank a folder's children by size,
//! so `sortBy: "size"` batches `get_dir_stats` over the subdirectories and sorts
//! here.
//!
//! **One tool, both questions.** "What's in this folder" is `sortBy: "name"`;
//! "where is my disk space going" is `sortBy: "size"`, which ranks files and
//! folders TOGETHER (a single 900 GB disk image outweighs every folder around it,
//! and a folders-only ranking would hide it). The folder's own recursive total
//! rides along in `size`, so one call answers "how big is this, and which of its
//! children explain that".
//!
//! Every result carries a typed [`Coverage`] block so the model can voice the
//! index's honesty (spec §2.4): the freshness token (`fresh` / `scanning` /
//! `stale` / `off`, only `fresh` authoritative), a typed "no index" / "not in
//! index" state instead of a wrong empty listing, and each size's exact-vs-
//! lower-bound / stale / updating flags straight from `DirStats`.
//!
//! **Every number arrives already spoken.** The agent can't run a script, so each
//! raw value has a formatted twin ([`ChildEntry::size_human`],
//! [`SizeStats::recursive_size_human`], the [`VolumeBlock`] pair) and a paged
//! listing carries a [`Remainder`] for the rows it didn't show. Raw and human
//! both, never one instead of the other: the raw value is what anything
//! downstream computes with. Uncertainty rides INSIDE the string (`≥`, `~`), so a
//! quoted figure can't shed its caveat.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use super::{expand_tilde, join_child_path};
use crate::file_system::volume::SpaceInfo;
use crate::index_host::index;
use crate::mcp::resources::indexing::status_token;
use crate::mcp::{ToolError, ToolResult};
use crate::search::{format_size, format_timestamp};
use cmdr_index::Freshness;
use cmdr_index::store::DirStats;

/// Rows per page when the caller doesn't say. Comfortably under the result budget
/// for a normal folder, so the common call is one round trip.
const DEFAULT_LIMIT: usize = 50;
/// The ceiling on `limit`. `fit_to_result_budget` is the real cut; this only stops
/// a caller from asking for a page nothing could carry.
const MAX_LIMIT: usize = 1_000;

/// The index's honesty for a read: its freshness token, whether reads are
/// authoritative (`fresh` only), and a plain-language caveat when they aren't (or
/// when the path isn't in the index).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    /// `fresh` / `scanning` / `stale` / `off`. The shared token every surface uses.
    pub index_status: String,
    /// Reads are authoritative only when the index is `fresh`.
    pub authoritative: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Build the coverage block. `indexed` is whether the path actually resolved in a
/// live index (so `false` distinguishes "not in the index" from an empty folder).
/// Reuses `status_token` + `Freshness::is_authoritative` so it can't drift from the
/// rest of the app.
pub(crate) fn coverage(enabled: bool, freshness: Option<Freshness>, indexed: bool) -> Coverage {
    let authoritative = freshness.is_some_and(|f| f.is_authoritative());
    let note = if !indexed {
        Some(if enabled {
            "This folder isn't in the drive index — it may be new, hidden, or outside the indexed area.".to_string()
        } else {
            "This volume isn't indexed, so I can't read it from the index.".to_string()
        })
    } else if !authoritative {
        Some(match freshness {
            Some(Freshness::Scanning) => "The index is still scanning, so this may be incomplete.".to_string(),
            _ => "The index may have drifted since the last full scan, so treat this as best-effort.".to_string(),
        })
    } else {
        None
    };
    Coverage {
        index_status: status_token(enabled, freshness).to_string(),
        authoritative,
        note,
    }
}

/// A size string the model can quote verbatim, with any uncertainty INSIDE the
/// string: `≥` when the number can only be higher (a lower bound), `~` when the
/// error runs in both directions.
///
/// The qualifier is part of the string rather than only a neighbouring flag
/// because the agent restates what a tool hands it. Given `"1.8 TB"` plus
/// `sizeIsLowerBound: true`, a model that quotes the number and drops the flag has
/// stated an exact total the index can't back; given `"≥ 1.8 TB"` it physically
/// can't. The flag stays too, for anything that branches on it.
fn qualified_size(bytes: u64, qualifier: Option<&str>) -> String {
    match qualifier {
        Some(q) => format!("{q} {}", format_size(bytes)),
        None => format_size(bytes),
    }
}

/// A size for a model to read out: `format_size`, carrying `≥` when it's only a
/// lower bound. Single-sourced on the `search` table's formatter, so a size reads
/// the same wherever it surfaces (and, like that table, it doesn't consult the
/// user's SI-vs-binary setting — MCP output stays internally consistent).
pub(crate) fn human_size(bytes: u64, is_lower_bound: bool) -> String {
    qualified_size(bytes, is_lower_bound.then_some("≥"))
}

/// One child row, shaped for the model.
///
/// Every raw number has a spoken twin (`size` / `size_human`, `modified` /
/// `modified_human`), because the agent can't run arithmetic: it can't turn
/// 1,975,684,321,280 into "1.8 TB" or an epoch into a date without guessing. Build
/// rows through [`ChildEntry::new`] / [`ChildEntry::set_size`] so the pair can't
/// drift apart.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildEntry {
    pub name: String,
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub is_directory: bool,
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub is_symlink: bool,
    /// How much space this child accounts for: a file's own size, a folder's
    /// RECURSIVE total (from `dir_stats`). One field, because the question a
    /// listing answers is "what's using the space in here", and a folder's own
    /// inode size answers nothing. `None` when the index has no size for it —
    /// never a wrong zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// `size`, spelled out (`"4.2 GB"`, or `"≥ 1.8 TB"` when it's a lower bound).
    /// Absent exactly when `size` is: an unknown size gets no string, never a
    /// `"0 B"` that would read as an empty folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_human: Option<String>,
    /// `true` when a folder's `size` is a lower bound (some subtree was never
    /// fully listed), so the model says "at least" rather than stating a total.
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub size_is_lower_bound: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
    /// `modified` as a date (`"2023-11-14"`). Absent exactly when `modified` is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_human: Option<String>,
}

impl ChildEntry {
    /// Build a row, deriving both spoken forms from the raw values so the two can
    /// never disagree.
    pub(crate) fn new(
        name: String,
        is_directory: bool,
        is_symlink: bool,
        size: Option<u64>,
        size_is_lower_bound: bool,
        modified: Option<u64>,
    ) -> Self {
        Self {
            name,
            is_directory,
            is_symlink,
            size,
            size_human: size.map(|b| human_size(b, size_is_lower_bound)),
            size_is_lower_bound,
            modified,
            modified_human: modified.map(format_timestamp),
        }
    }

    /// Replace the size (and its lower-bound flag), re-deriving `size_human`. The
    /// only way to change a size after construction — assigning the field alone
    /// would leave the string behind, stating the old number.
    fn set_size(&mut self, size: Option<u64>, is_lower_bound: bool) {
        self.size = size;
        self.size_human = size.map(|b| human_size(b, is_lower_bound));
        self.size_is_lower_bound = is_lower_bound;
    }
}

/// What a listing is ordered by. `Size` is the disk-usage question; the other two
/// are browsing orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SortBy {
    Name,
    Size,
    Modified,
}

impl SortBy {
    /// The wire token, so the result can echo the order it actually used.
    fn token(self) -> &'static str {
        match self {
            SortBy::Name => "name",
            SortBy::Size => "size",
            SortBy::Modified => "modified",
        }
    }

    /// The order a caller means when they name this key but no direction:
    /// biggest and newest first, names A→Z.
    fn default_order(self) -> Order {
        match self {
            SortBy::Name => Order::Asc,
            SortBy::Size | SortBy::Modified => Order::Desc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Asc,
    Desc,
}

/// Which children to keep. Absent means both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeFilter {
    Files,
    Dirs,
}

/// A parsed, defaulted `list_dir` request. Parsing is separate from listing so the
/// defaults and the clamps are testable without an index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ListOptions {
    pub sort_by: SortBy,
    pub order: Order,
    pub limit: usize,
    pub offset: usize,
    pub type_filter: Option<TypeFilter>,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            sort_by: SortBy::Name,
            order: SortBy::Name.default_order(),
            limit: DEFAULT_LIMIT,
            offset: 0,
            type_filter: None,
        }
    }
}

impl ListOptions {
    /// Parse the paging and ordering params. An unknown token is an error rather
    /// than a silent fallback: a caller who asked to rank by size and got name
    /// order would read the top row as the biggest one.
    pub(crate) fn from_params(params: &Value) -> Result<Self, ToolError> {
        let sort_by = match params.get("sortBy").and_then(|v| v.as_str()) {
            None => SortBy::Name,
            Some("name") => SortBy::Name,
            Some("size") => SortBy::Size,
            Some("modified") => SortBy::Modified,
            Some(other) => {
                return Err(ToolError::invalid_params(format!(
                    "Unknown sortBy '{other}'. Use 'name', 'size', or 'modified'."
                )));
            }
        };
        let order = match params.get("order").and_then(|v| v.as_str()) {
            None => sort_by.default_order(),
            Some("asc") => Order::Asc,
            Some("desc") => Order::Desc,
            Some(other) => {
                return Err(ToolError::invalid_params(format!(
                    "Unknown order '{other}'. Use 'asc' or 'desc'."
                )));
            }
        };
        let type_filter = match params.get("type").and_then(|v| v.as_str()) {
            None => None,
            Some("file") => Some(TypeFilter::Files),
            Some("dir") => Some(TypeFilter::Dirs),
            Some(other) => {
                return Err(ToolError::invalid_params(format!(
                    "Unknown type '{other}'. Use 'file' or 'dir'."
                )));
            }
        };
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| (n as usize).clamp(1, MAX_LIMIT))
            .unwrap_or(DEFAULT_LIMIT);
        let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        Ok(Self {
            sort_by,
            order,
            limit,
            offset,
            type_filter,
        })
    }
}

/// A directory's recursive size totals plus the honest-size flags from `DirStats`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeStats {
    pub recursive_size: u64,
    /// `recursive_size`, spelled out — with `≥` inside the string when it's a lower
    /// bound, so a quoted total can't pass for an exact one.
    pub recursive_size_human: String,
    pub recursive_file_count: u64,
    pub recursive_dir_count: u64,
    /// `true` when `recursive_size` is a lower bound, not an exact total (some
    /// subtree was never fully listed).
    pub size_is_lower_bound: bool,
    /// `true` when the exact size was computed at an older volume epoch (stale).
    pub size_is_stale: bool,
    /// `true` while the indexer is still applying writes affecting this subtree.
    pub size_is_updating: bool,
    /// `true` if a descendant is a symlink (so the size may omit linked content).
    pub has_symlinks: bool,
}

impl SizeStats {
    fn from_dir_stats(s: &DirStats) -> Self {
        Self {
            recursive_size: s.recursive_size,
            recursive_size_human: human_size(s.recursive_size, !s.recursive_size_complete),
            recursive_file_count: s.recursive_file_count,
            recursive_dir_count: s.recursive_dir_count,
            size_is_lower_bound: !s.recursive_size_complete,
            size_is_stale: s.recursive_size_stale,
            size_is_updating: s.recursive_size_pending,
            has_symlinks: s.recursive_has_symlinks,
        }
    }
}

// ── list_dir ──────────────────────────────────────────────────────────────────

/// The volume the listed folder sits on, and how full it is.
///
/// Rides along on every listing because a size is only actionable next to a
/// capacity: "Downloads is 40 GB" means something different on a drive with 2 TB
/// free than on one with 8 GB. Absent space (nothing watching the volume) is
/// reported as absent, never as zero.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeBlock {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    /// `total_bytes`, spelled out. Present exactly when its byte counterpart is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_human: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_bytes: Option<u64>,
    /// `available_bytes`, spelled out. Present exactly when its byte counterpart is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_human: Option<String>,
}

impl VolumeBlock {
    /// Build the block from the poller's space reading, deriving the spoken forms.
    /// No space known ⇒ all four fields absent, never a zero that reads as a full
    /// disk. Storage with no ceiling omits them for the same reason: it has no
    /// capacity and no free figure, and inventing either would be worse than
    /// silence.
    pub(crate) fn new(id: String, space: Option<SpaceInfo>) -> Self {
        let total = space.and_then(|s| s.total_bytes());
        let available = space.and_then(|s| s.available_bytes());
        Self {
            id,
            total_bytes: total,
            total_human: total.map(format_size),
            available_bytes: available,
            available_human: available.map(format_size),
        }
    }
}

/// What the rows this page did NOT return add up to, so a paged answer can account
/// for the whole folder without the model subtracting anything.
///
/// Present only when it can be stated truthfully; see [`remainder`] for the four
/// cases that omit it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remainder {
    /// Children beyond this page: `total - returned`.
    pub count: usize,
    /// The folder's own recursive total minus the sizes on this page.
    pub bytes: u64,
    /// `bytes`, spelled out, carrying `~` when `is_approximate`.
    pub human: String,
    /// `true` when the folder total or any returned child is a lower bound.
    ///
    /// Deliberately NOT `is_lower_bound`: the bounds run in both directions here.
    /// An understated folder total pulls the remainder DOWN, an understated child
    /// size pushes it UP, and the two can be in play at once — so the figure is
    /// uncertain in an unknown direction, and naming one would be false precision.
    pub is_approximate: bool,
}

/// What the un-returned children add up to, or `None` when that can't be said
/// honestly. A wrong remainder is worse than none: the model would state it as
/// fact.
///
/// Omitted when:
///
/// - `count == 0` — this page is the whole folder, so there's nothing to account
///   for and a zero would only invite interpretation.
/// - Any returned child has an unknown size. It's missing from the subtraction, so
///   the difference silently absorbs it and overstates what's left.
/// - The folder has no `dir_stats` total to subtract from (caller-side: the
///   `stats` argument is `None`).
/// - A `type` filter is active. `count` would then be "folders not shown" while
///   the folder's recursive total still counts every loose file, so the pair would
///   describe two different populations in one sentence ("the other 3 folders come
///   to 40 GB", where 38 GB of it is files that were never in the running).
fn remainder(stats: &DirStats, rows: &[ChildEntry], total: usize, opts: &ListOptions) -> Option<Remainder> {
    if opts.type_filter.is_some() {
        return None;
    }
    let count = total.saturating_sub(rows.len());
    if count == 0 {
        return None;
    }
    let mut shown: u64 = 0;
    let mut is_approximate = !stats.recursive_size_complete;
    for row in rows {
        // An unknown size ends the whole remainder: `?` returns `None` from here.
        let size = row.size?;
        shown = shown.saturating_add(size);
        is_approximate |= row.size_is_lower_bound;
    }
    // Saturating: a lower-bound folder total can be smaller than the children we
    // already listed, and a negative remainder isn't a thing to report.
    let bytes = stats.recursive_size.saturating_sub(shown);
    Some(Remainder {
        count,
        bytes,
        human: qualified_size(bytes, is_approximate.then_some("~")),
        is_approximate,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDirResult {
    pub path: String,
    pub coverage: Coverage,
    /// Which volume this folder is on, with its capacity and free space.
    pub volume: VolumeBlock,
    /// This folder's own recursive totals: what "how big is this folder" asks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<SizeStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ChildEntry>>,
    /// How many children matched `type`, before paging. Absent when the path isn't
    /// in the index (`children: None`) — never a wrong zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    /// How many children `children` actually carries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned: Option<usize>,
    /// Where this page started, so a caller can resume with `offset: offset + returned`.
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_zero")]
    pub offset: usize,
    /// `true` when children beyond this page exist — the caller's `limit`, the result
    /// budget, or both cut it.
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub truncated: bool,
    /// What the children this page didn't return add up to. Absent whenever it
    /// can't be stated honestly (see [`remainder`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remainder: Option<Remainder>,
    /// The order the rows are actually in. Echoed so a model that asked for a size
    /// ranking can trust the top row is the biggest.
    pub sorted_by: &'static str,
}

/// One page of children: the rows, and how many there were to page through.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Page {
    pub rows: Vec<ChildEntry>,
    /// Everything that matched `type`, before `offset`/`limit`.
    pub total: usize,
}

/// Order the children and cut the requested page. Pure, so the ordering rules are
/// testable without an index.
///
/// **Unknown sizes sort last in both directions.** A folder the index has no
/// `dir_stats` row for is unknown, not empty, so it must never lead a "biggest
/// first" ranking — and flipping to `asc` must not promote it either. Ties break
/// on name, so a page is stable and `offset` paging can't skip or repeat a row.
pub(crate) fn sort_and_page(mut children: Vec<ChildEntry>, opts: &ListOptions) -> Page {
    if let Some(filter) = opts.type_filter {
        children.retain(|c| match filter {
            TypeFilter::Files => !c.is_directory,
            TypeFilter::Dirs => c.is_directory,
        });
    }
    let total = children.len();
    children.sort_by(|a, b| {
        let ordering = match opts.sort_by {
            SortBy::Name => a.name.cmp(&b.name),
            SortBy::Size => compare_unknown_last(a.size, b.size, opts.order),
            SortBy::Modified => compare_unknown_last(a.modified, b.modified, opts.order),
        };
        let ordering = match (opts.sort_by, opts.order) {
            // `compare_unknown_last` has already applied the direction (it must, to
            // keep unknowns last either way); only name flips here.
            (SortBy::Name, Order::Desc) => ordering.reverse(),
            _ => ordering,
        };
        ordering.then_with(|| a.name.cmp(&b.name))
    });
    let rows = children.into_iter().skip(opts.offset).take(opts.limit).collect();
    Page { rows, total }
}

/// Compare two optional keys so that `None` always sorts AFTER any `Some`,
/// whichever direction the caller asked for.
fn compare_unknown_last(a: Option<u64>, b: Option<u64>, order: Order) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => match order {
            Order::Asc => a.cmp(&b),
            Order::Desc => b.cmp(&a),
        },
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Shape one directory's listing. Pure over the resolved inputs, so the coverage
/// flags are testable without a live index.
///
/// A huge folder is PAGED, not shipped whole: a 20k-entry listing dwarfs any prompt
/// budget, so the page is cut again to what one tool result may carry and the counts
/// say so (`total` / `returned` / `offset` / `truncated`). The order is deterministic,
/// so "the next `limit` after `offset`" is a stable, resumable window.
pub(crate) fn build_list_dir(
    path: &str,
    page: Option<Page>,
    stats: Option<&DirStats>,
    enabled: bool,
    freshness: Option<Freshness>,
    volume: VolumeBlock,
    opts: &ListOptions,
) -> ListDirResult {
    let indexed = page.is_some();
    let total = page.as_ref().map(|p| p.total);
    let fitted = page.map(|p| crate::mcp::fit_to_result_budget(p.rows));
    let returned = fitted.as_ref().map(|f| f.items.len());
    // More exists beyond this page when the rows we're returning don't reach the end
    // of what matched — whether `limit` or the budget did the cutting.
    let truncated = match (total, returned) {
        (Some(total), Some(returned)) => opts.offset.saturating_add(returned) < total,
        _ => false,
    };
    let remainder = match (fitted.as_ref(), total, stats) {
        (Some(fitted), Some(total), Some(stats)) => remainder(stats, &fitted.items, total, opts),
        _ => None,
    };
    ListDirResult {
        path: path.to_string(),
        coverage: coverage(enabled, freshness, indexed),
        volume,
        size: stats.map(SizeStats::from_dir_stats),
        total,
        returned,
        offset: opts.offset,
        truncated,
        remainder,
        sorted_by: opts.sort_by.token(),
        children: fitted.map(|f| f.items),
    }
}

pub fn list_dir_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute or ~-relative folder. To find where space goes, start at a volume root or ~ with sortBy size and drill into the biggest child. Relay the coverage block and any lower-bound size rather than a total the index can't back." },
            "sortBy": {
                "type": "string",
                "enum": ["name", "size", "modified"],
                "description": "size ranks files and folders together by space used (a folder by its recursive total). Default name."
            },
            "order": { "type": "string", "enum": ["asc", "desc"], "description": "Default desc for size and modified, asc for name." },
            "limit": { "type": "integer", "description": "Default 50, max 1000; a page may come back shorter to fit one result." },
            "offset": { "type": "integer", "description": "Children to skip; resume with offset + returned." },
            "type": { "type": "string", "enum": ["file", "dir"], "description": "Only files or only folders; omit for both." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

pub async fn execute_list_dir<R: Runtime>(_app: &AppHandle<R>, params: &Value) -> ToolResult {
    let path = required_path(params)?;
    let opts = ListOptions::from_params(params)?;
    let rows = index()
        .list_children(&path)
        .map_err(|e| ToolError::internal(e.to_string()))?;
    let stats = index()
        .dir_stats(&path)
        .map_err(|e| ToolError::internal(e.to_string()))?;
    let status = index().volume_status_for_path(&path);
    let space = crate::mcp::resources::volumes::space_summary(&status.volume_id);

    let page = match rows {
        None => None,
        Some(rows) => {
            let children: Vec<ChildEntry> = rows.iter().map(child_from_row).collect();
            Some(page_with_folder_sizes(&path, children, &opts)?)
        }
    };
    let volume = VolumeBlock::new(status.volume_id.clone(), space);
    let result = build_list_dir(
        &path,
        page,
        stats.as_ref(),
        status.enabled,
        status.freshness,
        volume,
        &opts,
    );
    serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))
}

/// Page the children, giving every folder row the recursive size that makes it
/// comparable with a file.
///
/// The order of the two steps is the whole trick. Ranking BY size needs every
/// folder's size before the sort, so the batch runs over the whole folder; any
/// other order pages first and enriches only the rows that survived, which keeps a
/// browse of a 20k-entry folder to one small batch. Same rows either way — only the
/// number of `dir_stats` lookups differs.
fn page_with_folder_sizes(path: &str, children: Vec<ChildEntry>, opts: &ListOptions) -> Result<Page, ToolError> {
    if opts.sort_by == SortBy::Size {
        let children = with_folder_sizes(path, children)?;
        return Ok(sort_and_page(children, opts));
    }
    let page = sort_and_page(children, opts);
    Ok(Page {
        rows: with_folder_sizes(path, page.rows)?,
        total: page.total,
    })
}

/// Replace each folder row's own inode size with its recursive total from
/// `dir_stats`, flagging the ones that are lower bounds. Files pass through
/// untouched; a folder with no stats row keeps `size: None` (unknown, never zero).
fn with_folder_sizes(path: &str, mut children: Vec<ChildEntry>) -> Result<Vec<ChildEntry>, ToolError> {
    let dir_indices: Vec<usize> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_directory && !c.is_symlink)
        .map(|(i, _)| i)
        .collect();
    if dir_indices.is_empty() {
        return Ok(children);
    }
    let paths: Vec<String> = dir_indices
        .iter()
        .map(|&i| join_child_path(path, &children[i].name))
        .collect();
    let stats = index()
        .dir_stats_batch(&paths)
        .map_err(|e| ToolError::internal(e.to_string()))?;
    for (&i, stats) in dir_indices.iter().zip(stats) {
        match stats {
            Some(stats) => children[i].set_size(Some(stats.recursive_size), !stats.recursive_size_complete),
            // No stats row: the index knows the folder but not its total. Say
            // nothing rather than pass its inode size off as a total.
            None => children[i].set_size(None, false),
        }
    }
    Ok(children)
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// The required `path` param, tilde-expanded (agents send `~/…`).
fn required_path(params: &Value) -> Result<String, ToolError> {
    let raw = params
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter"))?;
    Ok(expand_tilde(raw))
}

/// Map an index `EntryRow` into the model-facing child shape. Typed against the
/// row's fields. A folder's `size` starts as its own logical size and is replaced
/// by the recursive total in [`with_folder_sizes`]; a file's is already final.
fn child_from_row(row: &cmdr_index::store::EntryRow) -> ChildEntry {
    ChildEntry::new(
        row.name.clone(),
        row.is_directory,
        row.is_symlink,
        row.logical_size,
        false,
        row.modified_at,
    )
}

#[cfg(test)]
mod tests;
