//! Index size updates, delivered only to the listings they touch, and only when a row would change.
//!
//! The drive index reports batches of directories whose recursive sizes changed
//! (`IndexEvent::DirsUpdated`, about once a second on a busy disk). This module keeps the set of open
//! listings (a [`ListingLifecycle`] observer), works out which rows of which listings a batch touches
//! ([`touched()`]), reads those rows' fresh stats from the index, and sends
//! `listing-index-sizes-changed` carrying only the rows whose shown values moved
//! ([`refresh`](mod@refresh)). It also writes them into the listing cache, so status-bar totals
//! and MCP reads see them.
//!
//! Three things keep an idle pane quiet:
//! - A batch that touched nothing a listing shows is dropped (a write in `~/Library` and a pane on
//!   `~/Downloads`).
//! - A reading that matches what a row already shows sends nothing (a cache file written and removed
//!   inside one flush).
//! - At most one batch refresh per listing per [`schedule::COOLDOWN`], with a trailing one so the
//!   last change always lands; and none at all while the main window is hidden
//!   (`main_window_visibility`), which the first refresh after it shows catches up in one go.
//!
//! The "size updating" hourglass shows only for an update that has run two seconds, which the index
//! decides (`DirStats::recursive_size_pending`). It flips with no batch to announce it, so a reading
//! that says when it will flip books a re-read of that row for that moment ([`schedule`]).
//!
//! The work runs on its own task, fed through a channel: the batch arrives on the index writer's
//! thread, which must not wait on anything here.

pub(crate) mod count;
#[cfg(test)]
mod count_test;
mod refresh;
mod schedule;
mod touched;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_index::store::DirStats;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::file_system::listing::cached_listing::LISTING_CACHE;
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::volume::Volume;
use crate::ignore_poison::IgnorePoison;
use crate::index_host::index;
use crate::listing_lifecycle::{ListingLifecycle, register_listing_lifecycle};

use refresh::RowSizes;
use schedule::Schedule;
pub(crate) use touched::{Touched, touched};

/// One folder row's fresh index reading.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderSizes {
    /// The row's path, as the listing holds it.
    pub path: String,
    /// The reading, or `None` when the index doesn't cover the folder (sizes stay, hourglass clears).
    pub stats: Option<DirStats>,
}

/// A listing's folder sizes moved in the index.
///
/// Carries only the rows whose shown values moved, already written into the listing cache, so the
/// pane applies them without asking again. `full` is the whole-volume case (a scan finishing), where
/// every row moved and the pane re-reads its window instead.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ListingIndexSizesChanged {
    /// The listing whose rows moved.
    pub listing_id: String,
    /// Every row moved: re-read the window's sizes rather than apply `folders`.
    pub full: bool,
    /// The folder rows whose shown values moved.
    pub folders: Vec<FolderSizes>,
    /// The listing's own folder moved (the `..` row), to `current_dir`.
    pub current_dir_changed: bool,
    /// The listing's own folder reading, when `current_dir_changed`.
    pub current_dir: Option<DirStats>,
}

/// One open listing, as the index spells its folder.
#[derive(Debug, Clone)]
struct OpenListing {
    volume_id: String,
    /// The folder in the index's path space: the volume's listing spelling, firmlink-normalized.
    index_dir: String,
}

/// Every open listing, by id. Kept by [`IndexSizeListings`].
static OPEN: LazyLock<Mutex<HashMap<String, OpenListing>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Where [`dirs_updated`] hands batches to the worker. Unset until [`start`] runs.
static BATCHES: OnceLock<mpsc::UnboundedSender<Vec<String>>> = OnceLock::new();

/// Keeps [`OPEN`] in step with the listing cache.
struct IndexSizeListings;

impl ListingLifecycle for IndexSizeListings {
    fn id(&self) -> &'static str {
        "index-sizes"
    }

    fn listing_opened(&self, listing_id: &str, _volume: &dyn Volume, _path: &Path) {
        // The cache already holds the listing (the open path inserts before it notifies), and its
        // record carries both the volume id and the volume's one spelling of the folder.
        let Some(listing) = LISTING_CACHE
            .read_ignore_poison()
            .get(listing_id)
            .map(|listing| OpenListing {
                volume_id: listing.volume_id.clone(),
                index_dir: cmdr_fs::firmlinks::normalize_path(&listing.path.as_path().to_string_lossy()),
            })
        else {
            return;
        };
        OPEN.lock_ignore_poison().insert(listing_id.to_string(), listing);
    }

    fn listing_closed(&self, listing_id: &str) {
        OPEN.lock_ignore_poison().remove(listing_id);
        // A folder-size count for a listing nobody shows any more is wasted I/O.
        count::cancel(listing_id);
    }
}

/// Registers the open-listing observer and starts the worker. Call once from setup.
pub(crate) fn start(app: AppHandle) {
    register_listing_lifecycle(Arc::new(IndexSizeListings));
    let (tx, rx) = mpsc::unbounded_channel();
    if BATCHES.set(tx).is_err() {
        return;
    }
    tauri::async_runtime::spawn(run(app, rx));
}

/// Hands one index batch to the worker. Called from the index event sink; never blocks.
pub(crate) fn dirs_updated(paths: Vec<String>) {
    if let Some(tx) = BATCHES.get() {
        // A closed channel means the app is shutting down; nothing is left to tell.
        let _ = tx.send(paths);
    }
}

/// What the worker remembers about one listing between refreshes.
#[derive(Default)]
struct ListingState {
    /// The work waiting for this listing, and when it's due.
    schedule: Schedule,
    /// Rows whose hourglass was last sent lit (the flag isn't on the cached entry).
    lit: HashSet<String>,
    /// The listing's own folder reading as last sent; `None` until the first send.
    current_dir: Option<Option<DirStats>>,
}

async fn run(app: AppHandle, mut batches: mpsc::UnboundedReceiver<Vec<String>>) {
    let mut states: HashMap<String, ListingState> = HashMap::new();
    let mut visibility = crate::main_window_visibility::subscribe();
    loop {
        // Hidden: no deadline, or a held refresh that's already due would spin the loop.
        let next_due = crate::main_window_visibility::is_visible()
            .then(|| next_due(&states))
            .flatten();
        tokio::select! {
            batch = batches.recv() => {
                let Some(paths) = batch else { return };
                for (listing_id, touched) in touched_listings(&paths) {
                    states.entry(listing_id).or_default().schedule.add_batch(touched);
                }
            }
            changed = visibility.changed() => {
                // The sender is a static, so this can't close; if it did, stop listening for it.
                if changed.is_err() {
                    visibility = crate::main_window_visibility::subscribe();
                }
            }
            () = sleep_until(next_due) => {}
        }
        // Hidden: hold everything; the first pass after the window shows catches up.
        if !crate::main_window_visibility::is_visible() {
            continue;
        }
        let open: HashSet<String> = OPEN.lock_ignore_poison().keys().cloned().collect();
        states.retain(|listing_id, _| open.contains(listing_id));
        let now = Instant::now();
        for (listing_id, state) in &mut states {
            let due = state.schedule.take_due(now);
            // A batch and a recheck due together are one read and at most one event.
            let touched = match (due.batch, due.recheck) {
                (Some(mut batch), Some(recheck)) => {
                    batch.merge(recheck);
                    batch
                }
                (Some(rows), None) | (None, Some(rows)) => rows,
                (None, None) => continue,
            };
            if let Some(event) = refresh_listing(listing_id, touched, state).await {
                let _ = event.emit(&app);
            }
        }
    }
}

/// When the next held refresh or recheck is due, if any listing has one waiting.
fn next_due(states: &HashMap<String, ListingState>) -> Option<Instant> {
    states.values().filter_map(|state| state.schedule.next_due()).min()
}

async fn sleep_until(due: Option<Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due).await,
        None => std::future::pending().await,
    }
}

/// Every open listing `paths` touched, with what it touched.
fn touched_listings(paths: &[String]) -> HashMap<String, Touched> {
    let open = OPEN.lock_ignore_poison().clone();
    open.into_iter()
        .filter_map(|(id, listing)| touched(paths, &listing.volume_id, &listing.index_dir).map(|t| (id, t)))
        .collect()
}

/// Re-reads what `touched` names from the index, writes what moved into the listing cache, and
/// returns the event to send, or `None` when nothing a row shows moved.
async fn refresh_listing(
    listing_id: &str,
    touched: Touched,
    state: &mut ListingState,
) -> Option<ListingIndexSizesChanged> {
    let listing_id = listing_id.to_string();
    let lit = std::mem::take(&mut state.lit);
    let last_current = state.current_dir.clone();
    // The index reads are indexed SQLite queries: the blocking pool, not this async worker.
    let outcome = tokio::task::spawn_blocking(move || refresh_blocking(&listing_id, &touched, lit, last_current))
        .await
        .ok()?;
    state.lit = outcome.lit;
    state.current_dir = outcome.current_dir;
    if let Some((after, rows)) = outcome.recheck {
        state.schedule.recheck(Instant::now(), after, rows);
    }
    outcome.event
}

struct RefreshOutcome {
    event: Option<ListingIndexSizesChanged>,
    lit: HashSet<String>,
    current_dir: Option<Option<DirStats>>,
    /// Rows whose hourglass flips on its own, and the soonest flip.
    recheck: Option<(Duration, Touched)>,
}

fn refresh_blocking(
    listing_id: &str,
    touched: &Touched,
    mut lit: HashSet<String>,
    last_current: Option<Option<DirStats>>,
) -> RefreshOutcome {
    let unchanged = |lit, current_dir| RefreshOutcome {
        event: None,
        lit,
        current_dir,
        recheck: None,
    };

    // The rows to re-read, as the cache holds them now.
    let Some((dir_path, rows)) = rows_to_read(listing_id, touched) else {
        return unchanged(lit, last_current);
    };
    let mut paths: Vec<String> = rows.iter().map(|(path, _)| path.clone()).collect();
    paths.push(dir_path);
    let Ok(mut stats) = index().dir_stats_batch(&paths) else {
        return unchanged(lit, last_current);
    };
    paths.pop();
    let current = stats.pop().flatten();
    let recheck = recheck_of(&rows, &stats, current.as_ref());

    if matches!(touched, Touched::Whole) {
        // Every row moved: a full re-enrich of the cache, and the pane re-reads its window.
        let _ = crate::file_system::listing::operations::refresh_listing_index_sizes(listing_id);
        lit.clear();
        return RefreshOutcome {
            event: Some(ListingIndexSizesChanged {
                listing_id: listing_id.to_string(),
                full: true,
                folders: Vec::new(),
                current_dir_changed: true,
                current_dir: current.clone(),
            }),
            lit,
            current_dir: Some(current),
            recheck,
        };
    }

    // Keep only the rows whose shown values move, and write those into the cache.
    let mut moved: Vec<(usize, RowSizes)> = Vec::new();
    for (position, ((path, before), reading)) in rows.iter().zip(&stats).enumerate() {
        let before = RowSizes::of(before, lit.contains(path));
        let after = before.after(reading.as_ref());
        if after != before {
            moved.push((position, after));
        }
    }
    if !moved.is_empty() {
        let moved_paths: Vec<String> = moved.iter().map(|(position, _)| paths[*position].clone()).collect();
        if let Some(listing) = LISTING_CACHE.write_ignore_poison().get_mut(listing_id) {
            listing.update_index_sizes_by_path(&moved_paths, |i, entry| moved[i].1.apply_to(entry));
        }
    }
    for (position, after) in &moved {
        if after.pending() {
            lit.insert(paths[*position].clone());
        } else {
            lit.remove(&paths[*position]);
        }
    }

    // `DirStats` has no `PartialEq`; compared as the `..` row would show it.
    let shown = |reading: &Option<DirStats>| RowSizes::default().after(reading.as_ref());
    let current_dir_changed = last_current.as_ref().is_none_or(|last| shown(last) != shown(&current));
    if moved.is_empty() && !current_dir_changed {
        return RefreshOutcome {
            recheck,
            ..unchanged(lit, last_current)
        };
    }
    let folders = moved
        .iter()
        .map(|(position, _)| FolderSizes {
            path: paths[*position].clone(),
            stats: stats[*position].clone(),
        })
        .collect();
    RefreshOutcome {
        event: Some(ListingIndexSizesChanged {
            listing_id: listing_id.to_string(),
            full: false,
            folders,
            current_dir_changed,
            current_dir: current.clone(),
        }),
        lit,
        current_dir: Some(current),
        recheck,
    }
}

/// The rows (and the listing's own folder) whose readings say their hourglass flips on its own,
/// with the soonest flip.
fn recheck_of(
    rows: &[(String, FileEntry)],
    stats: &[Option<DirStats>],
    current: Option<&DirStats>,
) -> Option<(Duration, Touched)> {
    let flips_in = |reading: Option<&DirStats>| reading.and_then(|stats| stats.recursive_size_pending_changes_in);
    let own = flips_in(current);
    let children: Vec<(&str, Duration)> = rows
        .iter()
        .zip(stats)
        .filter_map(|((_, entry), reading)| flips_in(reading.as_ref()).map(|after| (entry.name.as_str(), after)))
        .collect();
    let soonest = own.into_iter().chain(children.iter().map(|(_, after)| *after)).min()?;
    Some((
        soonest,
        Touched::Rows {
            own: own.is_some(),
            children: children
                .into_iter()
                .map(|(name, _)| name.to_string())
                .collect::<BTreeSet<_>>(),
        },
    ))
}

/// The listing's own path and the folder rows `touched` names, with their cached entries, or `None`
/// when the listing is gone.
fn rows_to_read(listing_id: &str, touched: &Touched) -> Option<(String, Vec<(String, FileEntry)>)> {
    let cache = LISTING_CACHE.read_ignore_poison();
    let listing = cache.get(listing_id)?;
    let dir_path = listing.path.as_path().to_string_lossy().into_owned();
    let is_folder = |entry: &&FileEntry| entry.is_directory && !entry.is_symlink;
    let rows = match touched {
        // Whole reads nothing per row: the full re-enrich covers them.
        Touched::Whole => Vec::new(),
        Touched::Rows { children, .. } => listing
            .entries()
            .iter()
            .filter(is_folder)
            .filter(|entry| children.contains(&entry.name))
            .map(|entry| (entry.path.clone(), entry.clone()))
            .collect(),
    };
    Some((dir_path, rows))
}
