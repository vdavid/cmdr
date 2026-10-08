//! Calculate folder sizes on demand: Total Commander's ⌥⇧⏎ ("count the space
//! subfolders occupy") and Space on a folder.
//!
//! The drive index answers folder sizes for the volumes it runs on; this walks
//! the folders it can't answer for (SFTP, WebDAV, S3, archives, an excluded
//! folder, any volume while indexing is off), one at a time (`measure.rs`).
//!
//! What the pane sees, through the event the index already uses (`publish.rs`):
//! a folder being walked shows its running total with the hourglass, from
//! [`HOURGLASS_DELAY`] in (a quick folder goes straight to its exact size, no
//! flicker) and every [`PROGRESS_EVERY`] after; a folder waiting its turn shows
//! the hourglass too, from [`HOURGLASS_DELAY`] after it was queued; a finished
//! one its exact size, or a lower bound (`≥`) when parts couldn't be read.
//!
//! Requests queue (`jobs.rs`): Space on another folder while one is walked adds
//! it to the queue rather than abandoning the first. Esc stops the job: the
//! folder being walked keeps what it counted as a lower bound, and the rows still
//! waiting go back to what they showed. A refresh that re-reads the folder drops
//! the readings, as Total Commander's does.

mod jobs;
#[cfg(test)]
mod jobs_test;
#[cfg(test)]
mod lifecycle_test;
mod measure;
#[cfg(test)]
mod measure_test;
mod publish;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::time::MissedTickBehavior;

use crate::file_system::listing::cached_listing::LISTING_CACHE;
use crate::file_system::volume::Volume;
use crate::ignore_poison::RwLockIgnorePoison;

use super::refresh::RowSizes;
pub(crate) use jobs::cancel;
use jobs::{Job, Joined, Queued};
use measure::{Live, MeasureError};
use publish::{Sink, Walk, Wrote, publish, reading, restore};

/// How long a folder is walked (or waits) before its hourglass shows: a count
/// that's done sooner never blinks one.
pub(crate) const HOURGLASS_DELAY: Duration = Duration::from_millis(200);
/// How often a folder being walked sends its running total, after the first.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// How often the walker looks at the running total and the queue.
const TICK: Duration = Duration::from_millis(50);

/// Why a count didn't start. Typed, so the frontend never reads a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CountFolderSizesError {
    /// The pane's listing is no longer cached (it moved on).
    Gone { listing_id: String },
    /// No volume answers for the listing's folder (unplugged, disconnected).
    NotConnected { volume_id: String },
}

/// How a count ended. A request that joined a running count gets that count's outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderSizeCountOutcome {
    /// Folders whose size landed in a pane that still shows them (exact, or a
    /// lower bound when parts couldn't be read).
    pub counted: usize,
    /// Folders it couldn't read, wholly (their rows went back to what they showed)
    /// or in part (their size is a lower bound).
    pub unreadable: usize,
    /// Stopped early: Esc, or the listing closing.
    pub cancelled: bool,
}

/// What a count will walk: the listing's volume and folder, and the folder rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CountPlan {
    pub volume_id: String,
    pub dir: PathBuf,
    pub folders: Vec<PlannedFolder>,
}

/// One folder a count will walk, with what its row shows now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedFolder {
    pub path: String,
    pub(in crate::listing_index_sizes) before: RowSizes,
}

/// The folder rows a pane shows that a count should walk, in row order.
///
/// Where the drive index runs on the volume (`index_owns`; ❌ not merely
/// registered: one shutting down or failed answers nothing), it owns the sizes it
/// has, even a lower bound it's still completing: only a folder it says nothing
/// about (excluded, or inside an archive) is walked, plus one whose size a stopped
/// count left (ours, so retryable). Elsewhere, every folder without an exact
/// size. With `only` (Space on a folder), just those paths. Symlinked folders are
/// skipped, as the index skips them.
pub(crate) fn plan(
    listing_id: &str,
    include_hidden: bool,
    only: Option<&[String]>,
    index_owns: impl Fn(&str) -> bool,
) -> Result<CountPlan, CountFolderSizesError> {
    let manual = jobs::manual_paths(listing_id, publish::listing_is_open);
    let cache = LISTING_CACHE.read_ignore_poison();
    let listing = cache.get(listing_id).ok_or_else(|| CountFolderSizesError::Gone {
        listing_id: listing_id.to_string(),
    })?;
    listing.touch();
    let owned = index_owns(&listing.volume_id);
    let folders = listing
        .rows(include_hidden)
        .iter()
        .filter(|entry| entry.is_directory && !entry.is_symlink)
        .filter(|entry| only.is_none_or(|paths| paths.contains(&entry.path)))
        .filter(|entry| {
            let exact = entry.recursive_size.is_some() && entry.recursive_size_complete == Some(true);
            match owned {
                true => entry.recursive_size.is_none() || (manual.contains(&entry.path) && !exact),
                false => !exact,
            }
        })
        .map(|entry| PlannedFolder {
            path: entry.path.clone(),
            before: RowSizes::of(entry, false),
        })
        .collect();
    Ok(CountPlan {
        volume_id: listing.volume_id.clone(),
        dir: listing.path.as_path().to_path_buf(),
        folders,
    })
}

/// Counts the folders [`plan`] picks, resolving the listing's volume the way a
/// copy scan does (an archive or `.git` route included). `sink` receives each
/// event, after its reading is in the listing cache.
pub(crate) async fn count(
    listing_id: &str,
    include_hidden: bool,
    only: Option<&[String]>,
    sink: Sink<'_>,
) -> Result<FolderSizeCountOutcome, CountFolderSizesError> {
    let index = crate::index_host::index();
    let plan = plan(listing_id, include_hidden, only, |volume_id| {
        index.volume_status(volume_id).enabled
    })?;
    let volume_id = plan.volume_id.clone();
    let dir = plan.dir.clone();
    count_resolving(
        listing_id,
        plan,
        async move {
            crate::file_system::volume::manager::get_volume_manager()
                .resolve(&volume_id, &dir)
                .await
                .volume
        },
        sink,
    )
    .await
}

async fn count_resolving(
    listing_id: &str,
    mut requested: CountPlan,
    resolve: impl Future<Output = Option<Arc<dyn Volume>>> + Send + 'static,
    sink: Sink<'_>,
) -> Result<FolderSizeCountOutcome, CountFolderSizesError> {
    let job = loop {
        let folders = requested.folders.iter().map(|f| (f.path.clone(), f.before)).collect();
        match jobs::enqueue(listing_id, folders, Instant::now()) {
            Joined::Owner(job) => break job,
            Joined::Waiter { appended, done } => {
                log::debug!(target: "folder_sizes", "count for {listing_id} queued {appended} folder(s) on the running one");
                return Ok(wait_for_end(done).await);
            }
            Joined::Retiring(done) => {
                // No replacement may publish before the old owner's final rows.
                // Re-read their sizes afterwards, including the exact-size skip.
                let _ = wait_for_end(done).await;
                let only: Vec<_> = requested.folders.iter().map(|f| f.path.clone()).collect();
                requested = plan(listing_id, true, Some(&only), |volume_id| {
                    crate::index_host::index().volume_status(volume_id).enabled
                })?;
            }
        }
    };
    if requested.folders.is_empty() {
        jobs::end(listing_id, &job);
        let outcome = FolderSizeCountOutcome::default();
        job.finish(outcome.clone());
        return Ok(outcome);
    }
    // Resolution can itself issue backend I/O. The task owns its lifetime, but
    // has no publication authority, just like the measurement worker.
    let mut resolving = tokio::spawn(resolve);
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let volume = loop {
        if job.is_cancelled() {
            finish_cancelled(listing_id, &job, sink);
            let outcome = FolderSizeCountOutcome {
                cancelled: true,
                ..Default::default()
            };
            job.finish(outcome.clone());
            return Ok(outcome);
        }
        tokio::select! {
            resolved = &mut resolving => {
                // Cancellation wins even when resolution finishes in the same turn.
                if job.is_cancelled() { continue; }
                match resolved {
                    Ok(Some(volume)) => break volume,
                    Ok(None) | Err(_) => {
                        job.cancel();
                        finish_cancelled(listing_id, &job, sink);
                        job.finish(FolderSizeCountOutcome { cancelled: true, ..Default::default() });
                        return Err(CountFolderSizesError::NotConnected { volume_id: requested.volume_id });
                    }
                }
            }
            _ = tick.tick() => light_waiting(listing_id, &job, sink),
        }
    };
    log::info!(target: "folder_sizes", "count started for {listing_id} on {}: {} folder(s), {}",
        requested.volume_id, requested.folders.len(), if volume.local_path().is_some() { "local walk" } else { "volume scan" });
    Ok(run(listing_id, &job, &volume, sink).await)
}

/// Queues `plan`'s folders on `listing_id`'s count, running it when none runs,
/// and answers how the count ended.
#[cfg(test)]
pub(crate) async fn count_with(
    listing_id: &str,
    plan: CountPlan,
    volume: Arc<dyn Volume>,
    sink: Sink<'_>,
) -> FolderSizeCountOutcome {
    count_resolving(listing_id, plan, std::future::ready(Some(volume)), sink)
        .await
        .expect("test volume is connected")
}

async fn wait_for_end(
    mut done: tokio::sync::watch::Receiver<Option<FolderSizeCountOutcome>>,
) -> FolderSizeCountOutcome {
    loop {
        if let Some(outcome) = done.borrow().clone() {
            return outcome;
        }
        if done.changed().await.is_err() {
            return FolderSizeCountOutcome {
                cancelled: true,
                ..Default::default()
            };
        }
    }
}

fn finish_cancelled(listing_id: &str, job: &Arc<Job>, sink: Sink<'_>) {
    for waiting in job.drain() {
        if waiting.lit {
            restore(listing_id, &waiting.path, waiting.before, waiting.before_manual, sink);
        }
    }
    jobs::end(listing_id, job);
}

/// What one folder's walk came to.
enum Step {
    Counted,
    Partial,
    Unreadable,
    /// The row went away (deleted, renamed) while it waited.
    Gone,
    Stopped,
}

/// The owner's loop: walks the queue until it drains or the job stops.
async fn run(listing_id: &str, job: &Arc<Job>, volume: &Arc<dyn Volume>, sink: Sink<'_>) -> FolderSizeCountOutcome {
    let started = Instant::now();
    let mut outcome = FolderSizeCountOutcome::default();
    loop {
        if job.is_cancelled() {
            break;
        }
        light_waiting(listing_id, job, sink);
        let Some(folder) = jobs::next(listing_id, job) else {
            break;
        };
        match walk_one(listing_id, job, volume, &folder, sink).await {
            Step::Counted => outcome.counted += 1,
            Step::Partial => {
                outcome.counted += 1;
                outcome.unreadable += 1;
            }
            Step::Unreadable => outcome.unreadable += 1,
            Step::Gone => {}
            Step::Stopped => break,
        }
    }
    if job.is_cancelled() {
        outcome.cancelled = true;
        finish_cancelled(listing_id, job, sink);
    }
    log::info!(target: "folder_sizes", "count for {listing_id} {}: {} counted, {} unreadable, {:?}",
        if outcome.cancelled { "stopped" } else { "finished" }, outcome.counted, outcome.unreadable, started.elapsed());
    job.finish(outcome.clone());
    outcome
}

/// Lights the hourglass on the queued rows that have waited [`HOURGLASS_DELAY`].
fn light_waiting(listing_id: &str, job: &Job, sink: Sink<'_>) {
    for (path, before) in job.due_to_light(Instant::now(), HOURGLASS_DELAY) {
        if publish(listing_id, &path, before.waiting(&path), sink) == Wrote::NoListing {
            job.cancel();
        }
    }
}

async fn walk_one(listing_id: &str, job: &Arc<Job>, volume: &Arc<dyn Volume>, folder: &Queued, sink: Sink<'_>) -> Step {
    let path = folder.path.as_str();
    let live = Arc::new(Live::default());
    let started = Instant::now();
    // When the row last showed a running total; `None` while it hasn't shown one.
    let mut shown: Option<Instant> = None;
    let mut walk = measure::start(Arc::clone(volume), path.to_string(), Arc::clone(job), Arc::clone(&live));
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let result = loop {
        tokio::select! {
            result = &mut walk => break result.unwrap_or_else(|err| Err(MeasureError::Unreadable(err.to_string()))),
            _ = tick.tick() => {
                // The UI stops waiting, but dropping a JoinHandle does NOT drop
                // backend I/O. Its worker observes ScanStop after the in-flight
                // operation returns and never publishes a late result.
                if job.is_cancelled() {
                    break Err(MeasureError::Stopped);
                }
                let now = Instant::now();
                let due = shown.map_or(now.duration_since(started) >= HOURGLASS_DELAY, |at| {
                    now.duration_since(at) >= PROGRESS_EVERY
                });
                if due {
                    shown = Some(now);
                    let progress = live.snapshot();
                    if publish(listing_id, path, reading(path, progress, progress.bytes, Walk::Running), sink)
                        == Wrote::NoListing
                    {
                        job.cancel();
                    }
                }
                light_waiting(listing_id, job, sink);
            }
        }
    };
    match result {
        Ok(measured) if !job.is_cancelled() => {
            let walk = if measured.skipped == 0 {
                Walk::Done
            } else {
                Walk::Partial
            };
            if measured.skipped > 0 {
                log::info!(target: "folder_sizes", "count for {listing_id}: {} item(s) unreadable in one folder; its size is a lower bound", measured.skipped);
            }
            match publish(
                listing_id,
                path,
                reading(path, measured.progress, measured.physical, walk),
                sink,
            ) {
                Wrote::Row if walk == Walk::Done => Step::Counted,
                Wrote::Row => Step::Partial,
                Wrote::NoRow => Step::Gone,
                Wrote::NoListing => {
                    job.cancel();
                    Step::Stopped
                }
            }
        }
        Ok(_) | Err(MeasureError::Stopped) => {
            // A row that showed a running total keeps it as a lower bound; one that
            // never got that far goes back to what it showed.
            match shown {
                Some(_) => {
                    let progress = live.snapshot();
                    let _ = publish(
                        listing_id,
                        path,
                        reading(path, progress, progress.bytes, Walk::Partial),
                        sink,
                    );
                }
                None => restore(listing_id, path, folder.before, folder.before_manual, sink),
            }
            Step::Stopped
        }
        Err(MeasureError::Unreadable(why)) => {
            log::info!(target: "folder_sizes", "count for {listing_id}: couldn't read a folder: {why}");
            restore(listing_id, path, folder.before, folder.before_manual, sink);
            Step::Unreadable
        }
    }
}
