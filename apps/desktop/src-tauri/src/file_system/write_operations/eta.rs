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
//! ## Phase transitions and rollback
//!
//! Resetting on phase change (scanning → copying, copying → rolling_back) is
//! required because the counters reset too. Otherwise an EWMA fed
//! `bytes_done = 0` after `bytes_done = 5_000_000_000` would emit garbage.
//! Rollback flips the sign: `bytes_done` decreases. The estimator treats
//! "progress toward the phase target" as positive (target is `bytes_total`
//! during forward phases, `0` during rollback).

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

/// Don't emit an ETA until at least this much wall time has elapsed in the current
/// phase. Catches the "200 ms in, rate is 50 MB/s" → "ETA = 0 s" footgun before
/// the EWMA settles.
const MIN_ELAPSED_FOR_ETA: Duration = Duration::from_millis(800);

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

/// State for one phase of one operation. Reset on phase transition.
#[derive(Debug)]
struct PhaseState {
    phase: WriteOperationPhase,
    started_at: Instant,
    last_t: Instant,
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
#[derive(Debug, Default)]
pub struct EtaEstimator {
    state: Option<PhaseState>,
}

impl EtaEstimator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the estimator with the latest counters and return the current stats.
    ///
    /// `now` is injected (not read from `Instant::now()` internally) so tests can
    /// drive synthetic timelines without touching the real clock.
    pub fn update(
        &mut self,
        now: Instant,
        phase: WriteOperationPhase,
        bytes_done: u64,
        bytes_total: u64,
        files_done: usize,
        files_total: usize,
    ) -> EtaStats {
        // On phase change (or first call), reseed and emit zero stats.
        // The next call's Δt will be measured against this seed.
        let needs_reset = match &self.state {
            None => true,
            Some(s) => s.phase != phase,
        };

        if needs_reset {
            self.state = Some(PhaseState {
                phase,
                started_at: now,
                last_t: now,
                last_bytes: bytes_done,
                last_files: files_done,
                bytes_rate: 0.0,
                files_rate: 0.0,
                samples: 0,
            });
            return EtaStats::ZERO;
        }

        let state = self.state.as_mut().expect("just reset or pre-existing");
        let dt = now.saturating_duration_since(state.last_t).as_secs_f64();
        if dt <= 0.0 {
            // Two updates in the same instant; return the last computed stats.
            return compute_stats(state, bytes_done, bytes_total, files_done, files_total);
        }

        // Δ toward the phase target. Forward phases grow the counters; rollback
        // shrinks them. `saturating_sub` neutralizes spurious regressions (a
        // late event arriving after a counter reset, etc.).
        let (delta_bytes, delta_files) = if phase == WriteOperationPhase::RollingBack {
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
            state.files_rate = alpha * inst_files_rate + (1.0 - alpha) * state.files_rate;
        }

        state.last_t = now;
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

    let warmed_up = state.samples >= MIN_SAMPLES_FOR_ETA
        && state.last_t.saturating_duration_since(state.started_at) >= MIN_ELAPSED_FOR_ETA;

    // Remaining work toward the phase target.
    let (remaining_bytes, remaining_files) = if state.phase == WriteOperationPhase::RollingBack {
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
mod tests {
    use super::*;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    /// Helper: drive the estimator through a sequence of (t_ms, bytes_done, files_done)
    /// samples and return the final stats.
    fn run(
        phase: WriteOperationPhase,
        bytes_total: u64,
        files_total: usize,
        samples: &[(u64, u64, usize)],
    ) -> EtaStats {
        let start = Instant::now();
        let mut est = EtaEstimator::new();
        let mut last = EtaStats::ZERO;
        for &(t_ms, b, f) in samples {
            last = est.update(at(start, t_ms), phase, b, bytes_total, f, files_total);
        }
        last
    }

    #[test]
    fn first_sample_seeds_and_returns_zero() {
        let stats = run(WriteOperationPhase::Copying, 1_000, 10, &[(0, 0, 0)]);
        assert_eq!(stats, EtaStats::ZERO);
    }

    #[test]
    fn warmup_suppresses_eta_until_min_elapsed() {
        // Two samples 200 ms apart, below MIN_ELAPSED_FOR_ETA.
        let stats = run(
            WriteOperationPhase::Copying,
            10_000_000,
            100,
            &[(0, 0, 0), (200, 2_000_000, 20)],
        );
        assert_eq!(stats.eta_seconds, None);
        // But rates are populated after the first delta.
        assert!(stats.bytes_per_second > 0);
        assert!(stats.files_per_second > 0.0);
    }

    #[test]
    fn byte_heavy_steady_workload() {
        // 1 GB at 100 MB/s, one file every second. After 2 s of progress,
        // ETA should be ~8 s (800 MB / 100 MB/s = 8 s).
        let stats = run(
            WriteOperationPhase::Copying,
            1_000_000_000,
            10,
            &[(0, 0, 0), (1000, 100_000_000, 1), (2000, 200_000_000, 2)],
        );
        assert!(
            (stats.bytes_per_second as i64 - 100_000_000).abs() < 5_000_000,
            "bytes_per_second = {} expected ~100 MB/s",
            stats.bytes_per_second,
        );
        let eta = stats.eta_seconds.expect("warmed up");
        assert!((7..=10).contains(&eta), "eta = {eta} expected ~8 s");
    }

    #[test]
    fn file_heavy_steady_workload() {
        // 100k tiny files, ~1 kB each (so byte work is trivial). 1k files/s.
        // After 2 s of progress, 2k files done: 98k left at 1k/s → ~98 s.
        let stats = run(
            WriteOperationPhase::Deleting,
            100_000_000,
            100_000,
            &[(0, 0, 0), (1000, 1_000_000, 1_000), (2000, 2_000_000, 2_000)],
        );
        assert!(
            (stats.files_per_second - 1000.0).abs() < 50.0,
            "files_per_second = {} expected ~1000",
            stats.files_per_second,
        );
        let eta = stats.eta_seconds.expect("warmed up");
        assert!((90..=110).contains(&eta), "eta = {eta} expected ~98 s");
    }

    /// The pathological case from the bug report: big files delete first, the
    /// size bar saturates fast, but a long tail of small files remains.
    /// Byte-only ETA would say ~0 s; the two-axis ETA must stay > 0 until the
    /// files axis is done.
    #[test]
    fn big_first_then_small_tail_keeps_eta_alive() {
        let start = Instant::now();
        let mut est = EtaEstimator::new();
        let bytes_total = 5_400_000_000_u64;
        let files_total = 174_661_usize;

        // Phase 1 (0–1 s): two huge files delete, bytes saturate, files barely move.
        est.update(
            at(start, 0),
            WriteOperationPhase::Deleting,
            0,
            bytes_total,
            0,
            files_total,
        );
        est.update(
            at(start, 500),
            WriteOperationPhase::Deleting,
            2_700_000_000,
            bytes_total,
            5,
            files_total,
        );
        est.update(
            at(start, 1000),
            WriteOperationPhase::Deleting,
            5_400_000_000,
            bytes_total,
            10,
            files_total,
        );

        // Phase 2 (1–6 s): byte rate collapses (nothing left to delete byte-wise),
        // file rate climbs as the small-file tail streams through at ~5k/s.
        // After several seconds of small-file deletion, ETA must reflect files left.
        let mut last = EtaStats::ZERO;
        for i in 1..=10 {
            let t = 1000 + i * 500;
            let files_done = (10 + i as usize * 2_500).min(files_total);
            last = est.update(
                at(start, t),
                WriteOperationPhase::Deleting,
                bytes_total,
                bytes_total,
                files_done,
                files_total,
            );
        }

        // 25_010 of 174_661 files done: about 149_651 remaining. At ~5k/s ≈ ~30 s.
        let eta = last.eta_seconds.expect("warmed up by now");
        assert!(
            eta >= 20,
            "ETA collapsed to {eta} s: should reflect remaining file count",
        );
        // Files rate should dominate the readout.
        assert!(last.files_per_second > 1000.0);
    }

    /// Mid-operation slowdown: starts at 60 MB/s, drops to 6 MB/s. The EWMA
    /// must converge to the new rate within ~3τ (≈ 9 s), not stay anchored to
    /// the historical average.
    #[test]
    fn adapts_to_mid_operation_slowdown() {
        let start = Instant::now();
        let mut est = EtaEstimator::new();
        let mut bytes_done: u64 = 0;

        // 10 s at 60 MB/s.
        for i in 0..=10 {
            let t = i * 1000;
            est.update(
                at(start, t),
                WriteOperationPhase::Copying,
                bytes_done,
                10_000_000_000,
                i as usize,
                1000,
            );
            bytes_done += 60_000_000;
        }

        // 12 s at 6 MB/s.
        let mut final_stats = EtaStats::ZERO;
        for i in 1..=12 {
            let t = 10_000 + i * 1000;
            bytes_done += 6_000_000;
            final_stats = est.update(
                at(start, t),
                WriteOperationPhase::Copying,
                bytes_done,
                10_000_000_000,
                (10 + i) as usize,
                1000,
            );
        }

        // After 12 s at the new rate (4τ) the EWMA's residual error fraction is
        // exp(-12/3) ≈ 1.8% of the original step. For a 60→6 MB/s step that's a
        // ~1 MB/s residual, so the reading should be ≤ 8 MB/s (under 35% over
        // target). Importantly, it must be well below the original 60 MB/s
        // (i.e. the estimator is converging, not anchored).
        let bps = final_stats.bytes_per_second;
        assert!(
            bps <= 8_000_000,
            "bytes_per_second = {bps} should have decayed to ≤ 8 MB/s after 12 s at new rate",
        );
        assert!(
            bps >= 5_500_000,
            "bytes_per_second = {bps} should still be ≥ 5.5 MB/s (overshooting low means the EWMA went off course)",
        );
    }

    #[test]
    fn brief_stall_then_resume_recovers() {
        let start = Instant::now();
        let mut est = EtaEstimator::new();

        // 5 s of steady 100 MB/s.
        for i in 0..=5 {
            est.update(
                at(start, i * 1000),
                WriteOperationPhase::Copying,
                i * 100_000_000,
                10_000_000_000,
                (i * 5) as usize,
                500,
            );
        }

        // 5 s of stall (no progress).
        for i in 1..=5 {
            est.update(
                at(start, 5_000 + i * 1000),
                WriteOperationPhase::Copying,
                500_000_000,
                10_000_000_000,
                25,
                500,
            );
        }

        // The rate has decayed significantly. ETA may be None or large; either
        // is acceptable. We just need it not to be a wildly wrong small number.
        let stalled = est.update(
            at(start, 10_000),
            WriteOperationPhase::Copying,
            500_000_000,
            10_000_000_000,
            25,
            500,
        );
        assert!(
            stalled.eta_seconds.map(|e| e > 30).unwrap_or(true),
            "ETA during stall = {:?}: should be large or None",
            stalled.eta_seconds,
        );

        // Resume at 100 MB/s for 6 s.
        let mut bytes = 500_000_000;
        let mut final_stats = EtaStats::ZERO;
        for i in 1..=6 {
            bytes += 100_000_000;
            final_stats = est.update(
                at(start, 10_000 + i * 1000),
                WriteOperationPhase::Copying,
                bytes,
                10_000_000_000,
                (25 + i * 5) as usize,
                500,
            );
        }
        let bps = final_stats.bytes_per_second;
        assert!(
            (80_000_000..=120_000_000).contains(&bps),
            "post-recovery bytes_per_second = {bps} expected ~100 MB/s",
        );
    }

    #[test]
    fn phase_transition_resets_state() {
        let start = Instant::now();
        let mut est = EtaEstimator::new();

        // Scanning phase: 1000 files/s.
        est.update(at(start, 0), WriteOperationPhase::Scanning, 0, 0, 0, 0);
        est.update(at(start, 1000), WriteOperationPhase::Scanning, 0, 0, 1000, 0);
        est.update(at(start, 2000), WriteOperationPhase::Scanning, 0, 0, 2000, 0);

        // Transition to Copying: bytes_done resets to 0 from scanning's 0,
        // but the file count is fresh. files_done starts back at 0 in the
        // emitter's view of "files copied so far" (vs "files scanned").
        let on_transition = est.update(at(start, 2100), WriteOperationPhase::Copying, 0, 5_000_000_000, 0, 2000);
        // Reset → zero stats on the transition sample, then re-warm.
        assert_eq!(on_transition, EtaStats::ZERO);
    }

    #[test]
    fn rollback_phase_computes_eta_toward_zero() {
        let start = Instant::now();
        let mut est = EtaEstimator::new();

        // Operation made it to 500 MB / 50 files before rollback starts.
        // During rollback, the counters decrease.
        est.update(
            at(start, 0),
            WriteOperationPhase::RollingBack,
            500_000_000,
            1_000_000_000,
            50,
            100,
        );
        est.update(
            at(start, 1000),
            WriteOperationPhase::RollingBack,
            400_000_000,
            1_000_000_000,
            40,
            100,
        );
        let stats = est.update(
            at(start, 2000),
            WriteOperationPhase::RollingBack,
            300_000_000,
            1_000_000_000,
            30,
            100,
        );

        // 100 MB/s deletion rate, 300 MB left to undo → ~3 s.
        let eta = stats.eta_seconds.expect("warmed up");
        assert!((2..=4).contains(&eta), "rollback eta = {eta} expected ~3 s");
        assert!(stats.bytes_per_second >= 90_000_000);
    }

    #[test]
    fn same_instant_double_update_is_idempotent() {
        let start = Instant::now();
        let mut est = EtaEstimator::new();
        est.update(at(start, 0), WriteOperationPhase::Copying, 0, 1_000, 0, 10);
        est.update(at(start, 1000), WriteOperationPhase::Copying, 500, 1_000, 5, 10);
        let a = est.update(at(start, 2000), WriteOperationPhase::Copying, 700, 1_000, 7, 10);
        let b = est.update(at(start, 2000), WriteOperationPhase::Copying, 800, 1_000, 8, 10);
        // Second call at same instant: rates unchanged, but counters refreshed.
        // The next call (with dt > 0) will use the latest counters as the
        // reference. We just check that the second update didn't blow up or
        // produce NaN.
        assert!(b.bytes_per_second >= a.bytes_per_second.saturating_sub(1));
    }

    /// `cargo-mutants` survivor target: the rate formula `delta / dt` is
    /// numerically indistinguishable from `delta * dt` whenever dt is exactly
    /// 1.0 s; every other test uses 1 s steps. This drives the estimator
    /// with `dt = 2.0 s` so `delta / 2` and `delta * 2` differ by 4x, then
    /// asserts the rate tightly enough to catch `* dt` and `% dt` mutants
    /// on the `inst_bytes_rate` / `inst_files_rate` lines.
    #[test]
    fn rate_division_uses_dt_not_a_constant() {
        // Two 2-second steps at 100 MB/s and 50 files/s. After seed + 1 EWMA
        // step, the rate should be very close to the instantaneous rate of
        // 100 MB/s and 50 files/s (the EWMA combines the post-seed direct-set
        // 100 MB/s with another 100 MB/s sample, no drift).
        let stats = run(
            WriteOperationPhase::Copying,
            10_000_000_000,
            10_000,
            &[(0, 0, 0), (2000, 200_000_000, 100), (4000, 400_000_000, 200)],
        );
        let bps = stats.bytes_per_second;
        let fps = stats.files_per_second;
        // Tight bounds: 100 MB/s ± 1%, 50 files/s ± 1%. `* dt` would give
        // 400 MB/s (4x), `% dt` would give 0 (since deltas are exact integer
        // multiples of 2.0).
        assert!(
            (99_000_000..=101_000_000).contains(&bps),
            "bytes_per_second = {bps} expected ~100 MB/s (within 1%)",
        );
        assert!((49.5..=50.5).contains(&fps), "files_per_second = {fps} expected ~50",);
    }

    /// `cargo-mutants` survivor target: the `samples == 0` branch (line 159)
    /// directly seeds the EWMA with the first post-seed sample, instead of
    /// blending it against the initial 0 rate. Existing 3-sample tests mask
    /// this because by the 3rd sample the EWMA has caught up. With only one
    /// post-seed sample, the mutant `!= 0` would give the EWMA-blended
    /// `alpha * inst_rate` instead of the full `inst_rate`.
    #[test]
    fn first_post_seed_sample_initializes_rate_directly() {
        // 1 second delta, 100 MB/s, 100 files/s. After exactly 2 updates
        // (seed + one post-seed), the rate should be the full instantaneous
        // rate, not the EWMA-blended value of ~alpha * inst_rate (alpha at
        // dt=1, tau=3 is ~0.283, so blended would be ~28.3 MB/s vs the
        // correct ~100 MB/s).
        let stats = run(
            WriteOperationPhase::Copying,
            10_000_000_000,
            10_000,
            &[(0, 0, 0), (1000, 100_000_000, 100)],
        );
        let bps = stats.bytes_per_second;
        let fps = stats.files_per_second;
        assert!(
            (99_000_000..=101_000_000).contains(&bps),
            "bytes_per_second after first post-seed sample = {bps}, expected ~100 MB/s \
             (mutant `samples != 0` would give ~28 MB/s from EWMA-from-zero)",
        );
        assert!(
            (99.0..=101.0).contains(&fps),
            "files_per_second after first post-seed sample = {fps}, expected ~100",
        );
    }
}
