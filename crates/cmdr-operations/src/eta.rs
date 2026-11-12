//! ETA + throughput estimator for write operations.
//!
//! Tracks two rates (bytes/second and files/second) via a time-weighted
//! exponential moving average (τ ≈ 3 s half-life), and learns what a file
//! costs against what a byte costs (`eta_cost_model.rs`: `time ≈ a · files +
//! b · bytes`, fitted on this phase's own history). The ETA prices the
//! remaining work at those costs over the work the rates say gets done per
//! second: `(a · files_left + b · bytes_left) / (a · files_rate + b · bytes_rate)`.
//!
//! This shape matters because the two axes go out of sync in real workloads,
//! in both directions. Deleting 5 GB of mixed sizes finishes the byte work in
//! the first second, then spends 20 s on per-file overhead for a tail of small
//! files: a byte-only ETA shows ~0 s. Copying a folder of 1 kB files at a
//! steady 16 files/s collapses the byte rate to kilobytes per second, and the
//! 1.5 GB of bigger files still ahead over that rate read "2h 9m left" on
//! 11 minutes of work (#1146): the per-axis `max` this replaced. Priced
//! per half, each part of the remaining work costs what it really does. The
//! `max` stays as the fallback for when the costs can't speak (`costed_eta`).
//!
//! Known limit: a file that takes longer than the device-barrier interval
//! (2 s, `cmdr_fs::durable::BARRIER_INTERVAL`) pays a barrier each, which the
//! counters can't tell from a per-file cost, so during a run of big files the
//! small files still ahead are priced one barrier too high each, until they
//! start and re-measure it. Pinned in `eta_workload_tests.rs`.
//!
//! ## Adaptivity
//!
//! Pure EWMA, no "overall average" blend. If the network drops mid-operation,
//! the EWMA decays to the new rate within ~3τ (≈9 s) without being anchored
//! to historical numbers. The smoothing constant is computed time-weighted:
//! `α = 1 - exp(-Δt / τ)`, so the response is identical whether progress events
//! arrive every 50 ms or every 500 ms.
//!
//! ## Files are counted between completions
//!
//! Bytes flow continuously, so bytes per emit interval is a fair sample. Files
//! land one whole file at a time, so the files rate is measured over the
//! working time from one completion to the next, ❌ never per emit: per emit,
//! every completion reads as `1 / 0.2 s = 5 files/s` however long the file
//! took. A window shorter than [`MIN_FILES_WINDOW`] waits for the next
//! completion, which is what stops a thumbnail landing microseconds after the
//! seed from reading as "17 629 files/s" on a video move.
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
//! being slow, and the ETA has to say so. A share that's GONE is the one device
//! wait that is: the transfer is waiting for it to come back, says so, and its
//! minutes aren't throughput (`super::outage_slot`).
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

use crate::rt::Instant;
use std::time::Duration;

use super::types::{ProgressAxis, WriteOperationPhase};

#[path = "eta_cost_model.rs"]
mod cost_model;
use cost_model::CostModel;

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

/// The shortest stretch of working time a files measurement may span. Just
/// under one 200 ms emit interval, so every emit that carries a completion can
/// measure, while a completion an off-cadence event reports microseconds after
/// the last one folds into the next window instead of dividing by nothing.
const MIN_FILES_WINDOW: Duration = Duration::from_millis(150);

/// How far the other axis's ETA must lead the bottleneck's before it counts as
/// leading: 10 %, so two axes running neck and neck don't trade places.
const AXIS_LEAD: f64 = 1.10;

/// How long, in WORKING time, the other axis must keep leading before it takes
/// over. Every folder bar in the transfer tree fills along the bottleneck, so a
/// flip redraws them all; a lead that comes and goes inside this window would
/// make them flicker.
const AXIS_HOLD: Duration = Duration::from_secs(3);

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
    /// The axis whose own ETA (remaining over its rate) is the longer one,
    /// which the folder bars fill along. Held steady: the other one
    /// takes over only after leading by more than [`AXIS_LEAD`] for
    /// [`AXIS_HOLD`] of working time. `Bytes` while warming up, `Files` for an
    /// operation that moves no bytes at all (a rename move).
    pub bottleneck: ProgressAxis,
}

impl EtaStats {
    pub const ZERO: Self = Self {
        bytes_per_second: 0,
        files_per_second: 0.0,
        eta_seconds: None,
        bottleneck: ProgressAxis::Bytes,
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
    /// Where the open files window starts: `working_elapsed` and the file count
    /// at the last files measurement (or the phase start).
    files_window_start: Duration,
    files_window_count: usize,
    /// Whether `files_rate` holds a measurement yet. The first one sets it
    /// directly; later ones blend in.
    files_measured: bool,
    samples: u32,
    /// What a file costs against what a byte costs, learned over this phase:
    /// the weights that combine the two rates into one estimate.
    costs: CostModel,
    /// The bottleneck as last reported.
    axis: ProgressAxis,
    /// When (in `working_elapsed`) the other axis started leading, while it
    /// still does.
    challenger_since: Option<Duration>,
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
            let axis = starting_axis(bytes_total, files_total);
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
                files_window_start: Duration::ZERO,
                files_window_count: files_done,
                files_measured: false,
                samples: 0,
                costs: CostModel::new(),
                axis,
                challenger_since: None,
            });
            return EtaStats {
                bottleneck: axis,
                ..EtaStats::ZERO
            };
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
            state.files_window_count = files_done;
            return compute_stats(state, bytes_done, bytes_total, files_done, files_total);
        }

        // Δ toward the phase target. A filling bar grows the counters; a draining
        // one shrinks them. `saturating_sub` neutralizes spurious regressions (a
        // late event arriving after a counter reset, etc.).
        let delta_bytes = toward_target(counters_drain, state.last_bytes, bytes_done) as f64;
        let delta_files = toward_target(counters_drain, state.last_files as u64, files_done as u64);
        let completed_now = delta_files > 0;
        state
            .costs
            .observe(delta_files as f64, delta_bytes, dt, MIN_FILES_WINDOW.as_secs_f64());

        let inst_bytes_rate = delta_bytes / dt;

        if state.samples == 0 {
            // First post-seed sample: initialize the EWMA directly. Starting
            // from 0 with α ≈ 0.06 means it would take ~30 samples to reach the
            // true rate, which is too sluggish for fast-finishing operations.
            state.bytes_rate = inst_bytes_rate;
        } else {
            state.bytes_rate = ewma(state.bytes_rate, inst_bytes_rate, dt);
        }

        state.last_t = now;
        state.last_human_wait = human_wait_total;
        state.working_elapsed += Duration::from_secs_f64(dt);

        // The files window closes only on a completion. Between completions
        // there's no information, and EWMA-ing the empty samples decayed the
        // rate toward zero during a long single-file stream (a 500 MB video
        // over MTP), which made `eta_files` explode: a 393 h ETA on a 22 min
        // copy. The rate holds until the next completion instead.
        let window = state.working_elapsed.saturating_sub(state.files_window_start);
        if completed_now && window >= MIN_FILES_WINDOW {
            let window_secs = window.as_secs_f64();
            let completed = toward_target(counters_drain, state.files_window_count as u64, files_done as u64) as f64;
            let inst_files_rate = completed / window_secs;
            state.files_rate = if state.files_measured {
                ewma(state.files_rate, inst_files_rate, window_secs)
            } else {
                inst_files_rate
            };
            state.files_measured = true;
            state.files_window_start = state.working_elapsed;
            state.files_window_count = files_done;
        }
        state.last_bytes = bytes_done;
        state.last_files = files_done;
        state.samples = state.samples.saturating_add(1);

        compute_stats(state, bytes_done, bytes_total, files_done, files_total)
    }
}

/// How far a counter moved toward the phase target since `last`. A filling bar
/// grows its counters; a draining one shrinks them. `saturating_sub`
/// neutralizes spurious regressions (a late event arriving after a counter
/// reset, etc.).
fn toward_target(counters_drain: bool, last: u64, now: u64) -> u64 {
    if counters_drain {
        last.saturating_sub(now)
    } else {
        now.saturating_sub(last)
    }
}

/// Time-weighted EWMA step: α = 1 − exp(−Δt / τ). At Δt = τ, α ≈ 0.63 (most of
/// the weight on the new sample). At Δt ≪ τ, α small (heavy smoothing). Weighting
/// by the span a sample covers keeps the response the same at any cadence.
fn ewma(rate: f64, sample: f64, span_secs: f64) -> f64 {
    let alpha = 1.0 - (-span_secs / EWMA_TAU_SECS).exp();
    alpha * sample + (1.0 - alpha) * rate
}

fn compute_stats(
    state: &mut PhaseState,
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

    let eta_bytes = axis_eta(remaining_bytes as f64, state.bytes_rate);
    let eta_files = axis_eta(remaining_files as f64, state.files_rate);
    let eta_seconds = if !warmed_up {
        None
    } else {
        let combined = costed_eta(
            &state.costs,
            remaining_bytes,
            remaining_files,
            state.bytes_rate,
            state.files_rate,
        )
        .or_else(|| eta_from_axes(eta_bytes, eta_files));
        combined.map(|seconds| whole_seconds(seconds, remaining_bytes > 0 || remaining_files > 0))
    };
    settle_axis(state, warmed_up, eta_bytes, eta_files, bytes_total, files_total);

    EtaStats {
        bytes_per_second,
        files_per_second,
        eta_seconds,
        bottleneck: state.axis,
    }
}

/// The bottleneck before any rate is known: bytes, unless the operation moves
/// none (a rename), where the file count is all there is.
fn starting_axis(bytes_total: u64, files_total: usize) -> ProgressAxis {
    if bytes_total == 0 && files_total > 0 {
        ProgressAxis::Files
    } else {
        ProgressAxis::Bytes
    }
}

/// Moves the bottleneck to the other axis once that one's ETA has led by more
/// than [`AXIS_LEAD`] for [`AXIS_HOLD`] of working time. An axis with no rate
/// yet can't lead or be led: nothing says how long it takes.
fn settle_axis(
    state: &mut PhaseState,
    warmed_up: bool,
    eta_bytes: Option<f64>,
    eta_files: Option<f64>,
    bytes_total: u64,
    files_total: usize,
) {
    if !warmed_up || (bytes_total == 0 && files_total > 0) {
        state.axis = starting_axis(bytes_total, files_total);
        state.challenger_since = None;
        return;
    }
    let (current, other) = match state.axis {
        ProgressAxis::Bytes => (eta_bytes, eta_files),
        ProgressAxis::Files => (eta_files, eta_bytes),
    };
    let other_leads = matches!((other, current), (Some(other), Some(current)) if other > current * AXIS_LEAD);
    if !other_leads {
        state.challenger_since = None;
        return;
    }
    let since = *state.challenger_since.get_or_insert(state.working_elapsed);
    if state.working_elapsed.saturating_sub(since) >= AXIS_HOLD {
        state.axis = match state.axis {
            ProgressAxis::Bytes => ProgressAxis::Files,
            ProgressAxis::Files => ProgressAxis::Bytes,
        };
        state.challenger_since = None;
    }
}

/// One axis's seconds to go: remaining work over its rate, `0` with nothing
/// left, and `None` on a stalled axis that still has work (we don't know how
/// long it takes, so the other axis decides).
fn axis_eta(remaining: f64, rate: f64) -> Option<f64> {
    if rate > 0.0 {
        Some(remaining / rate)
    } else if remaining == 0.0 {
        Some(0.0)
    } else {
        None
    }
}

/// The remaining work priced by the learned costs, over the work the two
/// rates say is getting done per second:
/// `(a · files_left + b · bytes_left) / (a · files_rate + b · bytes_rate)`.
///
/// Only the ratio of `a` to `b` matters, and the rates keep their 3 s
/// responsiveness. With `a` negligible it IS the bytes ETA (a big-file copy),
/// with `b` negligible the files ETA (a run of tiny files), and a mix lands in
/// between, each half of the remaining work at its own price. `None` when the
/// costs can't speak for the work left (a cost clamped to zero with only its
/// axis remaining) or the rates they weigh are both stalled.
fn costed_eta(
    costs: &CostModel,
    remaining_bytes: u64,
    remaining_files: usize,
    bytes_rate: f64,
    files_rate: f64,
) -> Option<f64> {
    let per_file = costs.secs_per_file();
    let per_byte = costs.secs_per_byte();
    let work_left = per_file * remaining_files as f64 + per_byte * remaining_bytes as f64;
    let work_per_second = per_file * files_rate.max(0.0) + per_byte * bytes_rate.max(0.0);
    (work_left > 0.0 && work_per_second > 0.0).then(|| work_left / work_per_second)
}

/// The fallback when the costed estimate can't speak: the longer of the two
/// per-axis ETAs, since the operation can't finish before both axes are done.
/// A stalled axis is disqualified (we don't know how long its work takes, so
/// we lean on the other). Both stalled → `None`.
fn eta_from_axes(eta_bytes: Option<f64>, eta_files: Option<f64>) -> Option<f64> {
    match (eta_bytes, eta_files) {
        (Some(b), Some(f)) => Some(b.max(f)),
        (Some(b), None) => Some(b),
        (None, Some(f)) => Some(f),
        (None, None) => None,
    }
}

/// Clamp to ≥1 s while any axis still has work: "0 s remaining" while files
/// are still streaming reads as done when it isn't.
fn whole_seconds(seconds: f64, any_work_left: bool) -> u32 {
    let seconds = if any_work_left { seconds.max(1.0) } else { 0.0 };
    seconds.min(u32::MAX as f64).ceil() as u32
}

#[cfg(test)]
#[path = "eta_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "eta_workload_tests.rs"]
mod workload_tests;
