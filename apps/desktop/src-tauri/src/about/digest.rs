//! A folder's digest ("What's inside") and an app's data left in the Library,
//! from the drive index and the search arena only.
//!
//! ❌ No walk, `stat`, or listing of the disk from here: a 2 TB folder answers as
//! fast as an empty one, a container is never opened, and a part the index hasn't
//! listed says so. `cmdr_about::digest` owns every rule; this reads the rows.

use std::cell::OnceCell;
use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use cmdr_about::digest::{
    APP_DATA_PLACES, AppData, ByteShare, FolderClass, FolderDigest, LibraryFolder, SHOWN_CHILDREN, SizedChild,
    UNCHANGED_FOR_SECS, app_data_place,
};
use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::payload::{RecursiveSize, SizeCoverage};
use cmdr_about::text::ItemText;
use cmdr_index::IndexStatusResponse;
use cmdr_index::store::{DirStats, EntryRow};

use super::index::{IndexPlace, recursive_size_fact, recursive_size_of};
use crate::file_system::framework_pool::note_activity;
use crate::index_host::index;
use crate::search::{PatternType, SearchQuery, SearchResult, SearchSort, run_if_loaded};

/// How many of a folder's untouched files the share counts, largest first. Past
/// it the share says "at least": the largest files carry most of the bytes, and
/// materializing every row of a huge folder isn't worth a card's millisecond.
const UNCHANGED_ROWS: u32 = 1_000;

/// `parent` joined with a child's `name`, as the index and the panes spell paths.
fn child_path(parent: &str, name: &str) -> String {
    format!("{}/{name}", parent.trim_end_matches('/'))
}

/// What the folder at `place` holds, from the index.
///
/// Always `Known`: the folder's class comes from its name, so `node_modules` is
/// installed packages even where the index hasn't listed it, and only the parts
/// that need the index say they can't tell.
pub(crate) fn folder_digest_fact(place: &IndexPlace, now_secs: u64) -> Fact<FolderDigest> {
    let name = Path::new(&place.path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let class = FolderClass::of(&name, &beside(&place.index_id, &place.path));
    let rows = match index().list_children(&place.index_id, &place.path) {
        Ok(Some(rows)) => Ok(rows),
        Ok(None) => Err(UnknownWhy::NotIndexed),
        Err(error) => {
            log::warn!(target: "about", "couldn't list a folder's children from the index: {error}");
            Err(UnknownWhy::Unparseable)
        }
    };
    let (children, largest, unchanged) = match rows {
        Ok(rows) => {
            // The two searches scan the whole arena (hundreds of ms on a big one), so
            // they run beside the listing, on jobs of their own, under one budget.
            let ends = Instant::now() + SEARCH_BUDGET;
            let largest = on_its_own_job(ends, "largest", {
                let place = place.clone();
                move || largest_files(&place)
            });
            let unchanged = if rows.is_empty() {
                // An empty folder has nothing to be untouched; the card says it's empty.
                None
            } else {
                let place = place.clone();
                Some(on_its_own_job(ends, "untouched", move || {
                    unchanged_share(&place, now_secs)
                }))
            };
            let children = sized_children(&place.index_id, &place.path, &rows, &walked_roots(&place.index_id));
            let unchanged = unchanged.map_or_else(|| Fact::unknown(UnknownWhy::NoRecord), Waiting::answer);
            (Fact::Known(children), largest.answer(), unchanged)
        }
        // Nothing listed it, so the arena can't hold it either.
        Err(why) => (Fact::unknown(why), Fact::unknown(why), Fact::unknown(why)),
    };
    Fact::Known(FolderDigest::of(children, largest, unchanged, class))
}

/// How long the digest waits for its two searches, inside the row's own budget
/// (`jobs::ROW_BUDGET`, 1 s): past it the listing stands and the search's part says
/// it timed out. Each scans the whole arena whatever the folder's size: 400–860 ms
/// for the largest files on an 8,028,071-row arena, 50–100 ms for the untouched ones
/// (macOS 27.0.1, release build, `search::bench_card` on a real `index-root.db`,
/// 2026-11-13), so a big drive's card fills them in after its first frame.
const SEARCH_BUDGET: Duration = Duration::from_millis(800);

/// A search running on its own pool job, and when the digest stops waiting for it.
struct Waiting<T> {
    answer: mpsc::Receiver<Fact<T>>,
    ends: Instant,
}

impl<T> Waiting<T> {
    /// The search's answer, or `TimedOut` once its budget ends.
    fn answer(self) -> Fact<T> {
        match self
            .answer
            .recv_timeout(self.ends.saturating_duration_since(Instant::now()))
        {
            Ok(fact) => fact,
            Err(mpsc::RecvTimeoutError::Timeout) => Fact::unknown(UnknownWhy::TimedOut),
            // The search panicked: no answer is coming.
            Err(mpsc::RecvTimeoutError::Disconnected) => Fact::unknown(UnknownWhy::Unparseable),
        }
    }
}

fn on_its_own_job<T: Send + 'static>(
    ends: Instant,
    which: &'static str,
    search: impl FnOnce() -> Fact<T> + Send + 'static,
) -> Waiting<T> {
    let (answer, waiting) = mpsc::sync_channel(1);
    super::pool::submit(Box::new(move || {
        note_activity("search the arena", which);
        let _ = answer.send(search());
    }));
    Waiting { answer: waiting, ends }
}

/// The roots a walk is rewriting right now, for each size's "still moving".
fn walked_roots(index_id: &str) -> Vec<String> {
    index()
        .status(index_id)
        .map(|status| status.walked_roots)
        .unwrap_or_default()
}

/// Each child with its size: a file's own, a folder's whole subtree from its
/// stats (or `NotIndexed` when it has none yet).
fn sized_children(index_id: &str, parent: &str, rows: &[EntryRow], walked: &[String]) -> Vec<SizedChild> {
    let folder_paths: Vec<String> = rows
        .iter()
        .filter(|row| row.is_directory)
        .map(|row| child_path(parent, &row.name))
        .collect();
    let mut folder_stats = stats_or_why(
        index().dir_stats_batch(index_id, &folder_paths).map_err(|_| ()),
        folder_paths.len(),
    )
    .into_iter();
    rows.iter()
        .map(|row| {
            let path = child_path(parent, &row.name);
            let size = if row.is_directory {
                folder_size(folder_stats.next().unwrap_or(Ok(None)), &path, walked)
            } else {
                file_size(row.logical_size, row.physical_size)
            };
            SizedChild {
                name: ItemText::new(&row.name),
                path,
                is_folder: row.is_directory,
                size,
            }
        })
        .collect()
}

/// Each of `count` folders' stats from one batch read, or why there are none: a batch
/// the index couldn't read is `Unparseable` for every folder, ❌ never `NotIndexed`
/// (which says the index hasn't reached them).
pub(crate) fn stats_or_why(
    batch: Result<Vec<Option<DirStats>>, ()>,
    count: usize,
) -> Vec<Result<Option<DirStats>, UnknownWhy>> {
    match batch {
        Ok(stats) => stats.into_iter().map(Ok).collect(),
        Err(()) => {
            // The kind of failure only: the error's text can carry a path.
            log::warn!(target: "about", "couldn't read folder sizes from the index for a card");
            (0..count).map(|_| Err(UnknownWhy::Unparseable)).collect()
        }
    }
}

pub(crate) fn folder_size(
    stats: Result<Option<DirStats>, UnknownWhy>,
    path: &str,
    walked: &[String],
) -> Fact<RecursiveSize> {
    match stats {
        Ok(Some(stats)) => Fact::Known(recursive_size_of(
            &stats,
            IndexStatusResponse::walk_affects(walked, path),
        )),
        Ok(None) => Fact::unknown(UnknownWhy::NotIndexed),
        Err(why) => Fact::unknown(why),
    }
}

/// A file's own size as a one-file total, so a file and a folder sort and show
/// alike.
fn file_size(logical: Option<u64>, physical: Option<u64>) -> Fact<RecursiveSize> {
    match logical {
        Some(logical) => Fact::Known(RecursiveSize {
            logical,
            physical: physical.unwrap_or(logical),
            files: 1,
            folders: 0,
            coverage: SizeCoverage::Exact,
            updating: false,
            has_symlinks: false,
        }),
        // The index holds the row without a size: it couldn't read one.
        None => Fact::unknown(UnknownWhy::NotIndexed),
    }
}

/// Whether a sibling named `marker` sits beside the folder, from the index's
/// listing of its parent (read once, and only for a name that asks).
fn beside<'a>(index_id: &'a str, path: &'a str) -> impl Fn(&str) -> bool + 'a {
    let siblings: OnceCell<HashSet<String>> = OnceCell::new();
    move |marker: &str| {
        let names = siblings.get_or_init(|| {
            let Some(parent) = Path::new(path).parent() else {
                return HashSet::new();
            };
            index()
                .list_children(index_id, &parent.to_string_lossy())
                .ok()
                .flatten()
                .map(|rows| rows.into_iter().map(|row| row.name.to_ascii_lowercase()).collect())
                .unwrap_or_default()
        });
        names.contains(&marker.to_ascii_lowercase())
    }
}

/// A size-sorted search for the folder's files, from the arena only.
fn files_under(place: &IndexPlace, modified_before: Option<u64>, limit: u32) -> Option<Result<SearchResult, String>> {
    let query = SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before,
        is_directory: Some(false),
        include_paths: Some(vec![place.path.clone()]),
        exclude_dir_names: None,
        include_path_ids: None,
        limit,
        case_sensitive: None,
        sort_by: Some(SearchSort::Size),
        // Everything under the folder counts: inside `node_modules` the system-folder
        // excludes would hide the very files the card is about.
        exclude_system_dirs: Some(false),
        count_only: false,
    };
    run_if_loaded(&query, &place.index_id)
}

/// What a search answered, or why the card can't say: no arena in memory or the
/// folder not in it yet is `NotIndexed`, ❌ never "no files".
fn searched(answer: Option<Result<SearchResult, String>>) -> Result<SearchResult, UnknownWhy> {
    match answer {
        None => Err(UnknownWhy::NotIndexed),
        Some(Err(error)) => {
            log::warn!(target: "about", "couldn't search a folder's files: {error}");
            Err(UnknownWhy::Unparseable)
        }
        Some(Ok(result)) if !result.unresolved_scopes.is_empty() => Err(UnknownWhy::NotIndexed),
        Some(Ok(result)) => Ok(result),
    }
}

/// The largest files anywhere under the folder.
fn largest_files(place: &IndexPlace) -> Fact<Vec<SizedChild>> {
    let limit = u32::try_from(SHOWN_CHILDREN).unwrap_or(u32::MAX);
    match searched(files_under(place, None, limit)) {
        Ok(result) => Fact::Known(
            result
                .entries
                .into_iter()
                .map(|entry| SizedChild {
                    name: ItemText::new(&entry.name),
                    path: entry.path,
                    is_folder: false,
                    size: file_size(entry.size, None),
                })
                .collect(),
        ),
        Err(why) => Fact::unknown(why),
    }
}

/// The share of the folder's bytes in files untouched for a year.
fn unchanged_share(place: &IndexPlace, now_secs: u64) -> Fact<ByteShare> {
    let year = u64::try_from(UNCHANGED_FOR_SECS).unwrap_or(u64::MAX);
    let Some(cutoff) = now_secs.checked_sub(year) else {
        return Fact::unknown(UnknownWhy::NotIndexed);
    };
    match searched(files_under(place, Some(cutoff), UNCHANGED_ROWS)) {
        Ok(result) => {
            let bytes = result.entries.iter().filter_map(|entry| entry.size).sum();
            let every_file_counted = result.hidden_by_excludes == 0
                && usize::try_from(result.total_count).is_ok_and(|total| total <= result.entries.len());
            ByteShare::unchanged(bytes, every_file_counted, &recursive_size_fact(place))
        }
        Err(why) => Fact::unknown(why),
    }
}

/// The Library folders named after the app with `bundle_id`, with their sizes,
/// from the index `index_id` that holds `home`. A place the index hasn't listed
/// is named as not looked in, ❌ never read from the disk: a container is never
/// opened.
pub(crate) fn app_data_fact(bundle_id: Option<&str>, home: &str, index_id: &str) -> Fact<AppData> {
    let Some(bundle_id) = bundle_id else {
        // An app that names no id has no folders to find by it.
        return Fact::unknown(UnknownWhy::NoRecord);
    };
    let walked = walked_roots(index_id);
    let mut found: Vec<(String, String, cmdr_about::owner::OwnerPlace)> = Vec::new();
    let mut not_looked_in = Vec::new();
    for (folder, place) in APP_DATA_PLACES {
        let library = format!("{}/Library/{folder}", home.trim_end_matches('/'));
        match index().list_children(index_id, &library) {
            Ok(Some(rows)) => found.extend(rows.into_iter().filter(|row| row.is_directory).filter_map(|row| {
                let path = child_path(&library, &row.name);
                let place = app_data_place(&path, home, bundle_id)?;
                Some((row.name, path, place))
            })),
            Ok(None) => not_looked_in.push(place),
            Err(error) => {
                log::warn!(target: "about", "couldn't list a Library folder from the index: {error}");
                not_looked_in.push(place);
            }
        }
    }
    let paths: Vec<String> = found.iter().map(|(_, path, _)| path.clone()).collect();
    let mut stats = stats_or_why(index().dir_stats_batch(index_id, &paths).map_err(|_| ()), paths.len()).into_iter();
    let folders = found
        .into_iter()
        .map(|(name, path, place)| {
            let size = folder_size(stats.next().unwrap_or(Ok(None)), &path, &walked);
            LibraryFolder {
                name: ItemText::new(&name),
                path,
                place,
                size,
            }
        })
        .collect();
    AppData::of(folders, not_looked_in)
}

#[cfg(test)]
#[path = "digest_tests.rs"]
mod tests;
