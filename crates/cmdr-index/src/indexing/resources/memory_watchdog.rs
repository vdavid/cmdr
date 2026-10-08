//! Memory watchdog: monitors the app's memory and takes action at safety
//! thresholds to prevent unbounded memory growth.
//!
//! - 8 GB: logs a warning with a full memory breakdown.
//! - 16 GB: stops EVERY volume's index, emits a user-visible event, and logs
//!   the same breakdown.
//! - After a stop: KEEPS WATCHING. If `phys_footprint` climbs another 2 GB (and
//!   then 4, 8, 16 — doubling, so a runaway gets a handful of proportionate
//!   alerts instead of one per tick) it escalates: the stop didn't hold, so
//!   whatever is growing isn't (only) the index scan. It re-arms once memory
//!   falls back under the warning line.
//!
//! **The threshold basis is `phys_footprint`, not `resident_size`.** On macOS,
//! RSS counts graphics and shared mappings that are NOT real memory pressure.
//! `phys_footprint` is the metric macOS itself keys memory pressure and jetsam
//! on, and it's what Activity Monitor's "Memory" column shows. So the per-tick
//! check reads `phys_footprint` (one cheap `task_info` call); when a threshold
//! trips, a full `MemorySnapshot` goes into the log so a rare event carries real
//! diagnostic context, not a bare number.
//!
//! **What a trip logs** (which allocator holds the bytes, and a verdict derived
//! from the same numbers) lives in `memory_snapshot.rs`.
//!
//! **The budget is GLOBAL, not per-volume** (plan rabbit hole #8, resolved by
//! David). Scans run in parallel — the network/USB wire is the bottleneck, not
//! RAM — so there's no one-at-a-time serialization; instead a single process-
//! wide budget is the safety net that stops ALL indexing if total memory
//! crosses the catastrophe line. The 16 GB number is a machine-protection stop,
//! NOT expected usage (real scan memory is the accumulator maps + the 20K
//! writer channel — hundreds of MB per normal volume).
//!
//! On non-macOS platforms this is a no-op stub (platform memory queries
//! differ and can be added later).

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
use super::memory_snapshot::{MemorySnapshot, gb};
#[cfg(target_os = "macos")]
use crate::indexing::lifecycle::state;

/// 8 GB in bytes.
#[cfg(target_os = "macos")]
const WARN_THRESHOLD: u64 = 8 * 1024 * 1024 * 1024;

/// 16 GB in bytes.
#[cfg(target_os = "macos")]
const STOP_THRESHOLD: u64 = 16 * 1024 * 1024 * 1024;

/// How often the watchdog checks memory (seconds).
#[cfg(target_os = "macos")]
const CHECK_INTERVAL_SECS: u64 = 5;

/// How much further `phys_footprint` must climb after a stop before the
/// watchdog shouts again. Doubles per escalation (see [`PostStop::next_step`]).
#[cfg(target_os = "macos")]
const FIRST_ESCALATION_STEP: u64 = 2 * 1024 * 1024 * 1024;

// ── Decision logic (pure) ────────────────────────────────────────────

/// What the watchdog decided to do on one tick. Pure output of
/// [`WatchdogState::decide`], so the policy is unit-testable without touching
/// Mach, the registry, or the app handle.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchdogAction {
    /// Nothing to say this tick.
    Nothing,
    /// Crossed the warning line for the first time since it was last below it.
    Warn,
    /// Crossed the stop line: stop all indexing.
    Stop,
    /// Memory kept climbing AFTER a stop. The stop didn't hold, so whatever is
    /// growing isn't (only) indexing.
    Escalate {
        /// How many escalations since the stop, 1-based.
        escalations: u32,
        /// How far `phys_footprint` has climbed past its level at the stop.
        growth_since_stop: u64,
    },
    /// Fell back under the warning line after a stop: re-armed.
    Recovered,
}

/// Book-keeping after a stop fired, so the watchdog can tell "the stop worked"
/// from "memory is still running away".
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PostStop {
    /// `phys_footprint` at the moment of the stop.
    at_stop: u64,
    /// `phys_footprint` at the last thing we logged (the stop, or an escalation).
    last_alert: u64,
    /// How much further it must climb before the next escalation. Doubles each
    /// time, so a runaway gets a handful of proportionate alerts rather than one
    /// per tick for as long as it grows.
    next_step: u64,
    /// Escalations logged since the stop.
    escalations: u32,
}

/// The watchdog's memory of what it has already reacted to.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct WatchdogState {
    /// Whether the warning line has already been reported at its current crossing.
    warned: bool,
    /// Set once the stop fires; cleared when memory falls back under the warning
    /// line. While set, the watchdog is in escalation mode.
    stopped: Option<PostStop>,
}

#[cfg(target_os = "macos")]
impl WatchdogState {
    /// Decide what this tick's `phys_footprint` reading calls for, updating the
    /// state. Pure: no I/O, no clock, no globals.
    fn decide(&mut self, phys_footprint: u64) -> WatchdogAction {
        // Already stopped: we're in escalation mode until memory comes back down.
        // The stop is NOT the end of the watch (that one-shot behavior is what let
        // a 16 GB incident climb to 40 GB unobserved).
        if let Some(post) = self.stopped.as_mut() {
            if phys_footprint < WARN_THRESHOLD {
                self.stopped = None;
                self.warned = false;
                return WatchdogAction::Recovered;
            }
            if phys_footprint >= post.last_alert.saturating_add(post.next_step) {
                post.last_alert = phys_footprint;
                post.next_step = post.next_step.saturating_mul(2);
                post.escalations += 1;
                return WatchdogAction::Escalate {
                    escalations: post.escalations,
                    growth_since_stop: phys_footprint.saturating_sub(post.at_stop),
                };
            }
            return WatchdogAction::Nothing;
        }

        if phys_footprint >= STOP_THRESHOLD {
            self.warned = true;
            self.stopped = Some(PostStop {
                at_stop: phys_footprint,
                last_alert: phys_footprint,
                next_step: FIRST_ESCALATION_STEP,
                escalations: 0,
            });
            return WatchdogAction::Stop;
        }

        if phys_footprint >= WARN_THRESHOLD {
            if self.warned {
                return WatchdogAction::Nothing;
            }
            self.warned = true;
            return WatchdogAction::Warn;
        }

        self.warned = false;
        WatchdogAction::Nothing
    }
}

/// Whether the single global watchdog task is already running. The watchdog is
/// process-wide (one global budget over all volumes), so the first `start()`
/// wins and later per-volume `start_indexing_for` calls are no-ops — without
/// this, every volume start would spawn a redundant watchdog loop all racing to
/// stop indexing. Never cleared: the loop runs for the process lifetime.
#[cfg(target_os = "macos")]
static WATCHDOG_RUNNING: AtomicBool = AtomicBool::new(false);

/// Start the global memory watchdog as a fire-and-forget background task.
///
/// On macOS, spawns ONE task (idempotent across volumes) that checks
/// `phys_footprint` every 5 seconds via `task_info`, for the whole process
/// lifetime. On other platforms, no-op.
#[cfg(target_os = "macos")]
pub fn start(events: std::sync::Arc<dyn crate::EventSink>) {
    // Idempotent: only the first caller spawns the single global watchdog.
    if WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::indexing::host::runtime::spawn(run_watchdog(events));
}

#[cfg(not(target_os = "macos"))]
pub fn start(_events: std::sync::Arc<dyn crate::EventSink>) {
    // No-op on non-macOS platforms
}

#[cfg(target_os = "macos")]
async fn run_watchdog(events: std::sync::Arc<dyn crate::EventSink>) {
    use std::time::Duration;

    let mut interval = tokio::time::interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    let mut state = WatchdogState::default();

    loop {
        interval.tick().await;

        // Per-tick check is cheap: one `task_info` call for `phys_footprint`.
        // The full breakdown is gathered only when a threshold actually trips.
        let phys_footprint = match cmdr_fs::process_memory::current_phys_footprint() {
            Some(b) => b,
            None => continue,
        };

        match state.decide(phys_footprint) {
            WatchdogAction::Nothing => {}
            WatchdogAction::Warn => on_warn(phys_footprint),
            WatchdogAction::Stop => on_stop(events.as_ref(), phys_footprint),
            WatchdogAction::Escalate {
                escalations,
                growth_since_stop,
            } => on_escalate(events.as_ref(), phys_footprint, escalations, growth_since_stop),
            WatchdogAction::Recovered => {
                log::info!(
                    "Memory watchdog: phys_footprint fell back to {:.2} GB, under the {} GB warning line. \
                     Re-armed; indexing can be started again.",
                    gb(phys_footprint),
                    WARN_THRESHOLD / (1024 * 1024 * 1024),
                );
            }
        }
    }
}

/// Crossed the warning line: log the breakdown, keep indexing.
#[cfg(target_os = "macos")]
fn on_warn(phys_footprint: u64) {
    let snapshot = MemorySnapshot::capture();
    log::warn!(
        "Memory watchdog: phys_footprint {:.2} GB crossed the {} GB warning threshold. \
         Indexing continues but the system may be under memory pressure.\n{}",
        gb(phys_footprint),
        WARN_THRESHOLD / (1024 * 1024 * 1024),
        snapshot.as_ref().map(MemorySnapshot::report).unwrap_or_default(),
    );
}

/// Crossed the stop line: stop every volume's index and tell the user.
#[cfg(target_os = "macos")]
fn on_stop(events: &dyn crate::EventSink, phys_footprint: u64) {
    let snapshot = MemorySnapshot::capture();

    // Worth an error report, with the breakdown attached: exactly the kind of
    // failure we want diagnostic context for when the user has opted in.
    events.emit(crate::IndexEvent::Error {
        report: crate::IndexErrorReport::MemoryWatchdog {
            action: crate::MemoryWatchdogAction::StoppedIndexing,
            phys_footprint_bytes: phys_footprint,
            limit_bytes: STOP_THRESHOLD,
            growth_since_stop_bytes: None,
            escalation: None,
            snapshot: snapshot.as_ref().map(|s| crate::Diagnostic(s.report())),
        },
    });

    // Report the user-visible warning, carrying the discriminating figures (not
    // just RSS) so a shipped error report tells the real story.
    events.emit(MemorySnapshot::memory_warning_event(
        snapshot.as_ref(),
        phys_footprint,
        crate::MemoryWatchdogAction::StoppedIndexing,
    ));

    // Global budget: stop EVERY registered volume's index, not just `root`. Scans
    // run in parallel (the wire, not RAM, is the bottleneck), so the safety net is
    // one process-wide stop rather than per-volume serialization.
    state::stop_all_indexing();
}

/// Memory kept climbing after the stop. Whatever is growing isn't (only)
/// indexing, so say so loudly and re-run the stop in case something restarted.
#[cfg(target_os = "macos")]
fn on_escalate(events: &dyn crate::EventSink, phys_footprint: u64, escalations: u32, growth_since_stop: u64) {
    let snapshot = MemorySnapshot::capture();

    events.emit(crate::IndexEvent::Error {
        report: crate::IndexErrorReport::MemoryWatchdog {
            action: crate::MemoryWatchdogAction::StillGrowingAfterStop,
            phys_footprint_bytes: phys_footprint,
            limit_bytes: STOP_THRESHOLD,
            growth_since_stop_bytes: Some(growth_since_stop),
            escalation: Some(escalations),
            snapshot: snapshot.as_ref().map(|s| crate::Diagnostic(s.report())),
        },
    });

    events.emit(MemorySnapshot::memory_warning_event(
        snapshot.as_ref(),
        phys_footprint,
        crate::MemoryWatchdogAction::StillGrowingAfterStop,
    ));

    // Cheap and idempotent: a volume may have been registered again since the
    // stop, and the subsystem hooks only flip atomics.
    state::stop_all_indexing();
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::*;

    // ── Decision logic ───────────────────────────────────────────────

    #[cfg(target_os = "macos")]
    const GB: u64 = 1024 * 1024 * 1024;

    #[cfg(target_os = "macos")]
    #[test]
    fn quiet_below_the_warning_line() {
        let mut state = WatchdogState::default();
        assert_eq!(state.decide(2 * GB), WatchdogAction::Nothing);
        assert_eq!(state.decide(7 * GB), WatchdogAction::Nothing);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn warns_once_per_crossing_of_the_warning_line() {
        let mut state = WatchdogState::default();
        assert_eq!(state.decide(9 * GB), WatchdogAction::Warn);
        assert_eq!(state.decide(9 * GB), WatchdogAction::Nothing, "should not re-warn");
        assert_eq!(state.decide(10 * GB), WatchdogAction::Nothing, "should not re-warn");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dropping_back_under_the_warning_line_rearms_the_warning() {
        let mut state = WatchdogState::default();
        assert_eq!(state.decide(9 * GB), WatchdogAction::Warn);
        assert_eq!(state.decide(3 * GB), WatchdogAction::Nothing);
        assert_eq!(state.decide(9 * GB), WatchdogAction::Warn, "should warn again");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn crossing_the_stop_line_stops_indexing_once() {
        let mut state = WatchdogState::default();
        assert_eq!(state.decide(17 * GB), WatchdogAction::Stop);
        assert_eq!(
            state.decide(17 * GB),
            WatchdogAction::Nothing,
            "a flat reading after the stop shouldn't re-stop every tick"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keeps_watching_after_a_stop_and_escalates_when_memory_keeps_climbing() {
        let mut state = WatchdogState::default();
        assert_eq!(state.decide(16 * GB), WatchdogAction::Stop);
        assert_eq!(state.decide(17 * GB), WatchdogAction::Nothing, "under the 2 GB step");
        assert_eq!(
            state.decide(18 * GB),
            WatchdogAction::Escalate {
                escalations: 1,
                growth_since_stop: 2 * GB,
            }
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn escalation_steps_double_so_a_runaway_does_not_spam_the_log() {
        let mut state = WatchdogState::default();
        assert_eq!(state.decide(16 * GB), WatchdogAction::Stop);
        assert_eq!(
            state.decide(18 * GB),
            WatchdogAction::Escalate {
                escalations: 1,
                growth_since_stop: 2 * GB
            }
        );
        assert_eq!(
            state.decide(20 * GB),
            WatchdogAction::Nothing,
            "next step is 4 GB, not 2"
        );
        assert_eq!(
            state.decide(22 * GB),
            WatchdogAction::Escalate {
                escalations: 2,
                growth_since_stop: 6 * GB
            }
        );
        assert_eq!(state.decide(28 * GB), WatchdogAction::Nothing, "next step is 8 GB");
        assert_eq!(
            state.decide(30 * GB),
            WatchdogAction::Escalate {
                escalations: 3,
                growth_since_stop: 14 * GB
            }
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recovering_under_the_warning_line_rearms_the_stop() {
        let mut state = WatchdogState::default();
        assert_eq!(state.decide(16 * GB), WatchdogAction::Stop);
        assert_eq!(state.decide(4 * GB), WatchdogAction::Recovered);
        assert_eq!(state.decide(4 * GB), WatchdogAction::Nothing);
        assert_eq!(
            state.decide(16 * GB),
            WatchdogAction::Stop,
            "a fresh runaway after recovery must stop again"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_stop_that_does_not_hold_stays_observed_all_the_way_up() {
        // Regression anchor for the 2026-07 incident: the watchdog stopped
        // indexing at 16 GB and then stopped watching, so the climb to 40 GB
        // went unobserved and the app had to be stopped by hand.
        let mut state = WatchdogState::default();
        assert_eq!(state.decide(16 * GB), WatchdogAction::Stop);

        let mut escalations = 0;
        let mut last_growth = 0;
        for gb in 17..=40 {
            match state.decide(gb * GB) {
                WatchdogAction::Escalate {
                    escalations: n,
                    growth_since_stop,
                } => {
                    escalations = n;
                    last_growth = growth_since_stop;
                }
                WatchdogAction::Nothing => {}
                other => panic!("unexpected action while climbing at {gb} GB: {other:?}"),
            }
        }
        assert!(
            escalations >= 3,
            "a 16→40 GB runaway should escalate several times, got {escalations}"
        );
        // Escalations land at 18, 22, and 30 GB (2 GB step, doubling), so the
        // last one reports 14 GB of growth past the stop.
        assert_eq!(
            last_growth,
            14 * GB,
            "each escalation should report the growth since the stop"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn thresholds_are_ordered() {
        const {
            assert!(
                WARN_THRESHOLD < STOP_THRESHOLD,
                "warn threshold must be below stop threshold"
            )
        };
    }
}
