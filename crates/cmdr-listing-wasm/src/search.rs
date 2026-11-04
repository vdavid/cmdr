//! ⌘F in the browser: the drive search's core (`cmdr-search`) over the demo's one
//! in-memory volume, answering the search commands the way the desktop's
//! `commands/search.rs` does. The demo's third engine host, beside the listing and
//! the viewer.
//!
//! The volume is the whole index: [`DemoSearch`] builds a `MemorySource` from what
//! the store holds and rebuilds it when the folder-size fold has seen a change (a
//! few thousand rows, cheaper than any staleness policy). Folder sizes come from that
//! fold (`folder_sizes.rs`), read per query, so a size filter reads the numbers the
//! panes show. A live run covers everything at once: the covered half, then the one
//! terminal report with nothing left to walk. Recent searches last the session, kept
//! by `cmdr-recents`' rules over `cmdr_search::history`'s key.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{HeldAt, InMemoryVolume};
use cmdr_listing::{DirectorySortMode, NameCollator, SearchSortRow, SortColumn, SortOrder, sort_search_rows};
use cmdr_search::history::{DEFAULT_MAX_COUNT, HistoryEntry};
use cmdr_search::ranking::ImportanceWeights;
use cmdr_search::run::{
    CoverageFacts, CoverageKind, ResultStream, RunEnded, RunFailed, RunOrigin, RunProgress, RunSink, SearchPhase,
    SearchRunCoverage, SearchRunError, SearchRuns, UnresolvedScope, WalkEnding,
};
use cmdr_search::{
    ARENA_ROOT_ID, CoveredHalf, DirSizeSource, MemoryRow, MemorySource, NameFold, NameRules, SearchContext,
    SearchError, SearchQuery, SearchResult, SearchResultEntry, SearchRows, SourceError, query, search_covered,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events::EventSink;
use crate::folder_sizes::FolderSizes;
use crate::invoke::{ok, parse};

/// Every command [`DemoSearch::invoke`] answers, by its Tauri name.
pub const SEARCH_COMMANDS: &[&str] = &[
    "prepare_search_index",
    "search_files",
    "search_files_streaming",
    "cancel_search",
    "release_search_index",
    "parse_search_scope",
    "get_system_dir_excludes",
    "sort_search_results",
    "get_recent_searches",
    "add_recent_search",
    "remove_recent_search",
    "clear_recent_searches",
    "apply_recent_searches_max_count",
];

/// The demo's name rules: a Mac's case rule and fold (it presents as one), with
/// patterns left in the form typed, because the tree's names are the browser's NFC
/// strings and an NFD pattern would miss every accented one.
pub const DEMO_NAME_RULES: NameRules = NameRules {
    case_insensitive_by_default: true,
    nfd_patterns: false,
    fold: NameFold::NfdLowercase,
};

/// The search over one in-memory volume.
pub struct DemoSearch {
    volume_id: String,
    store: Arc<InMemoryVolume>,
    sizes: Arc<FolderSizes>,
    sink: Arc<dyn EventSink>,
    home: Option<String>,
    collator: Arc<NameCollator>,
    runs: SearchRuns,
    rows: Mutex<Option<Arc<Rows>>>,
    recents: Mutex<Vec<HistoryEntry>>,
    minted_ids: AtomicU64,
}

/// The volume as a search source, as of one change count of the fold.
struct Rows {
    changes: u64,
    source: MemorySource,
    /// Every folder's path by its id, the root's included: what the fold is asked.
    folders: HashMap<i64, PathBuf>,
}

impl DemoSearch {
    /// The search over `store` (filed under `volume_id`), sizing folders through
    /// `sizes`, ordering a results pane with `collator` (the panes' own), and
    /// writing `~` for `home` in a result's parent path.
    pub fn new(
        volume_id: String,
        store: Arc<InMemoryVolume>,
        sizes: Arc<FolderSizes>,
        sink: Arc<dyn EventSink>,
        home: Option<String>,
        collator: Arc<NameCollator>,
    ) -> Self {
        Self {
            volume_id,
            store,
            sizes,
            sink,
            home,
            collator,
            runs: SearchRuns::new(),
            rows: Mutex::new(None),
            recents: Mutex::new(Vec::new()),
            minted_ids: AtomicU64::new(0),
        }
    }

    /// Whether [`Self::invoke`] answers `command`.
    pub fn handles(command: &str) -> bool {
        SEARCH_COMMANDS.contains(&command)
    }

    /// Runs `command` with Tauri-style `args` (camelCase keys).
    pub fn invoke(&self, command: &str, args: Value) -> Result<Value, Value> {
        match command {
            "prepare_search_index" => {
                // The volume IS the index: always ready, never loading.
                let rows = self.rows();
                ok(PrepareResult {
                    ready: true,
                    entry_count: rows.source.arena().len() as u64,
                    loading: false,
                })
            }
            "search_files" => {
                let a: QueryArgs = parse(command, args)?;
                self.search_files(&a.query).map_or_else(|e| Err(Value::String(e)), ok)
            }
            "search_files_streaming" => {
                let a: StreamingArgs = parse(command, args)?;
                ok(self.search_streaming(&a.query, a.run_id, a.order))
            }
            "cancel_search" => {
                let a: RunIdArgs = parse(command, args)?;
                ok(self.runs.cancel(&a.run_id))
            }
            "release_search_index" => {
                // The arena stays (it's tiny); closing the dialog only stops its runs.
                let a: ReleaseArgs = parse(command, args)?;
                self.runs.cancel_dialog_runs_except(a.keep_run_id.as_deref());
                Ok(Value::Null)
            }
            "parse_search_scope" => {
                let a: ScopeArgs = parse(command, args)?;
                ok(query::parse_scope(&a.scope, self.home.as_deref()))
            }
            "get_system_dir_excludes" => ok(query::SYSTEM_DIR_EXCLUDES),
            "sort_search_results" => {
                let a: SortArgs = parse(command, args)?;
                ok(sort_search_rows(
                    &a.rows,
                    a.sort_by,
                    a.sort_order,
                    a.dir_sort_mode,
                    &self.collator,
                ))
            }
            "get_recent_searches" => {
                let a: LimitArgs = parse(command, args)?;
                let recents = self.recents.lock_ignore_poison();
                let limit = a.limit.map_or(recents.len(), |n| n as usize);
                ok(&recents[..limit.min(recents.len())])
            }
            "add_recent_search" => {
                let a: AddRecentArgs = parse(command, args)?;
                let cap = a.max_count.map_or(DEFAULT_MAX_COUNT, |n| n as usize);
                let mut recents = self.recents.lock_ignore_poison();
                cmdr_recents::add_to(&mut recents, a.entry, cap, || {
                    format!("demo-{}", self.minted_ids.fetch_add(1, Ordering::Relaxed))
                });
                Ok(Value::Null)
            }
            "remove_recent_search" => {
                let a: IdArgs = parse(command, args)?;
                cmdr_recents::remove_from(&mut self.recents.lock_ignore_poison(), &a.id);
                Ok(Value::Null)
            }
            "clear_recent_searches" => {
                self.recents.lock_ignore_poison().clear();
                Ok(Value::Null)
            }
            "apply_recent_searches_max_count" => {
                let a: MaxCountArgs = parse(command, args)?;
                cmdr_recents::trim_to(&mut self.recents.lock_ignore_poison(), a.max_count as usize);
                Ok(Value::Null)
            }
            _ => Err(Value::String(format!("the browser demo doesn't answer `{command}`"))),
        }
    }

    /// `search_files`: the whole answer at once.
    fn search_files(&self, query: &SearchQuery) -> Result<SearchResult, String> {
        let half = self.covered_half(query).map_err(|e| e.to_string())?;
        Ok(SearchResult {
            entries: half.entries,
            total_count: half.total,
            uncovered_scopes: Vec::new(),
            unresolved_scopes: half.unresolved_scopes,
            target_volume_id: self.volume_id.clone(),
            hidden_by_excludes: half.hidden_by_excludes,
        })
    }

    /// `search_files_streaming`: a live run over ground that's all covered, start to
    /// terminal event, before the command answers. The events reach the page a task
    /// later (the page's sink defers each one), so the dialog has registered the run
    /// by the time they land.
    fn search_streaming(&self, query: &SearchQuery, run_id: String, order: u64) -> LiveSearchStart {
        let run = self
            .runs
            .register(&run_id, &self.volume_id, RunOrigin::Dialog { order });
        let sink = WireSink(self.sink.as_ref());
        let mut stream = ResultStream::new(&run, &sink, query);
        stream.announce(SearchPhase::ResolvingCoverage);
        match self.covered_half(query) {
            Ok(half) => {
                stream.add_indexed(half.entries, half.total, half.hidden_by_excludes);
                stream.finish(CoverageFacts {
                    unresolved_scopes: half
                        .unresolved_scopes
                        .into_iter()
                        .map(|path| UnresolvedScope { path, walked: false })
                        .collect(),
                    ..CoverageFacts::new(WalkEnding::NothingToWalk, CoverageKind::Covered, self.volume_id.clone())
                });
            }
            Err(SearchError::Query(error)) => stream.fail(SearchRunError::Query, error.to_string()),
            Err(SearchError::SourceUnreadable(error)) => {
                // The fold never fails a read; say it the way the desktop says an
                // unreadable index, should one ever.
                log::warn!(target: "search", "demo search: {error}");
                stream.fail(SearchRunError::IndexUnreadable, INDEX_UNREADABLE.to_string());
            }
        }
        self.runs.deregister(&run_id);
        LiveSearchStart {
            run_id,
            target_volume_id: self.volume_id.clone(),
        }
    }

    /// One pass of the core over the volume. Each scope pairs as written with its
    /// canonical form, as the desktop's `scope_pairs` does: `~` expanded, then the
    /// volume's own symlinks resolved (`InMemoryVolume::canonicalize`, the page's
    /// `realpath`), the literal kept where that fails, as `realpath`'s failure keeps it.
    fn covered_half(&self, query: &SearchQuery) -> Result<CoveredHalf, SearchError> {
        let rows = self.rows();
        let home = self.home.as_deref();
        let scope: Vec<(String, String)> = query
            .include_paths
            .iter()
            .flatten()
            .map(|path| {
                let expanded = query::expand_tilde(path, home).into_owned();
                let canonical = self
                    .store
                    .canonicalize(Path::new(&expanded))
                    .map_or(expanded, |real| real.to_string_lossy().into_owned());
                (path.clone(), canonical)
            })
            .collect();
        let weights = ImportanceWeights::empty();
        let ctx = SearchContext {
            rules: DEMO_NAME_RULES,
            home_dir: home,
            weights: &weights,
        };
        let sizes = FoldSizes {
            rows: &rows,
            sizes: &self.sizes,
        };
        search_covered(&rows.source, &sizes, query, &scope, &ctx)
    }

    /// The source as of the fold's latest change count, rebuilt when it moved.
    fn rows(&self) -> Arc<Rows> {
        let changes = self.sizes.changes();
        let mut cached = self.rows.lock_ignore_poison();
        if let Some(rows) = cached.as_ref().filter(|rows| rows.changes == changes) {
            return Arc::clone(rows);
        }
        let rows = Arc::new(self.build(changes));
        *cached = Some(Arc::clone(&rows));
        rows
    }

    fn build(&self, changes: u64) -> Rows {
        let mut dirs = Vec::new();
        let rows: Vec<MemoryRow> = self
            .store
            .held_entries()
            .into_iter()
            .map(|entry| {
                // A symlink is a leaf, as the drive index stores it: never followed.
                let is_directory = entry.is_directory && !entry.is_symlink;
                if is_directory {
                    dirs.push(entry.path.clone());
                }
                MemoryRow {
                    path: entry.path,
                    is_directory,
                    size: if is_directory { None } else { entry.size },
                    modified_at: entry.modified_at,
                }
            })
            .collect();
        // The store is a tree by construction; a refusal here is a broken store.
        let source = MemorySource::new(rows, "", DEMO_NAME_RULES).unwrap_or_else(|error| {
            log::error!(target: "search", "demo search: the volume isn't a tree: {error}");
            MemorySource::new(Vec::new(), "", DEMO_NAME_RULES).expect("an empty tree is a tree")
        });
        let mut folders: HashMap<i64, PathBuf> = dirs
            .into_iter()
            .filter_map(|path| Some((source.id_of(&path)?, PathBuf::from(path))))
            .collect();
        folders.insert(ARENA_ROOT_ID, PathBuf::from("/"));
        Rows {
            changes,
            source,
            folders,
        }
    }
}

/// The fold as the core's folder-size source: one truth for panes, operations, and
/// search, read per query.
struct FoldSizes<'a> {
    rows: &'a Rows,
    sizes: &'a FolderSizes,
}

impl FoldSizes<'_> {
    fn size_of(held: Option<HeldAt>) -> Option<u64> {
        match held? {
            HeldAt::Folder(totals) => Some(totals.size),
            HeldAt::Leaf { .. } => None,
        }
    }
}

impl DirSizeSource for FoldSizes<'_> {
    fn in_range(&self, min: Option<u64>, max: Option<u64>) -> Result<HashMap<i64, u64>, SourceError> {
        let (ids, paths): (Vec<i64>, Vec<PathBuf>) = self.rows.folders.iter().map(|(id, p)| (*id, p.clone())).unzip();
        Ok(ids
            .into_iter()
            .zip(self.sizes.held_at(&paths))
            .filter_map(|(id, held)| Some((id, Self::size_of(held)?)))
            .filter(|&(_, size)| min.is_none_or(|min| size >= min) && max.is_none_or(|max| size <= max))
            .collect())
    }

    fn of(&self, ids: &[i64]) -> Result<Vec<Option<u64>>, SourceError> {
        Ok(ids
            .iter()
            .map(|id| {
                let path = self.rows.folders.get(id)?;
                Self::size_of(self.sizes.held_at(std::slice::from_ref(path)).pop().flatten())
            })
            .collect())
    }
}

/// What a run says when its source can't be read, the desktop's sentence.
const INDEX_UNREADABLE: &str = "Cmdr can’t read this drive’s index. Re-indexing the drive fixes it.";

/// A run's facts onto the page's events, by the desktop's event names and payloads
/// (`apps/desktop/src-tauri/src/search/live/events.rs`).
struct WireSink<'a>(&'a dyn EventSink);

impl WireSink<'_> {
    fn send(&self, event: &str, payload: impl Serialize) {
        match serde_json::to_value(payload) {
            Ok(value) => self.0.emit(event, value),
            Err(error) => log::error!(target: "search", "demo search: `{event}` didn't serialize: {error}"),
        }
    }
}

impl RunSink for WireSink<'_> {
    fn progress(&self, fact: RunProgress) {
        self.send(
            SEARCH_PROGRESS,
            ProgressPayload {
                run_id: fact.run_id,
                phase: fact.phase,
                target_volume_id: fact.target_volume_id,
                entries: fact.entries,
                match_count: fact.match_count,
                dirs_found: fact.dirs_found,
                current_path: fact.current_path,
                capped: fact.capped,
            },
        );
    }

    fn complete(&self, fact: RunEnded) {
        self.send(SEARCH_COMPLETE, EndedPayload::from(fact));
    }

    fn cancelled(&self, fact: RunEnded) {
        self.send(SEARCH_CANCELLED, EndedPayload::from(fact));
    }

    fn failed(&self, fact: RunFailed) {
        self.send(
            SEARCH_ERROR,
            ErrorPayload {
                run_id: fact.run_id,
                error: fact.error,
                message: fact.message,
            },
        );
    }
}

/// The live run's four events, by their Tauri names (`events_test.rs` pins them).
pub const SEARCH_PROGRESS: &str = "search-progress";
/// The run finished on its own terms.
pub const SEARCH_COMPLETE: &str = "search-complete";
/// Somebody stopped the run.
pub const SEARCH_CANCELLED: &str = "search-cancelled";
/// The run couldn't run.
pub const SEARCH_ERROR: &str = "search-error";

/// `SearchProgressEvent`'s payload.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProgressPayload {
    run_id: String,
    phase: SearchPhase,
    target_volume_id: String,
    entries: Vec<SearchResultEntry>,
    match_count: u32,
    dirs_found: u64,
    current_path: Option<String>,
    capped: bool,
}

/// `SearchCompleteEvent`'s and `SearchCancelledEvent`'s payload.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EndedPayload {
    run_id: String,
    match_count: u32,
    coverage: SearchRunCoverage,
}

impl From<RunEnded> for EndedPayload {
    fn from(fact: RunEnded) -> Self {
        Self {
            run_id: fact.run_id,
            match_count: fact.match_count,
            coverage: fact.coverage,
        }
    }
}

/// `SearchErrorEvent`'s payload.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorPayload {
    run_id: String,
    error: SearchRunError,
    message: String,
}

/// `PrepareResult` in the app.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareResult {
    ready: bool,
    entry_count: u64,
    loading: bool,
}

/// `LiveSearchStart` in the app.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveSearchStart {
    run_id: String,
    target_volume_id: String,
}

// The argument shapes, one per command signature in the desktop's `commands/search.rs`.

#[derive(Deserialize)]
struct QueryArgs {
    query: SearchQuery,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamingArgs {
    query: SearchQuery,
    run_id: String,
    order: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunIdArgs {
    run_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseArgs {
    keep_run_id: Option<String>,
}

#[derive(Deserialize)]
struct ScopeArgs {
    scope: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SortArgs {
    rows: Vec<SearchSortRow>,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
}

#[derive(Deserialize)]
struct LimitArgs {
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddRecentArgs {
    entry: HistoryEntry,
    max_count: Option<u32>,
}

#[derive(Deserialize)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaxCountArgs {
    max_count: u32,
}
