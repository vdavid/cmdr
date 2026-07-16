//! Typed Tauri progress + terminal events for image enrichment: image
//! indexing joins the top-right indexing indicator as a SECOND publisher alongside the
//! drive indexer, so the user sees honest per-volume progress + ETA and knows when a
//! sweep is running.
//!
//! Two events per pass:
//! - [`MediaEnrichProgressEvent`] (`media-enrich-progress`), throttled — the live bar.
//! - [`MediaEnrichTerminalEvent`] (`media-enrich-terminal`), exactly one per pass on
//!   EVERY exit path (completion, pause, cancel, and the `?`-error bubbles). Its typed
//!   [`MediaEnrichTerminalReason`] tells the frontend to clear the row (completion /
//!   cancel / failure) or re-voice it paused (the two pause reasons). Without a terminal
//!   on every exit the row would stick at "enriching" forever — the stuck-row bug
//!   `index-scan-aborted` fixed for drive scans.
//!
//! The [`EnrichTerminalGuard`] (RAII) guarantees the "on every exit path" property: it
//! defaults to [`MediaEnrichTerminalReason::Failed`] and emits on `Drop`, so a `?`-error
//! bubble still reports a terminal; the pass overrides the reason before a clean exit.
//! Registered in `ipc.rs`'s `collect_events!`; consumed via the typed `on*` wrappers in
//! `tauri-commands/indexing.ts`.

use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

use crate::ignore_poison::IgnorePoison;

use super::progress::{EnrichProgress, EnrichProgressSink, should_emit_progress};

/// The throttle cadence: emit at pass start, then at most every
/// [`MIN_INTERVAL_MS`] or every [`MIN_STEP`] processed images (and the caller adds a
/// final tick). Keeps emission off the per-image hot path bar a counter + time check.
const MIN_INTERVAL_MS: u64 = 500;
const MIN_STEP: u64 = 100;

/// Throttled progress for one volume's enrichment pass. `total` / `bytes_total` are the
/// ENRICHABLE-subset denominators (images passing the coverage gates), NEVER the full
/// walked set — a raw walked-set denominator rebuilds the never-finishes bug inside the
/// indicator. Wire name pinned (the `…Event` suffix wouldn't kebab-case to it).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "media-enrich-progress")]
#[serde(rename_all = "camelCase")]
pub struct MediaEnrichProgressEvent {
    pub volume_id: String,
    /// Subset images processed so far (enriched, already-current, or quietly skipped).
    pub done: u64,
    /// Total images in the enrichable subset (the honest denominator).
    pub total: u64,
    /// Bytes processed so far.
    pub bytes_done: u64,
    /// Total bytes across the enrichable subset.
    pub bytes_total: u64,
}

/// Why a volume's enrichment pass ended. A typed discriminant, never a string
/// (`no-string-matching`): the frontend clears the indicator row on `Completed` /
/// `Cancelled` / `Failed` and re-voices it paused on the two pause reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum MediaEnrichTerminalReason {
    /// The pass enriched every eligible image and GC'd vanished rows.
    Completed { enriched: u64, gc_count: u64 },
    /// A network pass paused because the app is in use (resumes when idle again).
    PausedWaitingForIdle,
    /// A network pass paused because the volume disconnected (resumes on reconnect).
    PausedDisconnected,
    /// The memory watchdog stopped the pass (resumes on the next scan / re-enable).
    Cancelled,
    /// The pass bubbled an error (e.g. a writer-send failure). The row must still clear.
    Failed,
}

/// A pass ended. EVERY pass exit emits exactly one (see the module docs), so the
/// indicator row never sticks at "enriching". Wire name pinned.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "media-enrich-terminal")]
#[serde(rename_all = "camelCase")]
pub struct MediaEnrichTerminalEvent {
    pub volume_id: String,
    pub reason: MediaEnrichTerminalReason,
}

// ── The production progress sink (throttled Tauri emitter) ──────────────────

/// The mutable throttle state behind the `&self` [`EnrichProgressSink`] contract.
#[derive(Default)]
struct ThrottleState {
    last_emit_ms: Option<u64>,
    last_done: u64,
}

/// The production progress sink: throttles per the [`should_emit_progress`] cadence and
/// emits [`MediaEnrichProgressEvent`]. An empty enrichable subset (`total == 0`) shows
/// no row at all. Constructed per pass; the enrich core calls [`report`](Self::report)
/// once at start (`done == 0`) and once per processed image.
pub(crate) struct TauriEnrichEmitter {
    app: AppHandle,
    volume_id: String,
    clock: Instant,
    state: Mutex<ThrottleState>,
}

impl TauriEnrichEmitter {
    /// A sink emitting for `volume_id` over `app`, its clock starting now.
    pub(crate) fn new(app: AppHandle, volume_id: String) -> Self {
        Self {
            app,
            volume_id,
            clock: Instant::now(),
            state: Mutex::new(ThrottleState::default()),
        }
    }
}

impl EnrichProgressSink for TauriEnrichEmitter {
    fn report(&self, progress: EnrichProgress) {
        // No enrichable images ⇒ no row (an empty pass never lights the indicator).
        if progress.total == 0 {
            return;
        }
        let now_ms = self.clock.elapsed().as_millis() as u64;
        let mut state = self.state.lock_ignore_poison();
        if !should_emit_progress(
            state.last_emit_ms,
            state.last_done,
            now_ms,
            progress.done,
            MIN_INTERVAL_MS,
            MIN_STEP,
        ) {
            return;
        }
        state.last_emit_ms = Some(now_ms);
        state.last_done = progress.done;
        drop(state);
        let _ = MediaEnrichProgressEvent {
            volume_id: self.volume_id.clone(),
            done: progress.done,
            total: progress.total,
            bytes_done: progress.bytes_done,
            bytes_total: progress.bytes_total,
        }
        .emit(&self.app);
    }
}

// ── The terminal guard (RAII, on-every-exit-path) ───────────────────────────

/// The boxed terminal-emit action the guard fires on drop. Production captures the
/// `AppHandle`; a test captures a recorder, so the "every exit emits a terminal" and
/// "an error path emits `Failed`" contracts are unit-testable without an app.
type TerminalEmit = Box<dyn FnOnce(MediaEnrichTerminalReason) + Send>;

/// Emits a [`MediaEnrichTerminalEvent`] on drop, so EVERY pass exit path (a clean
/// return, a pause, a cancel, or a `?`-error bubble) reports exactly one terminal event.
/// Defaults to [`MediaEnrichTerminalReason::Failed`]; the pass overrides the
/// reason via [`set`](Self::set) before a clean / paused / cancelled exit, so only a
/// genuinely error-bubbled exit keeps `Failed`.
pub(crate) struct EnrichTerminalGuard {
    emit: Option<TerminalEmit>,
    reason: MediaEnrichTerminalReason,
}

impl EnrichTerminalGuard {
    /// A guard that fires `emit` on drop with whatever reason is current then.
    pub(crate) fn new(emit: TerminalEmit) -> Self {
        Self {
            emit: Some(emit),
            reason: MediaEnrichTerminalReason::Failed,
        }
    }

    /// A guard that emits `media-enrich-terminal` for `volume_id` over `app`.
    pub(crate) fn for_app(app: AppHandle, volume_id: String) -> Self {
        Self::new(Box::new(move |reason| {
            let _ = MediaEnrichTerminalEvent { volume_id, reason }.emit(&app);
        }))
    }

    /// A guard that emits nothing (the scheduler has no app — unit tests).
    pub(crate) fn disabled() -> Self {
        Self {
            emit: None,
            reason: MediaEnrichTerminalReason::Failed,
        }
    }

    /// Override the reason the guard will emit on drop.
    pub(crate) fn set(&mut self, reason: MediaEnrichTerminalReason) {
        self.reason = reason;
    }
}

impl Drop for EnrichTerminalGuard {
    fn drop(&mut self) {
        if let Some(emit) = self.emit.take() {
            emit(self.reason.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// A recorder for the guard's on-drop reason.
    fn recording_guard(sink: Arc<Mutex<Option<MediaEnrichTerminalReason>>>) -> EnrichTerminalGuard {
        EnrichTerminalGuard::new(Box::new(move |reason| {
            *sink.lock_ignore_poison() = Some(reason);
        }))
    }

    #[test]
    fn an_error_exit_emits_a_failed_terminal() {
        // The guard is created, the reason is NEVER set (a `?`-error bubble), and it
        // drops. It must still emit a terminal — and `Failed` — so the row clears.
        let sink = Arc::new(Mutex::new(None));
        {
            let _guard = recording_guard(Arc::clone(&sink));
            // ... an error bubbles out here, so `set` is never reached ...
        }
        assert_eq!(*sink.lock_ignore_poison(), Some(MediaEnrichTerminalReason::Failed));
    }

    #[test]
    fn a_clean_exit_emits_the_set_reason() {
        let sink = Arc::new(Mutex::new(None));
        {
            let mut guard = recording_guard(Arc::clone(&sink));
            guard.set(MediaEnrichTerminalReason::Completed {
                enriched: 12,
                gc_count: 3,
            });
        }
        assert_eq!(
            *sink.lock_ignore_poison(),
            Some(MediaEnrichTerminalReason::Completed {
                enriched: 12,
                gc_count: 3
            })
        );
    }

    #[test]
    fn a_disabled_guard_emits_nothing() {
        // No panic, no emit: the scheduler builds this when it has no app (unit tests).
        let mut guard = EnrichTerminalGuard::disabled();
        guard.set(MediaEnrichTerminalReason::Cancelled);
        drop(guard);
    }
}
