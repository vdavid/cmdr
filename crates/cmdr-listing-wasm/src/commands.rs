//! The listing commands, answered the way the desktop's Tauri commands answer
//! them (`apps/desktop/src-tauri/src/commands/file_system/listing.rs`): the same
//! command names, the same camelCase arguments, the same JSON back.
//!
//! [`Demo::invoke`] is shaped like Tauri's `invoke`: `Ok` is the command's
//! answer, `Err` what the promise rejects with (the command's serialized error,
//! or a string for arguments that don't parse, as Tauri does).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cmdr_brief_columns::{
    BriefColumnsIpcError, BriefColumnsQuery, FontMetrics, FontMetricsError, FontMetricsStore,
    compute_brief_column_text_widths, widths_from_parallel,
};
use cmdr_fs::deadline::TimedOut;
use cmdr_fs::volume::{ResolvedVolume, Volume};
use cmdr_listing::host::{ListingVolumes, Unregistered};
use cmdr_listing::{
    CompareDirectoriesError, CompareDirectoriesMode, DirectorySortMode, ListingId, ListingRequest, Listings, OnNoMatch,
    PaneMarks, RefreshOutcome, RowBeside, RowSpan, SortColumn, SortOrder,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use cmdr_fs::volume::host::VolumeHost;

use crate::backend_listings::BackendListings;
use crate::events::{DemoEvents, EventSink, ListingPathEvent};
use crate::folder_sizes::FolderSizes;
use crate::operations_host::DemoOperations;
use crate::tree::{DemoTree, TreeError};

/// Every command [`Demo::invoke`] answers, by its Tauri name.
pub const COMMANDS: &[&str] = &[
    "list_directory_start_streaming",
    "cancel_listing",
    "resort_listing",
    "get_file_range",
    "compare_directories",
    "find_file_index",
    "find_file_indices",
    "find_first_fuzzy_match",
    "get_file_at",
    "get_file_beside",
    "list_directory_end",
    "keep_listings_alive",
    "set_listing_include_hidden",
    "get_selection_snapshot",
    "get_selection_entries",
    "set_listing_name_filter",
    "refresh_listing",
    "get_listing_stats",
    "refresh_listing_index_sizes",
    // The drive index's folder stats (`commands/indexing.rs` in the app), off
    // the volume's fold: the status bar, selection info, and the `..` row.
    "get_dir_stats",
    "get_dir_stats_batch",
    // Brief mode's column widths (`commands/font_metrics.rs` and `listing.rs` in the
    // app). The page measures its own fonts and stores them, as the desktop's does.
    "store_font_metrics",
    "extend_font_metrics",
    "has_font_metrics",
    "get_brief_column_text_widths",
];

/// How fast the demo's one device streams, slow enough to watch a copy's
/// progress and pause it in the queue (`DETAILS.md` § Writes has the measured rate).
pub const DEVICE_BYTES_PER_SEC: u64 = 5 * 1024 * 1024;

/// How long engine work holds the page before it breathes (invariant: no
/// stretch past about 50 ms; `smoke.mjs` probes it).
pub const DEVICE_SLICE: Duration = Duration::from_millis(8);

/// The device's size, the same disk the volume list shows: free space is what
/// the files leave of it, live.
pub const DEVICE_CAPACITY_BYTES: u64 = 994_662_584_320;

/// One engine over one in-memory volume: what the demo page talks to. The
/// listing engine and the write engine share the volume and the panes, so a
/// write reaches the listings through `directory-diff`.
#[derive(Clone)]
pub struct Demo {
    listings: Listings,
    events: Arc<DemoEvents>,
    /// The page's measured fonts, in memory only: the desktop's disk cache has no
    /// counterpart, so a reload measures again.
    fonts: Arc<FontMetricsStore>,
    operations: DemoOperations,
    /// Folder sizes off the volume, the drive index's stand-in.
    sizes: Arc<FolderSizes>,
}

impl Demo {
    /// The engines over `tree`, reporting every event to `sink`.
    pub fn new(tree: DemoTree, sink: Arc<dyn EventSink>) -> Result<Self, TreeError> {
        let volume_id = tree.volume_id.clone();
        // Late-bound: the volume patches the panes, which list the volume.
        let panes = Arc::new(BackendListings::default());
        let store = Arc::new(
            tree.into_in_memory()?
                .with_bandwidth(DEVICE_BYTES_PER_SEC)
                .with_yielding(DEVICE_SLICE)
                .with_capacity(DEVICE_CAPACITY_BYTES)
                .with_pane_patches(volume_id.clone(), Arc::clone(&panes) as _),
        );
        // The drive index's stand-in, for the panes, the dialogs, and the status bar.
        let sizes = Arc::new(FolderSizes::new(Arc::clone(&store)));
        let volume: Arc<dyn Volume> = store;
        let events = Arc::new(DemoEvents::new(Arc::clone(&sink)));
        let volumes = Arc::new(OneVolume {
            volume_id: volume_id.clone(),
            volume: Arc::clone(&volume),
        });
        let listings = Listings::builder(volumes, Arc::clone(&events) as _)
            .index(Arc::clone(&sizes) as _)
            .build();
        panes.attach(&listings);
        panes.attach_sizes(Arc::clone(&sizes), Arc::clone(&sink));
        listings.start_orphan_reaper();
        let volume_host = VolumeHost::builder().listings(panes).build();
        let operations = DemoOperations::new(volume_id, volume, volume_host, &listings, Arc::clone(&sizes) as _, sink);
        Ok(Self {
            listings,
            events,
            fonts: Arc::new(FontMetricsStore::new()),
            operations,
            sizes,
        })
    }

    /// The write engine's host, for a caller driving the engine directly.
    #[cfg(test)]
    pub(crate) fn operations(&self) -> &DemoOperations {
        &self.operations
    }

    /// Whether [`Demo::invoke`] answers `command`.
    pub fn handles(command: &str) -> bool {
        COMMANDS.contains(&command) || DemoOperations::handles(command)
    }

    /// Every command [`Demo::invoke`] answers, by its Tauri name.
    pub fn commands() -> impl Iterator<Item = &'static str> {
        COMMANDS.iter().copied().chain(DemoOperations::commands())
    }

    /// Runs `command` with Tauri-style `args` (camelCase keys).
    pub async fn invoke(&self, command: &str, args: Value) -> Result<Value, Value> {
        if DemoOperations::handles(command) {
            return self.operations.invoke(command, args).await;
        }
        let l = &self.listings;
        match command {
            "list_directory_start_streaming" => {
                let a: StartArgs = parse(command, args)?;
                l.start(ListingRequest {
                    id: ListingId::new(a.listing_id.clone()),
                    volume_id: a.volume_id,
                    path: PathBuf::from(a.path),
                    include_hidden: a.include_hidden,
                    sort_by: a.sort_by,
                    sort_order: a.sort_order,
                    dir_sort_mode: a.directory_sort_mode.unwrap_or_default(),
                });
                ok(StartResult {
                    listing_id: a.listing_id,
                    status: Status::Loading,
                })
            }
            "cancel_listing" => {
                let a: IdArgs = parse(command, args)?;
                l.cancel(&ListingId::new(a.listing_id));
                Ok(Value::Null)
            }
            "resort_listing" => {
                let a: ResortArgs = parse(command, args)?;
                answer(l.resort(
                    &ListingId::new(a.listing_id),
                    (a.sort_by, a.sort_order, a.directory_sort_mode.unwrap_or_default()),
                    a.include_hidden,
                    PaneMarks {
                        cursor_filename: a.cursor_filename.as_deref(),
                        selected: a.selected_indices.as_deref(),
                        all_selected: a.all_selected.unwrap_or(false),
                    },
                    a.expected_sequence,
                ))
            }
            "get_file_range" => {
                let a: RangeArgs = parse(command, args)?;
                let span = RowSpan {
                    start: a.start,
                    count: a.count,
                };
                answer(l.file_range(&ListingId::new(a.listing_id), span, a.include_hidden))
            }
            "compare_directories" => {
                let a: CompareArgs = parse(command, args)?;
                answer(
                    l.compare_directories(
                        (&ListingId::new(a.left_listing_id), a.left_include_hidden),
                        (&ListingId::new(a.right_listing_id), a.right_include_hidden),
                        a.mode,
                    )
                    .map_err(CompareDirectoriesError::from),
                )
            }
            "find_file_index" => {
                let a: NameArgs = parse(command, args)?;
                answer(l.find_file_index(&ListingId::new(a.listing_id), &a.name, a.include_hidden))
            }
            "find_file_indices" => {
                let a: NamesArgs = parse(command, args)?;
                answer(l.find_file_indices(&ListingId::new(a.listing_id), &a.names, a.include_hidden))
            }
            "find_first_fuzzy_match" => {
                let a: QueryArgs = parse(command, args)?;
                answer(l.fuzzy_find_first_match(&ListingId::new(a.listing_id), &a.query, a.include_hidden))
            }
            "get_file_at" => {
                let a: IndexArgs = parse(command, args)?;
                answer(l.file_at(&ListingId::new(a.listing_id), a.index, a.include_hidden))
            }
            "get_file_beside" => {
                let a: BesideArgs = parse(command, args)?;
                answer(l.file_beside(&ListingId::new(a.listing_id), &a.name, a.side, a.include_hidden))
            }
            "list_directory_end" => {
                let a: IdArgs = parse(command, args)?;
                l.end(&ListingId::new(a.listing_id));
                Ok(Value::Null)
            }
            "keep_listings_alive" => {
                let a: IdsArgs = parse(command, args)?;
                let ids: Vec<ListingId> = a.listing_ids.into_iter().map(ListingId::new).collect();
                let gone: Vec<String> = l.keep_alive(&ids).iter().map(|id| id.as_str().to_string()).collect();
                ok(gone)
            }
            "set_listing_include_hidden" => {
                let a: HiddenArgs = parse(command, args)?;
                answer(l.set_include_hidden(
                    &ListingId::new(a.listing_id),
                    a.include_hidden,
                    a.expected_sequence,
                    PaneMarks {
                        cursor_filename: a.cursor_filename.as_deref(),
                        selected: a.selected_indices.as_deref(),
                        all_selected: a.all_selected.unwrap_or(false),
                    },
                ))
            }
            "get_selection_snapshot" => {
                let a: SelectionArgs = parse(command, args)?;
                answer(l.selection_snapshot(
                    &ListingId::new(a.listing_id),
                    a.include_hidden,
                    &a.selected_indices,
                    a.expected_sequence,
                ))
            }
            "get_selection_entries" => {
                let a: SelectionArgs = parse(command, args)?;
                answer(l.selection_entries(
                    &ListingId::new(a.listing_id),
                    a.include_hidden,
                    &a.selected_indices,
                    a.expected_sequence,
                ))
            }
            "set_listing_name_filter" => {
                let a: FilterArgs = parse(command, args)?;
                answer(l.set_name_filter_guarded(
                    &ListingId::new(a.listing_id),
                    a.pattern.as_deref(),
                    a.include_hidden,
                    PaneMarks {
                        cursor_filename: a.cursor_filename.as_deref(),
                        selected: Some(&a.selected_indices),
                        all_selected: false,
                    },
                    if a.refuse_empty {
                        OnNoMatch::Refuse
                    } else {
                        OnNoMatch::Apply
                    },
                    a.expected_sequence,
                ))
            }
            "refresh_listing" => {
                let a: RefreshArgs = parse(command, args)?;
                // `force` only skips the desktop's watcher-backed short-circuit,
                // which never applies to a local-kind volume: both re-read.
                let _ = a.force;
                ok(self.refresh(a.listing_id).await)
            }
            "get_listing_stats" => {
                let a: StatsArgs = parse(command, args)?;
                let selected = a.selected_indices.as_deref();
                answer(l.stats(&ListingId::new(a.listing_id), a.include_hidden, selected))
            }
            "refresh_listing_index_sizes" => {
                let a: IdArgs = parse(command, args)?;
                l.refresh_index_sizes(&ListingId::new(a.listing_id));
                Ok(Value::Null)
            }
            "get_dir_stats" => {
                let a: PathArgs = parse(command, args)?;
                ok(self.sizes.dir_stats(&[a.path]).pop().flatten())
            }
            "get_dir_stats_batch" => {
                let a: PathsArgs = parse(command, args)?;
                ok(self.sizes.dir_stats(&a.paths))
            }
            "store_font_metrics" => {
                let a: FontWidthsArgs = parse(command, args)?;
                answer(widths_from_parallel(a.code_points, a.widths).map(|widths| {
                    let metrics = FontMetrics::new(a.font_id.clone(), widths);
                    self.fonts.insert(a.font_id, metrics);
                }))
            }
            "extend_font_metrics" => {
                let a: FontWidthsArgs = parse(command, args)?;
                answer(widths_from_parallel(a.code_points, a.widths).and_then(|widths| {
                    // The desktop's not-stored answer, which its extend reports as a
                    // persist that didn't land.
                    self.fonts
                        .extend_with(&a.font_id, widths, |_| ())
                        .map_err(|detail| FontMetricsError::CouldntPersist { detail })
                }))
            }
            "has_font_metrics" => {
                let a: FontIdArgs = parse(command, args)?;
                ok(self.fonts.contains(&a.font_id))
            }
            "get_brief_column_text_widths" => {
                let query: BriefColumnsQuery = parse(command, args)?;
                answer(compute_brief_column_text_widths(l, &self.fonts, &query).map_err(BriefColumnsIpcError::from))
            }
            _ => Err(Value::String(format!("the browser demo doesn't answer `{command}`"))),
        }
    }

    /// `refresh_listing`, awaited in place. The desktop spawns the re-read and
    /// waits at most two seconds, so a hung mount can't hold the caller; an
    /// in-memory read can't hang, so there's nothing to time out.
    async fn refresh(&self, listing_id: String) -> TimedOut<()> {
        let id = ListingId::new(listing_id.clone());
        if let RefreshOutcome::DirectoryGone(path) = self.listings.refresh(&id).await {
            let path = path.to_string_lossy().into_owned();
            let event = ListingPathEvent {
                listing_id: &listing_id,
                path,
            };
            self.events.send("directory-deleted", event);
        }
        TimedOut {
            data: (),
            timed_out: false,
        }
    }
}

pub(crate) fn parse<T: DeserializeOwned>(command: &str, args: Value) -> Result<T, Value> {
    serde_json::from_value(args).map_err(|e| Value::String(format!("invalid args for command `{command}`: {e}")))
}

pub(crate) fn ok(answer: impl Serialize) -> Result<Value, Value> {
    serde_json::to_value(answer).map_err(|e| Value::String(e.to_string()))
}

pub(crate) fn answer<T: Serialize, E: Serialize>(result: Result<T, E>) -> Result<Value, Value> {
    match result {
        Ok(answer) => ok(answer),
        Err(error) => Err(ok(error)?),
    }
}

/// `ListingStatus` and `StreamingListingStartResult` in the app: the answer is
/// always "loading", and the rest arrives as events.
#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
enum Status {
    Loading,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartResult {
    listing_id: String,
    status: Status,
}

/// The one volume the demo serves, with no routes.
struct OneVolume {
    volume_id: String,
    volume: Arc<dyn Volume>,
}

impl ListingVolumes for OneVolume {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        (volume_id == self.volume_id).then(|| Arc::clone(&self.volume))
    }

    fn resolve<'a>(
        &'a self,
        volume_id: &'a str,
        path: &'a std::path::Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = ResolvedVolume> + Send + 'a>> {
        Box::pin(async move { self.resolve_local_only(volume_id, path) })
    }

    fn resolve_local_only(&self, volume_id: &str, path: &std::path::Path) -> ResolvedVolume {
        ResolvedVolume::passthrough(self.get(volume_id), path)
    }

    fn unregistered<'a>(
        &'a self,
        _volume_id: &'a str,
        _path: &'a std::path::Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Unregistered> + Send + 'a>> {
        Box::pin(async { Unregistered::Gone })
    }
}

// The argument shapes, one per command signature in the desktop's `listing.rs`.

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartArgs {
    volume_id: String,
    path: String,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: Option<DirectorySortMode>,
    listing_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdArgs {
    listing_id: String,
}

#[derive(Deserialize)]
struct PathArgs {
    path: String,
}

#[derive(Deserialize)]
struct PathsArgs {
    paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdsArgs {
    listing_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResortArgs {
    listing_id: String,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: Option<DirectorySortMode>,
    cursor_filename: Option<String>,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
    all_selected: Option<bool>,
    expected_sequence: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeArgs {
    listing_id: String,
    start: usize,
    count: usize,
    include_hidden: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompareArgs {
    left_listing_id: String,
    left_include_hidden: bool,
    right_listing_id: String,
    right_include_hidden: bool,
    mode: CompareDirectoriesMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NameArgs {
    listing_id: String,
    name: String,
    include_hidden: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NamesArgs {
    listing_id: String,
    names: Vec<String>,
    include_hidden: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryArgs {
    listing_id: String,
    query: String,
    include_hidden: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexArgs {
    listing_id: String,
    index: usize,
    include_hidden: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BesideArgs {
    listing_id: String,
    name: String,
    side: RowBeside,
    include_hidden: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HiddenArgs {
    listing_id: String,
    include_hidden: bool,
    expected_sequence: Option<u64>,
    cursor_filename: Option<String>,
    selected_indices: Option<Vec<usize>>,
    all_selected: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionArgs {
    listing_id: String,
    include_hidden: bool,
    selected_indices: Vec<usize>,
    expected_sequence: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilterArgs {
    listing_id: String,
    pattern: Option<String>,
    include_hidden: bool,
    cursor_filename: Option<String>,
    selected_indices: Vec<usize>,
    refuse_empty: bool,
    expected_sequence: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefreshArgs {
    listing_id: String,
    force: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FontWidthsArgs {
    font_id: String,
    code_points: Vec<u32>,
    widths: Vec<f32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FontIdArgs {
    font_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatsArgs {
    listing_id: String,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
}
