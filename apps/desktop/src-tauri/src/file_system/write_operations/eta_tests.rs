//! Unit tests for the ETA + throughput estimator.
//!
//! Every test drives `EtaEstimator` on a synthetic timeline (`Instant`s derived
//! from one `start`), so a run never depends on the real clock or on how fast
//! the machine is. The module doc on `eta.rs` explains the model these assert.

use super::*;

/// Test-only shorthand for the common case: nobody was ever asked anything, so
/// wall time and working time are the same thing.
impl EtaEstimator {
    fn update_at(
        &mut self,
        now: Instant,
        phase: WriteOperationPhase,
        bytes_done: u64,
        bytes_total: u64,
        files_done: usize,
        files_total: usize,
    ) -> EtaStats {
        self.update(EtaSample {
            now,
            human_wait_total: Duration::ZERO,
            phase,
            bytes_done,
            bytes_total,
            files_done,
            files_total,
        })
    }
}

fn at(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

/// Helper: drive the estimator through a sequence of (t_ms, bytes_done, files_done)
/// samples and return the final stats.
fn run(phase: WriteOperationPhase, bytes_total: u64, files_total: usize, samples: &[(u64, u64, usize)]) -> EtaStats {
    let start = Instant::now();
    let mut est = EtaEstimator::new();
    let mut last = EtaStats::ZERO;
    for &(t_ms, b, f) in samples {
        last = est.update_at(at(start, t_ms), phase, b, bytes_total, f, files_total);
    }
    last
}

#[test]
fn first_sample_seeds_and_returns_zero() {
    let stats = run(WriteOperationPhase::Copying, 1_000, 10, &[(0, 0, 0)]);
    assert_eq!(stats, EtaStats::ZERO);
}

#[test]
fn bulk_skip_baseline_jump_does_not_pollute_rate() {
    // Models the volume-copy Skip-All path. Caller emits `(0, 0)` at the
    // Copying-phase boundary (the estimator reseeds and returns ZERO);
    // the driver's bulk-skip prelude then jumps the counters to
    // `(bulk_skip_files, bulk_skip_bytes)` instantly. Without an explicit
    // baseline reseed, the bulk-skip delta over ε time becomes the
    // first-sample rate (~22 GB/s, ~250 files/s in this fixture), and
    // EWMA takes many seconds to decay it. The fix: call
    // `reseed_baseline` before the bulk-skip emit so the jump becomes
    // the new starting point, not throughput. The next real per-file
    // emit's delta is then just the actually-copied portion.
    let start = Instant::now();
    let mut est = EtaEstimator::new();

    // t=0: initial Copying emit (phase transition Scanning -> Copying).
    let initial = est.update_at(at(start, 0), WriteOperationPhase::Copying, 0, 35_000_000_000, 0, 1051);
    assert_eq!(initial, EtaStats::ZERO);

    // t=1 ms: driver bulk-skip prelude credits 22 GB / 250 files
    // instantly. Caller calls `reseed_baseline` immediately before the
    // emit so the estimator absorbs the jump as its new starting point.
    est.reseed_baseline(at(start, 1), Duration::ZERO, 22_000_000_000, 250);

    // t=1001 ms: first real per-file emit. Actually-copied delta vs.
    // the new baseline = 15 MB / 1 file over 1 s.
    let stats = est.update_at(
        at(start, 1001),
        WriteOperationPhase::Copying,
        22_015_000_000,
        35_000_000_000,
        251,
        1051,
    );

    // Pre-fix: bytes_per_second is in the GB/s range and files_per_second
    // is in the hundreds (a 250-file / 22-GB jump over ε time pinned the
    // EWMA's first sample, then partially decayed). Assert single-digit
    // multiples of the true rate, not orders of magnitude off.
    assert!(
        stats.bytes_per_second < 50_000_000,
        "bytes_per_second = {} (expected ~15 MB/s, bulk-skip should not feed the rate)",
        stats.bytes_per_second,
    );
    assert!(
        stats.files_per_second < 5.0,
        "files_per_second = {} (expected ~1 file/s, bulk-skip should not feed the rate)",
        stats.files_per_second,
    );
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
    est.update_at(
        at(start, 0),
        WriteOperationPhase::Deleting,
        0,
        bytes_total,
        0,
        files_total,
    );
    est.update_at(
        at(start, 500),
        WriteOperationPhase::Deleting,
        2_700_000_000,
        bytes_total,
        5,
        files_total,
    );
    est.update_at(
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
        last = est.update_at(
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

/// The pathological inverse of `big_first_then_small_tail_keeps_eta_alive`:
/// small files first, then a long single-file stream (e.g. a 500 MB video
/// from a phone). `delta_files == 0` for many samples in a row while bytes
/// keep flowing — historically the EWMA decayed `files_rate` to ~0.001,
/// which made `eta_files` explode to >100 hours and `max(eta_bytes, eta_files)`
/// picked the bogus value. Fix: skip the `files_rate` EWMA update when
/// `delta_files == 0`. ETA must stay bytes-rate-bounded in this scenario.
#[test]
fn long_single_file_stream_does_not_decay_files_rate_to_zero() {
    let start = Instant::now();
    let mut est = EtaEstimator::new();
    let bytes_total = 35_000_000_000_u64; // 35 GB total
    let files_total = 1_046_usize;

    // Phase 1 (0–6 s): 6 small-to-medium files complete at ~1/s.
    // Each ~80 MB at ~80 MB/s. After this: 480 MB done, 6 files done.
    est.update_at(
        at(start, 0),
        WriteOperationPhase::Copying,
        0,
        bytes_total,
        0,
        files_total,
    );
    for i in 1..=6 {
        let t = i * 1000;
        est.update_at(
            at(start, t),
            WriteOperationPhase::Copying,
            i * 80_000_000,
            bytes_total,
            i as usize,
            files_total,
        );
    }

    // Phase 2 (6–24 s): one big 500 MB video streams in. Bytes flow at
    // ~28 MB/s (560 MB over 20 s); no file completes for 90 sample points
    // at 200 ms each. This is the regime that used to wreck `files_rate`.
    let mut last = EtaStats::ZERO;
    let mut bytes_done = 480_000_000_u64;
    for i in 1..=90 {
        let t = 6_000 + i * 200;
        bytes_done += 5_600_000; // 5.6 MB per 200 ms = 28 MB/s
        last = est.update_at(
            at(start, t),
            WriteOperationPhase::Copying,
            bytes_done,
            bytes_total,
            6, // ← no completion across all 90 samples
            files_total,
        );
    }

    // Sanity: bytes_rate stays healthy across the long stream.
    assert!(
        last.bytes_per_second >= 25_000_000 && last.bytes_per_second <= 32_000_000,
        "bytes_per_second = {} should remain ~28 MB/s during the long stream",
        last.bytes_per_second,
    );

    // The bug: `files_rate` decayed to ~7e-4 → `eta_files` ≈ 1040/7e-4 = 1.4M s.
    // After the fix `files_rate` stays at the last positive EWMA value (~0.6)
    // so `eta_files` stays bounded (~1700 s).
    let eta = last.eta_seconds.expect("warmed up by now");
    assert!(
        eta < 10_000,
        "ETA exploded to {eta} s: files_rate decay during a long single-file stream broke the readout",
    );
    // And the files-axis rate must not have collapsed below a believable floor.
    // 6 completions in the first 6 s seeded the EWMA around 1 files/s; the
    // 90 zero-delta samples after the stream should not drag it below 0.1.
    assert!(
        last.files_per_second >= 0.1,
        "files_per_second = {} collapsed below 0.1 during the zero-delta stream",
        last.files_per_second,
    );
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
        est.update_at(
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
        final_stats = est.update_at(
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
        est.update_at(
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
        est.update_at(
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
    let stalled = est.update_at(
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
        final_stats = est.update_at(
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

/// Drive a steady 100 MB/s, 1 file/s copy for 5 s, then leave it parked for
/// five minutes with `human_wait` deciding whether those minutes were a
/// person's or the transfer's, and take one more second of the same steady
/// copy afterwards. Returns the stats from that last sample.
fn steady_copy_across_a_five_minute_wait(human_wait: bool) -> EtaStats {
    const PARK_MS: u64 = 300_000;
    let start = Instant::now();
    let mut est = EtaEstimator::new();
    let bytes_total = 10_000_000_000_u64;
    let files_total = 100_usize;

    for i in 0..=5 {
        est.update(EtaSample {
            now: at(start, i * 1000),
            human_wait_total: Duration::ZERO,
            phase: WriteOperationPhase::Copying,
            bytes_done: i * 100_000_000,
            bytes_total,
            files_done: i as usize,
            files_total,
        });
    }

    // Nothing is emitted while the operation is parked (a paused transfer
    // and one waiting on an answer both go quiet), so the wait shows up as
    // one long gap before the next sample.
    let waited = if human_wait {
        Duration::from_millis(PARK_MS)
    } else {
        Duration::ZERO
    };
    est.update(EtaSample {
        now: at(start, 5_000 + PARK_MS + 1_000),
        human_wait_total: waited,
        phase: WriteOperationPhase::Copying,
        bytes_done: 600_000_000,
        bytes_total,
        files_done: 6,
        files_total,
    })
}

/// The assertion the whole human-wait exclusion exists for: a transfer that
/// spent five minutes waiting for a person to answer a conflict prompt (or
/// sat paused) comes back with the estimate it had, rather than one that
/// counted the thinking as dead-slow copying.
#[test]
fn a_human_wait_leaves_the_rate_and_the_eta_where_they_were() {
    let after = steady_copy_across_a_five_minute_wait(true);

    // 9.4 GB left at 100 MB/s ≈ 94 s. Pre-fix this read ~400 h, because the
    // 100 MB moved across the gap was divided by the five minutes the
    // prompt was open.
    let eta = after.eta_seconds.expect("warmed up long before the wait");
    assert!(
        (80..=110).contains(&eta),
        "eta = {eta}s after a human wait: the estimate must survive the answer, not jump",
    );
    assert!(
        (90_000_000..=110_000_000).contains(&after.bytes_per_second),
        "bytes_per_second = {} after a human wait: expected the steady ~100 MB/s",
        after.bytes_per_second,
    );
}

/// The negative control, so the exclusion can't quietly grow to cover
/// everything: five minutes of a slow share is the TRANSFER being slow, and
/// the ETA has to say so.
#[test]
fn a_device_wait_still_moves_the_eta() {
    let after = steady_copy_across_a_five_minute_wait(false);

    let eta = after.eta_seconds.expect("warmed up");
    assert!(
        eta > 1_000,
        "eta = {eta}s: five minutes for 100 MB is a genuinely slow transfer and the ETA must reflect it",
    );
    assert!(
        after.bytes_per_second < 10_000_000,
        "bytes_per_second = {}: the rate must follow a real slowdown down",
        after.bytes_per_second,
    );
}

#[test]
fn the_warm_up_gate_measures_working_time_not_wall_time() {
    // Paused 10 s into the first 200 ms of copying: the ETA must still be
    // withheld, because 200 ms of measured work is exactly what the gate
    // exists to distrust.
    let start = Instant::now();
    let mut est = EtaEstimator::new();
    est.update(EtaSample {
        now: at(start, 0),
        human_wait_total: Duration::ZERO,
        phase: WriteOperationPhase::Copying,
        bytes_done: 0,
        bytes_total: 10_000_000,
        files_done: 0,
        files_total: 100,
    });
    let stats = est.update(EtaSample {
        now: at(start, 10_200),
        human_wait_total: Duration::from_secs(10),
        phase: WriteOperationPhase::Copying,
        bytes_done: 2_000_000,
        bytes_total: 10_000_000,
        files_done: 20,
        files_total: 100,
    });

    assert_eq!(stats.eta_seconds, None, "200 ms of work is not a warmed-up estimator");
    assert!(
        stats.bytes_per_second > 0,
        "the rates still populate on the first delta"
    );
}

#[test]
fn phase_transition_resets_state() {
    let start = Instant::now();
    let mut est = EtaEstimator::new();

    // Scanning phase: 1000 files/s.
    est.update_at(at(start, 0), WriteOperationPhase::Scanning, 0, 0, 0, 0);
    est.update_at(at(start, 1000), WriteOperationPhase::Scanning, 0, 0, 1000, 0);
    est.update_at(at(start, 2000), WriteOperationPhase::Scanning, 0, 0, 2000, 0);

    // Transition to Copying: bytes_done resets to 0 from scanning's 0,
    // but the file count is fresh. files_done starts back at 0 in the
    // emitter's view of "files copied so far" (vs "files scanned").
    let on_transition = est.update_at(at(start, 2100), WriteOperationPhase::Copying, 0, 5_000_000_000, 0, 2000);
    // Reset → zero stats on the transition sample, then re-warm.
    assert_eq!(on_transition, EtaStats::ZERO);
}

#[test]
fn rollback_phase_computes_eta_toward_zero() {
    let start = Instant::now();
    let mut est = EtaEstimator::new();

    // Operation made it to 500 MB / 50 files before rollback starts.
    // During rollback, the counters decrease.
    est.update_at(
        at(start, 0),
        WriteOperationPhase::RollingBack,
        500_000_000,
        1_000_000_000,
        50,
        100,
    );
    est.update_at(
        at(start, 1000),
        WriteOperationPhase::RollingBack,
        400_000_000,
        1_000_000_000,
        40,
        100,
    );
    let stats = est.update_at(
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
    est.update_at(at(start, 0), WriteOperationPhase::Copying, 0, 1_000, 0, 10);
    est.update_at(at(start, 1000), WriteOperationPhase::Copying, 500, 1_000, 5, 10);
    let a = est.update_at(at(start, 2000), WriteOperationPhase::Copying, 700, 1_000, 7, 10);
    let b = est.update_at(at(start, 2000), WriteOperationPhase::Copying, 800, 1_000, 8, 10);
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
