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

use std::sync::{Arc, OnceLock, RwLock};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::indexing::events::{EventSink, NoopEventSink};

/// The installed sink. An `RwLock` rather than a `OnceLock` because tests swap it
/// (see [`install_for_test`]); production writes it exactly once.
static INSTALLED: RwLock<Option<Arc<dyn EventSink>>> = RwLock::new(None);

/// A [`set_event_sink`] call that arrived after one was already installed.
#[derive(Debug)]
pub(crate) struct EventSinkAlreadySet;

/// Tells the index where to report. Call once at startup. A second call keeps the
/// first sink, so a late caller can't redirect events away from a running scan.
pub(crate) fn set_event_sink(sink: Arc<dyn EventSink>) -> Result<(), EventSinkAlreadySet> {
    let mut slot = INSTALLED.write_ignore_poison();
    if slot.is_some() {
        return Err(EventSinkAlreadySet);
    }
    *slot = Some(sink);
    Ok(())
}

/// The installed sink, or a no-op one when nothing was installed.
///
/// Dropping events is the right default for a test binary or a tool: they have no
/// frontend to tell, and a subsystem that couldn't report would otherwise have to
/// grow an error path for something that can't be acted on.
pub(crate) fn current() -> Arc<dyn EventSink> {
    if let Some(installed) = INSTALLED.read_ignore_poison().as_ref() {
        return Arc::clone(installed);
    }
    static FALLBACK: OnceLock<Arc<dyn EventSink>> = OnceLock::new();
    Arc::clone(FALLBACK.get_or_init(NoopEventSink::shared))
}

/// Report into `sink` for the duration of one test, restoring whatever was there
/// when the returned guard drops.
///
/// The slot is process-wide, so hold the index handle's test lock first.
#[cfg(any(test, feature = "testing"))]
#[must_use = "the sink is restored when the guard drops"]
pub fn install_for_test(sink: Arc<dyn EventSink>) -> TestSinkGuard {
    TestSinkGuard {
        previous: INSTALLED.write_ignore_poison().replace(sink),
    }
}

/// Restores the previously-installed sink on drop, including on a panic, so one
/// failing test can't leave every later one reporting into its recorder.
#[cfg(any(test, feature = "testing"))]
pub struct TestSinkGuard {
    previous: Option<Arc<dyn EventSink>>,
}

#[cfg(any(test, feature = "testing"))]
impl Drop for TestSinkGuard {
    fn drop(&mut self) {
        *INSTALLED.write_ignore_poison() = self.previous.take();
    }
}
