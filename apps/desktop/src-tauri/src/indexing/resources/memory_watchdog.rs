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
//! **The snapshot reads BOTH allocators, and says which one holds the bytes.**
//! mimalloc (our global allocator, so the whole Rust heap) is invisible to the
//! macOS malloc-zone APIs, so a zone-only reading under-reports the heap the
//! watchdog polices by orders of magnitude. `crate::process_memory` owns that
//! gotcha and the readers; `MemoryAttribution` here turns the numbers into the
//! log's verdict, so the claim can never contradict the figures beside it.
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
use crate::indexing::lifecycle::state;
#[cfg(target_os = "macos")]
use crate::pluralize::grouped;

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
pub fn start(app: tauri::AppHandle) {
    // Idempotent: only the first caller spawns the single global watchdog.
    if WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(run_watchdog(app));
}

#[cfg(not(target_os = "macos"))]
pub fn start(_app: tauri::AppHandle) {
    // No-op on non-macOS platforms
}

#[cfg(target_os = "macos")]
async fn run_watchdog(app: tauri::AppHandle) {
    use std::time::Duration;

    let mut interval = tokio::time::interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    let mut state = WatchdogState::default();

    loop {
        interval.tick().await;

        // Per-tick check is cheap: one `task_info` call for `phys_footprint`.
        // The full breakdown is gathered only when a threshold actually trips.
        let phys_footprint = match crate::process_memory::current_phys_footprint() {
            Some(b) => b,
            None => continue,
        };

        match state.decide(phys_footprint) {
            WatchdogAction::Nothing => {}
            WatchdogAction::Warn => on_warn(phys_footprint),
            WatchdogAction::Stop => on_stop(&app, phys_footprint),
            WatchdogAction::Escalate {
                escalations,
                growth_since_stop,
            } => on_escalate(&app, phys_footprint, escalations, growth_since_stop),
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
fn on_stop(app: &tauri::AppHandle, phys_footprint: u64) {
    let snapshot = MemorySnapshot::capture();

    // Drives a user-visible toast; exactly the kind of error we want to ship
    // diagnostic context for when the user has opted in.
    crate::log_error!(
        "Memory watchdog: phys_footprint {:.2} GB exceeded the {} GB safety limit. \
         Stopping all indexing to prevent a system crash.\n{}",
        gb(phys_footprint),
        STOP_THRESHOLD / (1024 * 1024 * 1024),
        snapshot.as_ref().map(MemorySnapshot::report).unwrap_or_default(),
    );

    // Emit user-visible event, carrying the discriminating figures (not just RSS)
    // so a shipped error report tells the real story.
    use tauri_specta::Event;
    let _ = MemorySnapshot::memory_warning_event(
        snapshot.as_ref(),
        phys_footprint,
        crate::indexing::MemoryWatchdogAction::StoppedIndexing,
    )
    .emit(app);

    // Global budget: stop EVERY registered volume's index, not just `root`. Scans
    // run in parallel (the wire, not RAM, is the bottleneck), so the safety net is
    // one process-wide stop rather than per-volume serialization.
    state::stop_all_indexing();
}

/// Memory kept climbing after the stop. Whatever is growing isn't (only)
/// indexing, so say so loudly and re-run the stop in case something restarted.
#[cfg(target_os = "macos")]
fn on_escalate(app: &tauri::AppHandle, phys_footprint: u64, escalations: u32, growth_since_stop: u64) {
    let snapshot = MemorySnapshot::capture();

    crate::log_error!(
        "Memory watchdog: phys_footprint is STILL climbing {:.2} GB after all indexing was stopped \
         (now {:.2} GB, escalation #{}). The stop didn't hold, so the growth is not (only) the index scan.\n{}",
        gb(growth_since_stop),
        gb(phys_footprint),
        escalations,
        snapshot.as_ref().map(MemorySnapshot::report).unwrap_or_default(),
    );

    use tauri_specta::Event;
    let _ = MemorySnapshot::memory_warning_event(
        snapshot.as_ref(),
        phys_footprint,
        crate::indexing::MemoryWatchdogAction::StillGrowingAfterStop,
    )
    .emit(app);

    // Cheap and idempotent: a volume may have been registered again since the
    // stop, and the subsystem hooks only flip atomics.
    state::stop_all_indexing();
}

/// Bytes as gibibytes, for log formatting.
#[cfg(target_os = "macos")]
fn gb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

/// Bytes as mebibytes, for log formatting.
#[cfg(target_os = "macos")]
fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

// ── Memory snapshot ──────────────────────────────────────────────────

/// Which accountant explains the bulk of `phys_footprint`. Derived purely from
/// the numbers in a [`MemorySnapshot`], so the log's claim can't drift from the
/// figures printed next to it.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryAttribution {
    /// mimalloc, our global allocator: every Rust allocation, indexing included.
    RustHeap,
    /// The system malloc zones: WebKit, Objective-C, C libraries.
    SystemMalloc,
    /// Neither allocator claims it: graphics surfaces, mapped files, stacks.
    Unattributed,
    /// No single source holds a majority.
    Mixed,
}

#[cfg(target_os = "macos")]
impl MemoryAttribution {
    /// Classify a footprint by its two allocator readings. `rust_heap` and
    /// `system_malloc` are disjoint (see `crate::process_memory`), so whatever
    /// they don't cover is unattributed.
    fn classify(phys_footprint: u64, rust_heap: u64, system_malloc: u64) -> MemoryAttribution {
        let untracked = untracked_bytes(phys_footprint, rust_heap, system_malloc);
        let majority = phys_footprint / 2;
        if phys_footprint == 0 {
            return MemoryAttribution::Mixed;
        }
        if rust_heap >= majority && rust_heap >= system_malloc && rust_heap >= untracked {
            MemoryAttribution::RustHeap
        } else if system_malloc >= majority && system_malloc >= untracked {
            MemoryAttribution::SystemMalloc
        } else if untracked >= majority {
            MemoryAttribution::Unattributed
        } else {
            MemoryAttribution::Mixed
        }
    }

    /// The one-line verdict for the log.
    fn explanation(self) -> &'static str {
        match self {
            MemoryAttribution::RustHeap => {
                "the Rust heap (mimalloc) holds most of it, so this IS backend memory: indexing, media, or another Rust subsystem"
            }
            MemoryAttribution::SystemMalloc => {
                "the system malloc zones hold most of it, so this is WebKit / Objective-C, not the Rust backend"
            }
            MemoryAttribution::Unattributed => {
                "neither allocator claims most of it: look at graphics surfaces, mapped files, and thread stacks"
            }
            MemoryAttribution::Mixed => "no single source holds a majority; read the lines above",
        }
    }
}

/// `phys_footprint` minus what both allocators account for. Saturating: mimalloc
/// can hold committed pages the footprint no longer counts, so the allocators
/// can sum past `phys_footprint`.
#[cfg(target_os = "macos")]
fn untracked_bytes(phys_footprint: u64, rust_heap: u64, system_malloc: u64) -> u64 {
    phys_footprint.saturating_sub(rust_heap.saturating_add(system_malloc))
}

/// A full memory breakdown, gathered when a threshold trips. The point is to
/// name where the bytes actually are: our Rust heap, the system allocator, or
/// neither.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct MemorySnapshot {
    /// The machine-pressure metric the thresholds key on (what Activity Monitor
    /// shows, what jetsam watches).
    phys_footprint: u64,
    /// Peak `phys_footprint` over the process lifetime, if the running kernel
    /// reports it (`ledger_phys_footprint_peak`).
    phys_footprint_peak: Option<u64>,
    /// Resident set size (RSS). Counts graphics and shared mappings that
    /// `phys_footprint` excludes.
    resident_size: u64,
    /// High-water mark of RSS.
    resident_size_max: u64,
    /// Bytes mimalloc has committed: the Rust heap, where indexing lives.
    rust_heap: u64,
    /// High-water mark of the above.
    rust_heap_peak: u64,
    /// Bytes the SYSTEM malloc zones hold. Disjoint from `rust_heap`.
    system_malloc_in_use: u64,
    /// Bytes those zones reserved from the OS (in use + free).
    system_malloc_reserved: u64,
    /// Number of system malloc zones.
    zone_count: u32,
    /// The largest system zone by in-use bytes: `(name, in_use)`.
    largest_zone: Option<(String, u64)>,
    /// Live FSEvents processed so far (a cheap indexing-internal pressure
    /// signal already tracked in this module's `super`).
    // TODO: also surface writer-channel depth and reconciler `pending_events`
    // len here once they're exposed as atomics — both are real indexing-memory
    // signals but neither is reachable from the watchdog today without new
    // plumbing.
    live_event_count: u64,
}

#[cfg(target_os = "macos")]
impl MemorySnapshot {
    /// Gather the full breakdown. Returns `None` only if the load-bearing
    /// `phys_footprint` query fails; everything else degrades gracefully.
    fn capture() -> Option<MemorySnapshot> {
        let vm = crate::process_memory::query_task_vm_info()?;
        let basic = crate::process_memory::query_basic_info();
        let rust_heap = crate::process_memory::query_mimalloc_heap();
        let zones = crate::process_memory::query_system_malloc_zones();

        Some(MemorySnapshot {
            phys_footprint: vm.phys_footprint,
            phys_footprint_peak: vm.phys_footprint_peak,
            resident_size: basic.as_ref().map_or(vm.resident_size, |b| b.resident_size),
            resident_size_max: basic.as_ref().map_or(0, |b| b.resident_size_max),
            rust_heap: rust_heap.committed,
            rust_heap_peak: rust_heap.peak_committed,
            system_malloc_in_use: zones.in_use,
            system_malloc_reserved: zones.reserved,
            zone_count: zones.zone_count,
            largest_zone: zones.largest_zone,
            live_event_count: crate::indexing::DEBUG_STATS.live_event_count.load(Ordering::Relaxed),
        })
    }

    /// `phys_footprint` neither allocator accounts for.
    fn untracked(&self) -> u64 {
        untracked_bytes(self.phys_footprint, self.rust_heap, self.system_malloc_in_use)
    }

    /// Where the bulk of the footprint actually is.
    fn attribution(&self) -> MemoryAttribution {
        MemoryAttribution::classify(self.phys_footprint, self.rust_heap, self.system_malloc_in_use)
    }

    /// A multi-line breakdown for the log. Deliberately verbose: this fires
    /// rarely, and when it does we want a real head start on diagnosis. Every
    /// line says what its number MEANS, because the previous version's unlabeled
    /// figures got read as the opposite of what they were.
    fn report(&self) -> String {
        let peak = match self.phys_footprint_peak {
            Some(p) => format!(", peak {:.2} GB", gb(p)),
            None => String::new(),
        };
        let largest = match &self.largest_zone {
            Some((name, bytes)) => format!(" (largest: {} {:.0} MB)", name, mb(*bytes)),
            None => String::new(),
        };
        format!(
            "  phys_footprint:  {:.2} GB{} — the metric macOS keys memory pressure and jetsam on (Activity Monitor's \"Memory\"); the thresholds key on this\n\
             \x20 resident_size:   {:.2} GB (max {:.2} GB) — RSS; counts graphics and shared mappings phys_footprint excludes\n\
             \x20 Rust heap:       {:.0} MB committed (peak {:.0} MB) — mimalloc, OUR global allocator: all Rust allocation, indexing included\n\
             \x20 system malloc:   {:.0} MB in use, {:.0} MB reserved across {} zone(s){} — WebKit / Objective-C / C only; blind to the Rust heap above\n\
             \x20 untracked:       {:.0} MB — phys_footprint minus both allocators: graphics surfaces, mapped files, thread stacks\n\
             \x20 verdict:         {}\n\
             \x20 live FSEvents:   {} processed\n\
             \x20 Reading vmmap next? mimalloc tags its arenas with VM tag 100, which macOS names VM_MEMORY_IOACCELERATOR, so `IOAccelerator` rows ARE this Rust heap, not GPU memory.",
            gb(self.phys_footprint),
            peak,
            gb(self.resident_size),
            gb(self.resident_size_max),
            mb(self.rust_heap),
            mb(self.rust_heap_peak),
            mb(self.system_malloc_in_use),
            mb(self.system_malloc_reserved),
            self.zone_count,
            largest,
            mb(self.untracked()),
            self.attribution().explanation(),
            grouped(self.live_event_count),
        )
    }

    /// Build the frontend event. Falls back to whatever the caller already knows
    /// (`phys_footprint`) if the full snapshot couldn't be gathered.
    fn memory_warning_event(
        snapshot: Option<&MemorySnapshot>,
        phys_footprint: u64,
        action: crate::indexing::MemoryWatchdogAction,
    ) -> crate::indexing::IndexMemoryWarningEvent {
        crate::indexing::IndexMemoryWarningEvent {
            phys_footprint_bytes: phys_footprint,
            resident_bytes: snapshot.map_or(phys_footprint, |s| s.resident_size),
            rust_heap_bytes: snapshot.map_or(0, |s| s.rust_heap),
            system_malloc_bytes: snapshot.map_or(0, |s| s.system_malloc_in_use),
            untracked_bytes: snapshot.map_or(0, MemorySnapshot::untracked),
            action,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn snapshot_captures_and_reports_key_fields() {
        let snapshot = MemorySnapshot::capture().expect("snapshot should capture on macOS");
        assert!(snapshot.phys_footprint > 0, "phys_footprint should be positive");
        assert!(snapshot.resident_size > 0, "resident_size should be positive");
        assert!(snapshot.rust_heap > 0, "the Rust heap should be positive");
        assert!(
            snapshot.system_malloc_in_use > 0,
            "the system malloc zones should be positive"
        );

        let report = snapshot.report();
        for needle in [
            "phys_footprint",
            "resident_size",
            "Rust heap",
            "system malloc",
            "untracked",
            "verdict",
            "live FSEvents",
        ] {
            assert!(
                report.contains(needle),
                "report should mention {needle}; got:\n{report}"
            );
        }
    }

    // ── Attribution ──────────────────────────────────────────────────

    /// The 2026-07 runaway, as the watchdog would have seen it: a 16.5 GB
    /// footprint that was almost entirely the Rust heap, with `resident` equal
    /// to `phys_footprint` (so a zero graphics delta).
    #[cfg(target_os = "macos")]
    fn incident_snapshot() -> MemorySnapshot {
        MemorySnapshot {
            phys_footprint: 16 * GB + GB / 2,
            phys_footprint_peak: Some(16 * GB + GB / 2),
            resident_size: 16 * GB + GB / 2,
            resident_size_max: 16 * GB + GB / 2,
            rust_heap: 15 * GB,
            rust_heap_peak: 15 * GB,
            system_malloc_in_use: GB + GB / 2,
            system_malloc_reserved: 2 * GB,
            zone_count: 4,
            largest_zone: Some(("DefaultMallocZone".to_string(), GB)),
            live_event_count: 1_234_567,
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_runaway_that_was_the_rust_heap_is_attributed_to_the_rust_heap() {
        // Pre-fix, this exact shape was logged as "likely WebView/GPU memory
        // (IOAccelerator), NOT the indexing heap" — off a resident−phys delta
        // that was 0.00 GB. Three investigations chased the frontend for it.
        let snapshot = incident_snapshot();
        assert_eq!(snapshot.attribution(), MemoryAttribution::RustHeap);
        assert_eq!(
            snapshot.resident_size, snapshot.phys_footprint,
            "the incident had no graphics delta at all"
        );

        let report = snapshot.report();
        assert!(
            !report.contains("NOT the indexing heap"),
            "the report must not deny the heap it just measured; got:\n{report}"
        );
        assert!(
            report.contains("IS backend memory"),
            "the verdict should name the Rust heap; got:\n{report}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_webkit_heavy_footprint_is_attributed_to_system_malloc() {
        assert_eq!(
            MemoryAttribution::classify(5 * GB, GB / 4, 4 * GB),
            MemoryAttribution::SystemMalloc
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn memory_neither_allocator_claims_is_unattributed() {
        // Real graphics/mapped-file territory: both allocators are small.
        assert_eq!(
            MemoryAttribution::classify(8 * GB, GB / 2, GB / 2),
            MemoryAttribution::Unattributed
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_footprint_with_no_majority_holder_is_mixed() {
        // 4 + 3.5 + 2.5 of 10: nobody owns half, so don't pretend to know.
        assert_eq!(
            MemoryAttribution::classify(10 * GB, 4 * GB, 3 * GB + GB / 2),
            MemoryAttribution::Mixed
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn untracked_bytes_never_underflow_when_the_allocators_overshoot() {
        // mimalloc can hold committed pages phys_footprint no longer counts.
        assert_eq!(untracked_bytes(4 * GB, 5 * GB, GB), 0);
        assert_eq!(untracked_bytes(0, 0, 0), 0);
        assert_eq!(MemoryAttribution::classify(0, 0, 0), MemoryAttribution::Mixed);
    }

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
