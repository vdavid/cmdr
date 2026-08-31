//! ETA + throughput estimator for write operations.
//!
//! Tracks two independent rates (bytes/second and files/second) via a
//! time-weighted exponential moving average (τ ≈ 3 s half-life), then combines
//! them with `max(ETA_bytes, ETA_files)`. The operation can't finish before
//! either axis is done, so the larger remaining time is the truthful one.
//!
//! This shape matters because the two axes go out of sync in real workloads:
//! deleting 5 GB of mixed sizes finishes the byte work in the first second
//! (a few large files), then spends 20 s on per-file syscall overhead for the
//! tail of small files. A byte-only ETA shows ~0 s while files keep streaming.
//!
//! ## Adaptivity
//!
//! Pure EWMA, no "overall average" blend. If the network drops mid-operation,
//! the EWMA decays to the new rate within ~3τ (≈9 s) without being anchored
//! to historical numbers. The smoothing constant is computed time-weighted:
//! `α = 1 - exp(-Δt / τ)`, so the response is identical whether progress events
//! arrive every 50 ms or every 500 ms.
//!
//! ## The clock is WORKING time, not wall time
//!
//! Every interval is measured minus whatever of it a person spent deciding —
//! a user pause, or an open conflict prompt — read off the operation's
//! `super::human_wait::HumanWaitClock` and carried on each [`EtaSample`]. A
//! parked transfer emits no progress, so without this the first sample after a
//! five-minute answer divides one file's bytes by five minutes: the rate
//! collapses and the ETA jumps to hours, on a copy that is running fine.
//! Device waits are deliberately NOT excluded: a slow share IS the transfer
//! being slow, and the ETA has to say so.
//!
//! ## Phase transitions and rollback
//!
//! Resetting on phase change (scanning → copying, copying → rolling_back) is
//! required because the counters reset too. Otherwise an EWMA fed
//! `bytes_done = 0` after `bytes_done = 5_000_000_000` would emit garbage.
//!
//! **A reversal is told which way its counters run, ❌ never asked to infer it
//! from the phase.** Both reversals report `RollingBack` and they count in
//! OPPOSITE directions on purpose: an in-flight cancel drains the bar the user
//! watched fill, while a reversal started from the history dialog opens a fresh
//! bar and fills it. `ReversalBar` rides on every sample, and it decides both the
//! sign of a delta and which end of the phase the remaining work is measured to.
//! Reading a filling reversal as a draining one reports a finish it has barely
//! started, then grows the estimate as it progresses.

use std::time::{Duration, Instant};

use super::types::WriteOperationPhase;

/// Half-life-ish time constant for the EWMA. 3 s feels live but not jittery;
/// short enough that walking 20 m from the router visibly drops the speed
/// within a few seconds. Bumping this up smooths more but lags real changes.
const EWMA_TAU_SECS: f64 = 3.0;

/// Don't emit an ETA until we've seen at least this many samples in the current
/// phase. The first sample initializes the EWMA from the instantaneous rate, which
/// can be wild. Wait for one more to stabilize.
const MIN_SAMPLES_FOR_ETA: u32 = 2;

/// Don't emit an ETA until at least this much WORKING time has elapsed in the
/// current phase (wall time minus every human wait). Catches the "200 ms in,
/// rate is 50 MB/s" → "ETA = 0 s" footgun before the EWMA settles.
const MIN_ELAPSED_FOR_ETA: Duration = Duration::from_millis(800);

/// Which way a `RollingBack` phase's counters run. Set by whoever drives the
/// reversal, ❌ never inferred from the phase (see the module doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReversalBar {
    /// Counts UP toward the total: the history dialog's reversal, a fresh
    /// operation opening a fresh bar.
    Fills,
    /// Counts DOWN toward zero: an in-flight cancel draining the bar the user
    /// watched fill.
    Drains,
}

/// Computed rates + ETA emitted to the frontend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EtaStats {
    /// Smoothed bytes per second.
    pub bytes_per_second: u64,
    /// Smoothed files per second.
    pub files_per_second: f32,
    /// Seconds remaining. `None` while the estimator is warming up or when both
    /// rates are zero (operation stalled: no point lying about the ETA).
    pub eta_seconds: Option<u32>,
}

impl EtaStats {
    pub const ZERO: Self = Self {
        bytes_per_second: 0,
        files_per_second: 0.0,
        eta_seconds: None,
    };
}

/// One progress observation: the counters, plus the two clocks that turn a
/// change in them into a rate.
#[derive(Debug, Clone, Copy)]
pub struct EtaSample {
    /// Injected (not read from `Instant::now()` inside the estimator) so tests
    /// can drive synthetic timelines without touching the real clock.
    pub now: Instant,
    /// How long this operation has spent waiting on a PERSON, in total
    /// (`super::human_wait::HumanWaitClock`). Monotonic; the estimator
    /// subtracts what accrued since the last sample from the elapsed wall time.
    pub human_wait_total: Duration,
    pub phase: WriteOperationPhase,
    /// Which way this operation's `RollingBack` frames run. Meaningless in any
    /// other phase, and read only there.
    pub reversal_bar: ReversalBar,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: usize,
    pub files_total: usize,
}

/// State for one phase of one operation. Reset on phase transition.
#[derive(Debug)]
struct PhaseState {
    phase: WriteOperationPhase,
    /// `true` while the counters run DOWN toward zero: a cancel's reversal
    /// draining the bar the user watched fill. The direction decides both the
    /// sign of every delta and which end of the phase the remaining work is
    /// measured to, so it can't be inferred from the phase alone — a reversal
    /// started from the history dialog reports the same phase while counting UP.
    counters_drain: bool,
    last_t: Instant,
    /// The human-wait reading at `last_t`, so the next sample can tell working
    /// time from time somebody spent deciding.
    last_human_wait: Duration,
    /// Working time seen in this phase: wall time minus every human wait. What
    /// the warm-up gate measures, so a copy paused 10 s into its first 200 ms of
    /// work doesn't come back claiming to be warmed up.
    working_elapsed: Duration,
    last_bytes: u64,
    last_files: usize,
    /// EWMA in absolute units per second, toward the phase target (forward or rollback).
    bytes_rate: f64,
    files_rate: f64,
    samples: u32,
}

/// Per-operation estimator. Constructed once when the operation starts; updated
/// from each progress emission. `Default` is the only way to make one;
/// there's no useful state to seed.
// DEFAULT-OK: `None` phase state is an estimator that hasn't seen a progress emission,
// and it reports no ETA until it has.
#[derive(Debug, Default)]
pub struct EtaEstimator {
    state: Option<PhaseState>,
}

impl EtaEstimator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the estimator with the latest counters and return the current stats.
    pub fn update(&mut self, sample: EtaSample) -> EtaStats {
        let EtaSample {
            now,
            human_wait_total,
            phase,
            reversal_bar,
            bytes_done,
            bytes_total,
            files_done,
            files_total,
        } = sample;
        let counters_drain = phase == WriteOperationPhase::RollingBack && reversal_bar == ReversalBar::Drains;

        // On phase change (or first call), reseed and emit zero stats.
        // The next call's Δt will be measured against this seed.
        let needs_reset = match &self.state {
            None => true,
            Some(s) => s.phase != phase || s.counters_drain != counters_drain,
        };

        if needs_reset {
            self.state = Some(PhaseState {
                phase,
                counters_drain,
                last_t: now,
                last_human_wait: human_wait_total,
                working_elapsed: Duration::ZERO,
                last_bytes: bytes_done,
                last_files: files_done,
                bytes_rate: 0.0,
                files_rate: 0.0,
                samples: 0,
            });
            return EtaStats::ZERO;
        }

        let state = self.state.as_mut().expect("just reset or pre-existing");
        // The rate window measures WORKING time. Whatever of this interval a
        // person spent deciding (a pause, an open conflict prompt) is theirs,
        // and dividing one file's bytes by the five minutes a prompt sat open
        // is how a healthy copy came to report "0.4 files/s, 409h 39m left".
        let waited = human_wait_total.saturating_sub(state.last_human_wait);
        let dt = now
            .saturating_duration_since(state.last_t)
            .saturating_sub(waited)
            .as_secs_f64();
        if dt <= 0.0 {
            // Two updates in the same instant, or an interval that was entirely
            // somebody's to spend. Re-anchor on this sample — its counters are
            // the new baseline, and any bytes that moved alongside the wait
            // simply don't inform the rate — and report the rates unchanged, so
            // the ETA on screen stays where the last real measurement left it.
            state.last_t = now;
            state.last_human_wait = human_wait_total;
            state.last_bytes = bytes_done;
            state.last_files = files_done;
            return compute_stats(state, bytes_done, bytes_total, files_done, files_total);
        }

        // Δ toward the phase target. A filling bar grows the counters; a draining
        // one shrinks them. `saturating_sub` neutralizes spurious regressions (a
        // late event arriving after a counter reset, etc.).
        let (delta_bytes, delta_files) = if counters_drain {
            (
                state.last_bytes.saturating_sub(bytes_done) as f64,
                state.last_files.saturating_sub(files_done) as f64,
            )
        } else {
            (
                bytes_done.saturating_sub(state.last_bytes) as f64,
                files_done.saturating_sub(state.last_files) as f64,
            )
        };

        let inst_bytes_rate = delta_bytes / dt;
        let inst_files_rate = delta_files / dt;

        // Time-weighted EWMA: α = 1 − exp(−Δt / τ). At Δt = τ, α ≈ 0.63 (most
        // of the weight on the new sample). At Δt ≪ τ, α small (heavy smoothing).
        let alpha = 1.0 - (-dt / EWMA_TAU_SECS).exp();

        if state.samples == 0 {
            // First post-seed sample: initialize the EWMA directly. Starting
            // from 0 with α ≈ 0.06 means it would take ~30 samples to reach the
            // true rate, which is too sluggish for fast-finishing operations.
            state.bytes_rate = inst_bytes_rate;
            state.files_rate = inst_files_rate;
        } else {
            state.bytes_rate = alpha * inst_bytes_rate + (1.0 - alpha) * state.bytes_rate;
            // Only update files_rate when a file actually completed. File
            // completions are bursty (one whole file at a time), so EWMA-ing
            // `delta_files == 0` samples decays the rate toward zero during
            // long single-file streams (e.g. a 500 MB video over MTP). That
            // makes `eta_files` explode, and `max(eta_bytes, eta_files)` picks
            // the bogus value (a 393 h ETA on a 22 min copy). Treat zero-delta
            // samples as "no information"; keep the last positive rate until
            // another completion arrives.
            if delta_files > 0.0 {
                state.files_rate = alpha * inst_files_rate + (1.0 - alpha) * state.files_rate;
            }
        }

        state.last_t = now;
        state.last_human_wait = human_wait_total;
        state.working_elapsed += Duration::from_secs_f64(dt);
        state.last_bytes = bytes_done;
        state.last_files = files_done;
        state.samples = state.samples.saturating_add(1);

        compute_stats(state, bytes_done, bytes_total, files_done, files_total)
    }
}

fn compute_stats(
    state: &PhaseState,
    bytes_done: u64,
    bytes_total: u64,
    files_done: usize,
    files_total: usize,
) -> EtaStats {
    let bytes_per_second = state.bytes_rate.max(0.0).round() as u64;
    let files_per_second = state.files_rate.max(0.0) as f32;

    let warmed_up = state.samples >= MIN_SAMPLES_FOR_ETA && state.working_elapsed >= MIN_ELAPSED_FOR_ETA;

    // Remaining work toward the phase target: zero for a draining bar, the totals
    // for a filling one. ❌ Don't key this on the phase — a reversal started from
    // the history dialog reports `RollingBack` while counting UP, and reading its
    // counters as "distance from zero" makes the ETA grow as it progresses.
    let (remaining_bytes, remaining_files) = if state.counters_drain {
        (bytes_done, files_done)
    } else {
        (
            bytes_total.saturating_sub(bytes_done),
            files_total.saturating_sub(files_done),
        )
    };

    let eta_seconds = if !warmed_up {
        None
    } else {
        eta_from_axes(remaining_bytes, state.bytes_rate, remaining_files, state.files_rate)
    };

    EtaStats {
        bytes_per_second,
        files_per_second,
        eta_seconds,
    }
}

/// Combine the two per-axis ETAs by taking the max. Either rate being zero
/// disqualifies that axis (we don't know how long the remaining work will take
/// on a stalled axis, so we lean on the other). Both stalled → `None`.
///
/// `max` is the elegant move: the operation can't finish before both axes
/// are done. When one axis has zero work left (large files done, only small
/// ones to go), its ETA is `0` and the other axis dominates naturally.
fn eta_from_axes(remaining_bytes: u64, bytes_rate: f64, remaining_files: usize, files_rate: f64) -> Option<u32> {
    let eta_bytes = if bytes_rate > 0.0 {
        Some(remaining_bytes as f64 / bytes_rate)
    } else if remaining_bytes == 0 {
        Some(0.0)
    } else {
        None
    };
    let eta_files = if files_rate > 0.0 {
        Some(remaining_files as f64 / files_rate)
    } else if remaining_files == 0 {
        Some(0.0)
    } else {
        None
    };

    let combined = match (eta_bytes, eta_files) {
        (Some(b), Some(f)) => b.max(f),
        (Some(b), None) => b,
        (None, Some(f)) => f,
        (None, None) => return None,
    };

    // Clamp to ≥1 s while any axis still has work. The UI showing "0 s
    // remaining" while files are still streaming is the bug we're fixing.
    let any_work_left = remaining_bytes > 0 || remaining_files > 0;
    let seconds = if any_work_left { combined.max(1.0) } else { 0.0 };

    Some(seconds.min(u32::MAX as f64).ceil() as u32)
}

#[cfg(test)]
#[path = "eta_tests.rs"]
mod tests;
