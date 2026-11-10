//! A delete or trash of an item that's already gone counts as done.
//!
//! The person asked for the item not to be there, and it isn't: something else
//! (another app, a sync client, a script) moved or deleted it first, often while
//! the pane still showed it (ERR-X84HN). So before the walk, each top-level
//! source gets ONE existence probe, and a gone one is dropped from the batch,
//! reported `Skipped` with `source_removed: true`, and counted on the completion
//! event's `already_gone`. ❌ Never a `SourceNotFound` failure: nothing went
//! wrong, and a failure there auto-sent an error report for a non-event.
//!
//! Only a POSITIVE absence counts: a local source through the queue check's
//! probe (`queued_sources.rs`), a volume source through the walk's own
//! top-level probe, whose answer the walk then reuses (`split_on`). A source that can't be looked at (permission, a
//! dropped share) stays in the batch for the engine, whose typed error says what
//! really happened. "We couldn't look" is not "it's gone".
//!
//! It runs BEFORE the source binding and the queue check, so a queued delete
//! whose items left meanwhile completes instead of asking "delete the rest?"
//! about items whose absence is the goal. The scan wait before it steps past a
//! preview that found a source missing (`scan_bridge::MissingSource`), so this
//! probe gets the last word (ERR-7YG5T).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::super::event_sinks::{OperationEventSink, SinkDecorator};
use super::super::queued_sources::local_source_is_gone;
use super::super::types::{
    AlreadyGone, SourceItemOutcome, WriteCompleteEvent, WriteOperationType, WriteSourceItemDoneEvent,
};
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{Volume, VolumeError};

use super::outage::Listed;
use super::volume_scan::top_level_entry;

/// The positions in `sources` still there, plus what the probe found gone.
pub(crate) struct GoneSplit {
    /// Positions of the sources the operation still has to handle, in order.
    pub kept: Vec<usize>,
    /// What the completion event reports, or `None` when nothing was gone.
    pub already_gone: Option<AlreadyGone>,
}

impl GoneSplit {
    /// The items at `kept`, in order. Positional, so a parallel list (trash's
    /// item sizes) filters through it too.
    pub(crate) fn select<T: Clone>(&self, items: &[T]) -> Vec<T> {
        self.kept
            .iter()
            .filter_map(|position| items.get(*position).cloned())
            .collect()
    }
}

/// Splits a LOCAL batch, announcing each gone source as it goes.
pub(crate) fn split_local(events: &dyn OperationEventSink, operation_id: &str, sources: &[PathBuf]) -> GoneSplit {
    let gone: Vec<bool> = sources.iter().map(|source| local_source_is_gone(source)).collect();
    finish_split(events, operation_id, sources, &gone)
}

/// A volume batch's split, plus what the probe learned of each source still
/// there, which the walk takes as its top-level answer
/// (`delete_volume_files_with_progress_probed`).
pub(super) struct VolumeGoneSplit {
    pub split: GoneSplit,
    pub probed: HashMap<PathBuf, Listed>,
}

/// [`split_local`] for a batch on a `Volume`.
///
/// ❗ It asks the walk's own top-level question (`top_level_entry`: link-aware,
/// one request), and the walk reuses the answer. A second probe per item cost
/// an object store a HEAD (a folder, a listing too) its estimate never counts.
pub(super) async fn split_on(
    volume: &dyn Volume,
    events: &dyn OperationEventSink,
    operation_id: &str,
    sources: &[PathBuf],
) -> VolumeGoneSplit {
    let mut gone = Vec::with_capacity(sources.len());
    let mut probed = HashMap::new();
    for source in sources {
        match top_level_entry(volume, source).await {
            Ok(listed) => {
                probed.insert(source.clone(), listed);
                gone.push(false);
            }
            Err(error) => gone.push(matches!(error, VolumeError::NotFound(_))),
        }
    }
    VolumeGoneSplit {
        split: finish_split(events, operation_id, sources, &gone),
        probed,
    }
}

fn finish_split(events: &dyn OperationEventSink, operation_id: &str, sources: &[PathBuf], gone: &[bool]) -> GoneSplit {
    let kept: Vec<usize> = (0..sources.len()).filter(|position| !gone[*position]).collect();
    let gone_sources: Vec<&PathBuf> = sources
        .iter()
        .zip(gone)
        .filter(|(_, gone)| **gone)
        .map(|(s, _)| s)
        .collect();
    if gone_sources.is_empty() {
        return GoneSplit {
            kept,
            already_gone: None,
        };
    }
    for source in &gone_sources {
        // Info, not warn: an expected outcome, and warn reaches error-report bundles.
        log::info!(
            target: "already_gone",
            "op={operation_id}: {} was already gone; counting it as done",
            LogPrivate::path(source)
        );
        events.emit_source_item_done(WriteSourceItemDoneEvent {
            operation_id: operation_id.to_string(),
            source_path: source.display().to_string(),
            source_removed: true,
            // Skipped, not Done: this operation removed nothing, and a moved-away
            // file still exists somewhere. A per-source consumer (an approved
            // suggestion's status) must not record it as deleted.
            outcome: SourceItemOutcome::Skipped,
        });
    }
    let name = match gone_sources.as_slice() {
        [only] => Some(display_name(only)),
        _ => None,
    };
    GoneSplit {
        kept,
        already_gone: Some(AlreadyGone {
            item_count: gone_sources.len(),
            total_count: sources.len(),
            name,
        }),
    }
}

fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The terminal event for a batch whose every item was already gone: it
/// completed, with nothing to do.
pub(crate) fn complete_all_gone(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    already_gone: AlreadyGone,
) {
    let mut complete = WriteCompleteEvent::all_skipped(operation_id, operation_type, 0);
    complete.already_gone = Some(already_gone);
    events.emit_complete(complete);
}

/// A sink that stamps `already_gone` on the completion the walk emits, and
/// forwards everything else untouched (a [`SinkDecorator`]). The walkers stay
/// unaware of the pre-flight; the note rides the one terminal event the FE
/// already reads.
pub(crate) struct AlreadyGoneSink<'a> {
    inner: &'a dyn OperationEventSink,
    already_gone: Option<AlreadyGone>,
}

impl<'a> AlreadyGoneSink<'a> {
    pub(crate) fn new(inner: &'a dyn OperationEventSink, already_gone: Option<AlreadyGone>) -> Self {
        Self { inner, already_gone }
    }
}

impl SinkDecorator for AlreadyGoneSink<'_> {
    fn inner(&self) -> &dyn OperationEventSink {
        self.inner
    }
    fn emit_complete(&self, mut event: WriteCompleteEvent) {
        if event.already_gone.is_none() {
            event.already_gone.clone_from(&self.already_gone);
        }
        self.inner.emit_complete(event);
    }
}
