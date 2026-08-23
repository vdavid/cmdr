//! Per-batch, per-folder activity rollups over the live loop's CORRECTED stream.
//!
//! A second observer beside `churn_monitor.rs`, and deliberately a different one. The churn
//! monitor measures raw deduplicated paths, which is right for "how hard does this subtree
//! churn" and wrong for "did something meaningful happen here": a rename arrives there as a
//! create plus a delete, and an `rm -rf` as sixty thousand removals. This one folds the batch
//! AFTER `detect_renames_by_inode` and the removal-storm coalescing, so a rename is one
//! `Renamed` and a storm is one `Removed` at its anchor.
//!
//! What it produces is one [`IndexEvent::FolderActivity`] per batch, carrying a
//! [`FolderChangeRollup`] per folder. ❌ Never one message per file: `INGESTION_HARD_CAP` is
//! 5,000,000, and a per-file message would put five million of them across the host boundary on
//! exactly the path the counters exist to survive.
//!
//! The host decides what the rollups are FOR; this module names no consumer.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::churn_monitor::ChurnObserver;
use super::watcher::FsEventFlags;
use crate::indexing::events::{EventSink, FolderChangeRollup, IndexEvent};

/// Log target for the one line this module can emit.
const LOG_TARGET: &str = "indexing::activity";

/// How many distinct folders one batch reports before the rest are dropped and counted.
///
/// A batch is bounded by `INGESTION_HARD_CAP`, so nothing bounds its distinct FOLDERS except
/// the disk. A `Vec` of half a million rollups crossing the boundary, and a host looping over
/// it on the live-loop thread, is the failure the per-folder shape exists to avoid in the first
/// place. Past the cap the batch reports the folders it saw first and says nothing about the
/// rest, which is the same bargain the rollup itself makes: signal, not a ledger.
const MAX_FOLDERS_PER_BATCH: usize = 4_096;

/// What one already-corrected event says happened, once its flags have been reduced to a
/// single answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) enum ChangeKind {
    /// The entry appeared.
    Created,
    /// Its content changed.
    Modified,
    /// It went away.
    Removed,
    /// It was renamed in place.
    Renamed,
}

/// Reduce one event's flags to the single kind it counts as.
///
/// ⚠️ **The flags are not one-hot.** One coalesced `FsChangeEvent` can carry `item_created`,
/// `item_removed`, and `item_renamed` at once, because the watcher merges everything it saw for
/// a path within the batch. So SOME priority has to be chosen, and a different order moves what
/// a consumer reads out of the counts materially. This one is:
///
/// 1. **Renamed** — the strongest statement of intent, and the one the inode pre-pass has
///    already worked to identify. A rename that also created and removed a name is still a
///    rename.
/// 2. **Created** — a new thing arriving is the next most meaningful. Preferred over `Removed`
///    so a create-then-delete churn cycle reads as arrival rather than disappearance.
/// 3. **Removed** — something went away.
/// 4. **Modified** — the churn floor: an existing thing was written to.
///
/// `None` for an event carrying none of the four (a bare `must_scan_sub_dirs` anchor, say):
/// it names no change to count, and inventing one would inflate every rescan into activity.
pub(in crate::indexing) fn kind_of(flags: &FsEventFlags) -> Option<ChangeKind> {
    if flags.item_renamed {
        Some(ChangeKind::Renamed)
    } else if flags.item_created {
        Some(ChangeKind::Created)
    } else if flags.item_removed {
        Some(ChangeKind::Removed)
    } else if flags.item_modified {
        Some(ChangeKind::Modified)
    } else {
        None
    }
}

/// The folder an event at `path` belongs to: its PARENT directory.
///
/// ⚠️ A directory's own event counts in its parent too, ❌ never in itself. A rollup describes
/// the folder a change happened IN, and `/a/b` being created is a change in `/a`.
///
/// `None` for a path with no parent (the root, or an empty string): nothing to credit.
fn parent_folder(path: &str) -> Option<&str> {
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    match trimmed.rfind('/') {
        Some(0) => Some("/"),
        Some(cut) => Some(&trimmed[..cut]),
        None => None,
    }
}

/// Unix seconds now. The batch's own instant, read once per batch rather than per event.
fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// One folder's counts while a batch is being folded.
#[derive(Default)]
struct Counts {
    created: u32,
    modified: u32,
    removed: u32,
    renamed: u32,
}

impl Counts {
    /// Count one more change. Saturating, because a counter that wrapped would turn the
    /// busiest folder on the disk into the quietest.
    fn record(&mut self, kind: ChangeKind) {
        let slot = match kind {
            ChangeKind::Created => &mut self.created,
            ChangeKind::Modified => &mut self.modified,
            ChangeKind::Removed => &mut self.removed,
            ChangeKind::Renamed => &mut self.renamed,
        };
        *slot = slot.saturating_add(1);
    }
}

/// Folds one live batch into per-folder counts and reports them through the sink.
///
/// State lives only for the length of a batch: [`report`](Self::report) drains the map, so
/// nothing accumulates between batches and memory is bounded by one batch's folders.
pub(in crate::indexing) struct ActivityObserver {
    volume_id: String,
    events: Arc<dyn EventSink>,
    folders: HashMap<String, Counts>,
    /// Folders this batch couldn't track because [`MAX_FOLDERS_PER_BATCH`] was reached.
    dropped: u64,
}

impl ActivityObserver {
    /// An observer reporting `volume_id`'s batches through `events`.
    fn new(volume_id: &str, events: Arc<dyn EventSink>) -> Self {
        ActivityObserver {
            volume_id: volume_id.to_string(),
            events,
            folders: HashMap::new(),
            dropped: 0,
        }
    }

    /// Credit one change of `kind` to `folder`.
    fn credit(&mut self, folder: &str, kind: ChangeKind) {
        if let Some(counts) = self.folders.get_mut(folder) {
            counts.record(kind);
            return;
        }
        if self.folders.len() >= MAX_FOLDERS_PER_BATCH {
            self.dropped += 1;
            return;
        }
        self.folders.entry(folder.to_string()).or_default().record(kind);
    }

    /// Fold one event in, crediting the folder it happened in.
    pub(in crate::indexing) fn record_event(&mut self, path: &str, flags: &FsEventFlags) {
        let Some(kind) = kind_of(flags) else {
            return;
        };
        self.record(path, kind);
    }

    /// Fold one change of a known kind in, crediting the folder it happened in.
    ///
    /// The inode rename pre-pass needs this: it takes its matched events OUT of the batch, so
    /// their flags never reach [`record_event`](Self::record_event) and only the FAILED matches
    /// would otherwise be counted — exactly the wrong half.
    pub(in crate::indexing) fn record(&mut self, path: &str, kind: ChangeKind) {
        let Some(folder) = parent_folder(path) else {
            return;
        };
        self.credit(folder, kind);
    }

    /// Fold a removal-storm anchor in as ONE removal inside the anchor itself.
    ///
    /// ⚠️ The anchor is the folder the storm happened IN, so this is the one input credited to
    /// the named folder rather than to its parent. The storm path drops every strict-descendant
    /// removal in favour of a subtree rescan, so without this a sixty-thousand-file delete
    /// inside a surviving folder would contribute nothing at all. One removal, not sixty
    /// thousand: what the batch actually knows is that the folder emptied out, and the
    /// per-file count is exactly what the coalescing threw away.
    pub(in crate::indexing) fn record_storm_anchor(&mut self, anchor: &str) {
        self.credit(anchor, ChangeKind::Removed);
    }

    /// Close the batch: emit what it saw, and reset.
    ///
    /// Emits nothing for a batch that saw no countable change, so a quiet loop stays quiet.
    fn report(&mut self, observed_at: u64) {
        let dropped = std::mem::take(&mut self.dropped);
        if dropped > 0 {
            log::debug!(
                target: LOG_TARGET,
                "activity rollup: {dropped} folder(s) past the per-batch cap of {MAX_FOLDERS_PER_BATCH} went unreported",
            );
        }
        if self.folders.is_empty() {
            return;
        }
        let folders: Vec<FolderChangeRollup> = self
            .folders
            .drain()
            .map(|(folder, counts)| FolderChangeRollup {
                folder,
                created: counts.created,
                modified: counts.modified,
                removed: counts.removed,
                renamed: counts.renamed,
                // Every event in one batch shares the batch's instant: the stream carries event
                // IDs, not timestamps, and a live batch spans milliseconds.
                last_event_at: observed_at,
            })
            .collect();
        self.folders.shrink_to_fit();
        self.events.emit(IndexEvent::FolderActivity {
            volume_id: self.volume_id.clone(),
            observed_at,
            folders,
        });
    }
}

/// What one live batch is observed by: the churn monitor and the activity tap, together.
///
/// ⚠️ **One struct rather than two parameters, and that is not a style choice.**
/// `process_live_batch` sits at exactly seven arguments and `clippy::too_many_arguments`
/// defaults to seven, which `clippy.toml` doesn't raise. Bundling keeps the signature where it
/// is and keeps both observers under the same guarantee: the batch takes them by `&mut`, so it
/// cannot be processed without them.
///
/// Both scanners (`churn_monitor/tests.rs` and `activity_monitor/tests.rs`) assert that every
/// live-batch driver builds one of these with [`from_env`](Self::from_env), which is the part
/// the compiler can't see — a third live loop in a new file, or an existing one downgrading to
/// the test-only `disabled` pair.
pub(in crate::indexing) struct BatchObservers {
    churn: ChurnObserver,
    activity: ActivityObserver,
}

impl BatchObservers {
    /// The real pair for a live loop: an env-gated churn monitor and a tap reporting through
    /// the loop's own sink.
    pub(in crate::indexing) fn from_env(volume_id: &str, events: Arc<dyn EventSink>, now: Instant) -> Self {
        BatchObservers {
            churn: ChurnObserver::from_env(volume_id, now),
            activity: ActivityObserver::new(volume_id, events),
        }
    }

    /// A pair that records nothing and reports nowhere. Test-only on purpose: production code
    /// has no way to opt out, so every live batch carries real observers.
    #[cfg(test)]
    pub(in crate::indexing) fn disabled() -> Self {
        BatchObservers {
            churn: ChurnObserver::disabled(),
            activity: ActivityObserver::new("", crate::indexing::events::NoopEventSink::shared()),
        }
    }

    /// A pair whose tap reports into `events`, for a test asserting on the rollups.
    #[cfg(test)]
    pub(in crate::indexing) fn tapping(volume_id: &str, events: Arc<dyn EventSink>) -> Self {
        BatchObservers {
            churn: ChurnObserver::disabled(),
            activity: ActivityObserver::new(volume_id, events),
        }
    }

    /// Supply the owning loop's cumulative raw-event count for this batch, for the churn
    /// monitor's dedup ratio. Returns `&mut Self` so the call reads as one expression at the
    /// `process_live_batch` call site.
    pub(in crate::indexing) fn with_raw_total(&mut self, raw_total: u64) -> &mut Self {
        self.churn.with_raw_total(raw_total);
        self
    }

    /// Fold the batch's raw deduplicated paths into the churn monitor, and emit its rollup if
    /// the period closed. Called before the batch drains, and before the empty-batch bail: an
    /// idle period must still close, or the time series grows holes exactly where "this subtree
    /// went quiet" is the answer.
    pub(in crate::indexing) fn observe_churn<'a>(&mut self, paths: impl Iterator<Item = &'a str>, now: Instant) {
        self.churn.observe(paths, now);
    }

    /// The activity tap, to fold each corrected change into.
    pub(in crate::indexing) fn activity(&mut self) -> &mut ActivityObserver {
        &mut self.activity
    }

    /// Close the batch and report what the tap saw.
    pub(in crate::indexing) fn finish_batch(&mut self) {
        self.activity.report(now_unix_secs());
    }
}

#[cfg(test)]
mod tests;
