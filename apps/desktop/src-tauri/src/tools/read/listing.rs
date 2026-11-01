//! The `list_dir` tool, over the drive index, one live read where no index lists
//! the folder, or the index a not-connected volume left on disk.
//!
//! The location is placed ONCE by `crate::location`'s resolver, and that volume's
//! index answers: a dead mount point is a typed miss, ❌ never a second routing of
//! the path onto `root`'s index (`tools/DETAILS.md` § The location contract).
//!
//! It REUSES the shipped index query cores (`Index::list_children`, `dir_stats`,
//! `dir_stats_batch`) and never re-derives listing logic. A connected volume whose
//! index doesn't list the folder answers from one bounded readdir instead
//! (`listing/live.rs`, § The live source); nothing else here touches the disk. A
//! volume that isn't connected answers from the same cores, which read its
//! database read-only (`source: offlineIndex`). Which of those answers, across
//! every proven member of the folder's share, is `listing/readers.rs`'s call
//! ([`answer_read`]). The one thing no index query does is rank a folder's children by size,
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
//!
//! A child row and the vocabulary for its size live in `listing/rows.rs`; this file
//! is the pipeline that fills them.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};
use tokio_util::sync::CancellationToken;

use super::join_child_path;
use crate::file_system::volume::SpaceInfo;
use crate::file_system::volume::manager::get_volume_manager;
use crate::index_host::index;
use crate::location::Location;
use crate::location::miss::LocationMissResult;
use crate::location::resolve::{LocationInput, VolumeFacts, resolve};
use crate::search::format_size;
use crate::tools::{ToolError, ToolResult};
use cmdr_index::store::DirStats;
use cmdr_index::{Freshness, IndexStatusResponse};

mod live;
mod readers;
mod rows;
mod source;

pub(crate) use live::{GatePolicy, LIVE_READ, LiveAsk, answer_live};
pub(crate) use readers::{LiveDeps, answer_read};
use rows::qualified_size;
pub(crate) use rows::{ChildEntry, RowKind, SizeClaim, human_size};
use source::index_coverage;
pub(crate) use source::{Coverage, IndexReach, ListSource};

/// Rows per page when the caller doesn't say. Comfortably under the result budget
/// for a normal folder, so the common call is one round trip.
const DEFAULT_LIMIT: usize = 50;
/// The ceiling on `limit`. `fit_to_result_budget` is the real cut; this only stops
/// a caller from asking for a page nothing could carry.
const MAX_LIMIT: usize = 1_000;

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
    /// `recursive_size`, spelled out — with `≥` (settled lower bound) or `~` (still
    /// moving) inside the string, so a quoted total can't shed its caveat.
    pub recursive_size_human: String,
    pub recursive_file_count: u64,
    pub recursive_dir_count: u64,
    /// `true` when `recursive_size` is a lower bound, not an exact total (some
    /// subtree was never fully listed).
    pub size_is_lower_bound: bool,
    /// `true` when the exact size was computed at an older volume epoch (stale).
    pub size_is_stale: bool,
    /// `true` while the indexer can still move this total: a walk is on this
    /// folder, above it, or below it (the roll-up repairs ancestors), or its own
    /// index writes are still draining. ⚠️ Both halves matter — reading only the
    /// per-folder pending flag calls a folder settled through the whole walk that
    /// is rewriting it, which is when its number is furthest from the truth.
    pub size_is_updating: bool,
    /// `true` if a descendant is a symlink (so the size may omit linked content).
    pub has_symlinks: bool,
}

impl SizeStats {
    /// `walk_moves_it` is the volume-level half of "still moving" (see
    /// [`IndexStatusResponse::walk_affects`]); `DirStats` carries the per-folder half.
    fn from_dir_stats(s: &DirStats, walk_moves_it: bool) -> Self {
        let is_updating = s.recursive_size_pending || walk_moves_it;
        Self {
            recursive_size: s.recursive_size,
            recursive_size_human: human_size(s.recursive_size, !s.recursive_size_complete, is_updating),
            recursive_file_count: s.recursive_file_count,
            recursive_dir_count: s.recursive_dir_count,
            size_is_lower_bound: !s.recursive_size_complete,
            size_is_stale: s.recursive_size_stale,
            size_is_updating: is_updating,
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

/// The volume a listing sits on, as it stands at the moment of the call.
///
/// One value rather than four arguments because all four answers must describe the
/// SAME volume at the SAME instant. Passed separately, `walked_roots` resolved for
/// one volume could sit beside an `enabled` read from another, and every size on
/// the page would quietly mis-report; as one value that's unrepresentable.
pub(crate) struct VolumeView {
    /// Whether this volume is indexed at all.
    pub enabled: bool,
    /// The index's freshness token, or `None` when the path resolves to no volume.
    pub freshness: Option<Freshness>,
    /// The part of this view the model sees: the volume's id and how full it is.
    pub block: VolumeBlock,
    /// The ground a walker holds right now, so a folder it's moving can say so.
    /// Empty means nothing is moving, which is also the answer for a volume whose
    /// status can't be read: a missing status must not invent motion.
    pub walked_roots: Vec<String>,
    /// Which copy of the index answers for it.
    pub reach: IndexReach,
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
    /// The folder this answers for, as a `{ volumeId, path }` a next call can take.
    pub location: Location,
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
    /// `true` when `total` counts only what a capped live read kept, so the folder
    /// holds at least that many.
    #[serde(skip_serializing_if = "crate::tools::read::is_false")]
    pub total_is_lower_bound: bool,
    /// How many children `children` actually carries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned: Option<usize>,
    /// Where this page started, so a caller can resume with `offset: offset + returned`.
    #[serde(skip_serializing_if = "crate::tools::read::is_zero")]
    pub offset: usize,
    /// `true` when children beyond this page exist — the caller's `limit`, the result
    /// budget, or both cut it.
    #[serde(skip_serializing_if = "crate::tools::read::is_false")]
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
    location: &Location,
    page: Option<Page>,
    stats: Option<&DirStats>,
    volume: &VolumeView,
    opts: &ListOptions,
) -> ListDirResult {
    let indexed = page.is_some();
    let total = page.as_ref().map(|p| p.total);
    let fitted = page.map(|p| crate::tools::fit_to_result_budget(p.rows));
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
    let path = location.path.as_str();
    ListDirResult {
        location: location.clone(),
        coverage: index_coverage(volume, indexed),
        volume: volume.block.clone(),
        size: stats
            .map(|s| SizeStats::from_dir_stats(s, IndexStatusResponse::walk_affects(&volume.walked_roots, path))),
        total,
        total_is_lower_bound: false,
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
            "path": { "type": "string", "description": "Absolute or ~-relative folder to list. Relay the coverage block and any lower-bound size rather than a total the index can't back." },
            "volumeId": { "type": "string", "description": "From list_volumes or a result's location; omit for a path on a connected volume." },
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

/// The registry's handler. An AI client's call has no turn to stop it, but
/// dropping the request still stops a live read (`live::run_bounded`).
pub async fn execute_list_dir<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    execute_list_dir_cancellable(app, params, &CancellationToken::new()).await
}

/// `list_dir` with the turn's cancel, so Stop ends a live read that's waiting
/// on a slow volume (`agent::tool_gate::dispatch` routes the agent's calls
/// here).
pub(crate) async fn execute_list_dir_cancellable<R: Runtime>(
    _app: &AppHandle<R>,
    params: &Value,
    cancel: &CancellationToken,
) -> ToolResult {
    let input =
        LocationInput::from_params(params).ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter"))?;
    let opts = ListOptions::from_params(params)?;
    let facts = VolumeFacts::now().await;
    let resolved = match resolve(&facts, &input) {
        Ok(resolved) => resolved,
        Err(miss) => return to_value(&LocationMissResult::from(miss)),
    };
    let volume_for = |id: &str| get_volume_manager().get(id);
    answer_read(
        &facts,
        &resolved,
        &opts,
        LiveDeps {
            cancel,
            cfg: LIVE_READ,
            policy: GatePolicy::now(),
            volume_for: &volume_for,
        },
    )
    .await
}

/// Shape a folder the index lists: its rows, its own totals, and each child
/// folder's recursive size, all from `index_id`.
fn answer_from_index(
    location: &Location,
    index_id: &str,
    rows: &[cmdr_index::store::EntryRow],
    volume: &VolumeView,
    opts: &ListOptions,
) -> ToolResult {
    let stats = index()
        .dir_stats(index_id, &location.path)
        .map_err(|e| ToolError::internal(e.to_string()))?;
    let children: Vec<ChildEntry> = rows.iter().map(child_from_row).collect();
    let folder = IndexedFolder {
        index_id,
        path: &location.path,
    };
    let page = page_with_folder_sizes(&folder, children, opts, &volume.walked_roots)?;
    to_value(&build_list_dir(location, Some(page), stats.as_ref(), volume, opts))
}

/// The listed folder as the index knows it: which index holds its rows, and its
/// app path. One value, so a child's size can't be asked of a different index
/// than the one that listed it.
struct IndexedFolder<'a> {
    index_id: &'a str,
    path: &'a str,
}

fn to_value(result: &impl Serialize) -> ToolResult {
    serde_json::to_value(result).map_err(|e| ToolError::internal(e.to_string()))
}

/// Page the children, giving every folder row the recursive size that makes it
/// comparable with a file.
///
/// The order of the two steps is the whole trick. Ranking BY size needs every
/// folder's size before the sort, so the batch runs over the whole folder; any
/// other order pages first and enriches only the rows that survived, which keeps a
/// browse of a 20k-entry folder to one small batch. Same rows either way — only the
/// number of `dir_stats` lookups differs.
fn page_with_folder_sizes(
    folder: &IndexedFolder<'_>,
    children: Vec<ChildEntry>,
    opts: &ListOptions,
    walked_roots: &[String],
) -> Result<Page, ToolError> {
    if opts.sort_by == SortBy::Size {
        let children = with_folder_sizes(folder, children, walked_roots)?;
        return Ok(sort_and_page(children, opts));
    }
    let page = sort_and_page(children, opts);
    Ok(Page {
        rows: with_folder_sizes(folder, page.rows, walked_roots)?,
        total: page.total,
    })
}

/// Replace each folder row's own inode size with its recursive total from
/// `dir_stats`, flagging the ones that are lower bounds and the ones still moving.
/// Files pass through untouched; a folder with no stats row keeps `size: None`
/// (unknown, never zero).
///
/// The walk test is PER CHILD, not inherited from the listed folder: a walker deep
/// inside one child moves that child and the folder above it, and leaves its
/// siblings settled.
fn with_folder_sizes(
    folder: &IndexedFolder<'_>,
    mut children: Vec<ChildEntry>,
    walked_roots: &[String],
) -> Result<Vec<ChildEntry>, ToolError> {
    let dir_indices: Vec<usize> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind() == RowKind::Folder)
        .map(|(i, _)| i)
        .collect();
    if dir_indices.is_empty() {
        return Ok(children);
    }
    let paths: Vec<String> = dir_indices
        .iter()
        .map(|&i| join_child_path(folder.path, &children[i].name))
        .collect();
    let stats = index()
        .dir_stats_batch(folder.index_id, &paths)
        .map_err(|e| ToolError::internal(e.to_string()))?;
    for ((&i, stats), child_path) in dir_indices.iter().zip(stats).zip(&paths) {
        match stats {
            Some(stats) => children[i].set_size(SizeClaim {
                bytes: Some(stats.recursive_size),
                is_lower_bound: !stats.recursive_size_complete,
                is_updating: stats.recursive_size_pending
                    || IndexStatusResponse::walk_affects(walked_roots, child_path),
            }),
            // No stats row: the index knows the folder but not its total. Say
            // nothing rather than pass its inode size off as a total.
            None => children[i].set_size(SizeClaim::unknown()),
        }
    }
    Ok(children)
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Map an index `EntryRow` into the model-facing child shape. Typed against the
/// row's fields. A folder's `size` starts as its own logical size and is replaced
/// by the recursive total in [`with_folder_sizes`]; a file's is already final.
fn child_from_row(row: &cmdr_index::store::EntryRow) -> ChildEntry {
    // The claim starts EXACT: this is the row's own inode size, which is settled and
    // complete. A folder's recursive total, and the caveats that come with it,
    // arrives in `with_folder_sizes`.
    ChildEntry::new(
        row.name.clone(),
        RowKind::of(row.is_directory, row.is_symlink),
        SizeClaim::exact(row.logical_size),
        row.modified_at,
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod live_shape_tests;
