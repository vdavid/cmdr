//! Where the index's typed events go.
//!
//! The `EventSink` trait and the `IndexEvent` enum live in `../events/sink.rs`;
//! this is only the injection point. It exists so the two places that START a
//! subsystem — a drive index and the media-enrichment scheduler — can get a sink
//! without constructing an app-side one, which would put the app's event mapping
//! back inside the index.
//!
//! Everything downstream already threads `Arc<dyn EventSink>` through
//! constructors, which is the shape to keep: read this once where a subsystem
//! starts, then pass it down. ❌ Don't reach back here from deep inside a scan.

use std::sync::{Arc, OnceLock};

use crate::indexing::events::{EventSink, NoopEventSink};

static INSTALLED: OnceLock<Arc<dyn EventSink>> = OnceLock::new();

/// A [`set_event_sink`] call that arrived after one was already installed.
#[derive(Debug)]
pub(crate) struct EventSinkAlreadySet;

/// Tells the index where to report. Call once at startup. A second call keeps the
/// first sink, so a late caller can't redirect events away from a running scan.
pub(crate) fn set_event_sink(sink: Arc<dyn EventSink>) -> Result<(), EventSinkAlreadySet> {
    INSTALLED.set(sink).map_err(|_| EventSinkAlreadySet)
}

/// The installed sink, or a no-op one when nothing was installed.
///
/// Dropping events is the right default for a test binary or a tool: they have no
/// frontend to tell, and a subsystem that couldn't report would otherwise have to
/// grow an error path for something that can't be acted on.
pub(crate) fn current() -> Arc<dyn EventSink> {
    if let Some(installed) = INSTALLED.get() {
        return Arc::clone(installed);
    }
    NoopEventSink::shared()
}
