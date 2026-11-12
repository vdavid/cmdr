//! The estimate against a simulated device whose true costs are known: every
//! file costs a fixed overhead plus its bytes at a fixed speed, and, like the
//! engine (`cmdr_fs::durable::DeviceBarriers`), a file whose note lands 2 s or
//! more after the last pass pays the next one before it counts as done: the
//! sync of every file written since the last pass (where those wait for it),
//! then the device barrier. The simulation reports the barrier time so far, a
//! running barrier included, and leaves the syncs out, as the engine does
//! (`DeviceBarriers::time_spent`). Each test drives the estimator at the 200 ms
//! emit cadence and compares `eta_seconds` with the time the simulation really
//! has left, and `bottleneck` with the axis the remaining time goes to.

use super::*;

/// The engine's barrier interval (`cmdr_fs::durable::BARRIER_INTERVAL`).
const BARRIER_EVERY_SECS: f64 = 2.0;

/// What one barrier costs, as measured for #1145: ~62 ms on a spinning disk,
/// ~12 ms on a disk image.
const SPINNING_DISK_BARRIER: f64 = 0.062;
const DISK_IMAGE_BARRIER: f64 = 0.012;

/// Every file costs `secs_per_file`, then streams its bytes at
/// `bytes_per_second`, then pays the pass when one is due: `barrier_secs`
/// plus `pass_secs_per_file` for each file the pass syncs.
struct Workload {
    sizes: Vec<u64>,
    secs_per_file: f64,
    bytes_per_second: f64,
    /// When each pass's barrier starts and ends: the stretch the engine counts
    /// in `time_spent`. The syncs before it are each file's own cost, which
    /// the cost model has to see.
    barriers: Vec<(f64, f64)>,
    /// When each file starts and completes, and the bytes of every file
    /// before it.
    starts: Vec<f64>,
    ends: Vec<f64>,
    bytes_before: Vec<u64>,
}

impl Workload {
    /// Each file's sync is part of its own cost: a pass is the barrier alone.
    fn new(sizes: Vec<u64>, secs_per_file: f64, bytes_per_second: f64, barrier_secs: f64) -> Self {
        Self::batched(sizes, secs_per_file, bytes_per_second, barrier_secs, 0.0)
    }

    /// Each file's sync waits for the pass, which pays `pass_secs_per_file`
    /// for each file written since the last one (a local disk on macOS).
    fn batched(
        sizes: Vec<u64>,
        secs_per_file: f64,
        bytes_per_second: f64,
        barrier_secs: f64,
        pass_secs_per_file: f64,
    ) -> Self {
        let mut starts = Vec::with_capacity(sizes.len());
        let mut ends = Vec::with_capacity(sizes.len());
        let mut bytes_before = Vec::with_capacity(sizes.len());
        let mut barriers = Vec::new();
        let (mut clock, mut bytes, mut last_pass, mut waiting) = (0.0, 0u64, 0.0, 0usize);
        for &size in &sizes {
            starts.push(clock);
            bytes_before.push(bytes);
            clock += secs_per_file + size as f64 / bytes_per_second;
            waiting += 1;
            if clock - last_pass >= BARRIER_EVERY_SECS {
                clock += waiting as f64 * pass_secs_per_file;
                let barrier_start = clock;
                clock += barrier_secs;
                barriers.push((barrier_start, clock));
                last_pass = clock;
                waiting = 0;
            }
            ends.push(clock);
            bytes += size;
        }
        Self {
            sizes,
            secs_per_file,
            bytes_per_second,
            barriers,
            starts,
            ends,
            bytes_before,
        }
    }

    fn bytes_total(&self) -> u64 {
        self.sizes.iter().sum()
    }

    /// `(bytes_done, files_done)` at a second into the run.
    fn counters_at(&self, at: f64) -> (u64, usize) {
        let files_done = self.ends.partition_point(|&end| end <= at);
        let Some(&size) = self.sizes.get(files_done) else {
            return (self.bytes_total(), files_done);
        };
        let streaming = (at - self.starts[files_done] - self.secs_per_file).max(0.0);
        let partial = ((streaming * self.bytes_per_second) as u64).min(size);
        (self.bytes_before[files_done] + partial, files_done)
    }

    /// The barrier time the engine has counted at a second into the run, a
    /// running barrier up to that second included.
    fn barrier_time_at(&self, at: f64) -> Duration {
        let secs: f64 = self
            .barriers
            .iter()
            .take_while(|(start, _)| *start < at)
            .map(|(start, end)| end.min(at) - start)
            .sum();
        Duration::from_secs_f64(secs)
    }

    fn finish(&self) -> f64 {
        *self.ends.last().expect("a workload has files")
    }
}

/// One emit: the second into the run, the stats it got, and what was really
/// left.
struct Reading {
    at: f64,
    stats: EtaStats,
    truth: f64,
    bytes_left: u64,
}

fn drive(workload: &Workload, until_secs: f64) -> Vec<Reading> {
    let start = Instant::now();
    let mut est = EtaEstimator::new();
    let finish = workload.finish();
    let bytes_total = workload.bytes_total();
    let files_total = workload.sizes.len();
    (0..=(until_secs * 5.0) as u64)
        .map(|step| {
            let at = step as f64 * 0.2;
            let (bytes_done, files_done) = workload.counters_at(at);
            let stats = est.update(EtaSample {
                now: start + Duration::from_millis(step * 200),
                human_wait_total: Duration::ZERO,
                barrier_total: workload.barrier_time_at(at),
                phase: WriteOperationPhase::Copying,
                reversal_bar: ReversalBar::Drains,
                bytes_done,
                bytes_total,
                files_done,
                files_total,
            });
            Reading {
                at,
                stats,
                truth: finish - at,
                bytes_left: bytes_total - bytes_done,
            }
        })
        .collect()
}

/// Every reading in `[from, to)` estimates within `tolerance` of the truth,
/// give or take the 1 s the whole-second rounding can add.
fn assert_tracks(readings: &[Reading], from: f64, to: f64, tolerance: f64) {
    for r in readings.iter().filter(|r| (from..to).contains(&r.at)) {
        let eta = f64::from(r.stats.eta_seconds.expect("warmed up"));
        assert!(
            (eta - r.truth).abs() <= r.truth * tolerance + 1.0,
            "at {:.1} s the estimate said {eta} s, the truth is {:.0} s",
            r.at,
            r.truth,
        );
    }
}

/// The predicted finish (`now + eta`) moves by at most `spread` of the time
/// left across `[from, to)`: no wobble on the 2 s barrier.
fn assert_steady(readings: &[Reading], from: f64, to: f64, spread: f64) {
    let window: Vec<&Reading> = readings.iter().filter(|r| (from..to).contains(&r.at)).collect();
    let (low, high) = window
        .iter()
        .map(|r| r.at + f64::from(r.stats.eta_seconds.expect("warmed up")))
        .fold((f64::MAX, f64::MIN), |(lo, hi), f| (lo.min(f), hi.max(f)));
    let truth = window.first().expect("readings in the window").truth;
    assert!(
        high - low <= truth * spread + 1.0,
        "the predicted finish moved {:.0} s over a window whose truth is {truth:.0} s",
        high - low
    );
}

fn tiny_files_then_big(lead_in: usize) -> Workload {
    let mut sizes = vec![8_000_000; lead_in];
    sizes.extend(std::iter::repeat_n(1_000, 12_000));
    sizes.extend([500_000_000; 3]);
    Workload::new(sizes, 0.0625, 30_000_000.0, SPINNING_DISK_BARRIER)
}

/// The reported case (#1146): deep in a folder of ~1 kB files at a steady
/// ~16 files/s, with 1.5 GB of bigger files still ahead, the byte rate has
/// collapsed to kilobytes per second. Remaining bytes over that rate said
/// "hours" on about 13 minutes of work.
#[test]
fn a_run_of_tiny_files_estimates_from_the_files_left() {
    let readings = drive(&tiny_files_then_big(300), 175.0);
    let deep = readings
        .iter()
        .find(|r| r.at >= 165.0)
        .expect("a reading deep in the run");
    let bytes_alone = deep.bytes_left as f64 / deep.stats.bytes_per_second as f64;
    assert!(
        bytes_alone > 10.0 * deep.truth,
        "the shape needs a collapsed byte rate: {bytes_alone:.0} s"
    );
    assert_tracks(&readings, 115.0, 175.0, 0.1);
    assert_steady(&readings, 130.0, 175.0, 0.04);
    // The folder bars follow the file count racing ahead, ❌ not the bytes
    // still ahead over a collapsed byte rate (#1149).
    assert_axis(&readings, 115.0, 175.0, ProgressAxis::Files);
}

/// Every reading in `[from, to)` reports `axis` as the bottleneck.
fn assert_axis(readings: &[Reading], from: f64, to: f64, axis: ProgressAxis) {
    for r in readings.iter().filter(|r| (from..to).contains(&r.at)) {
        assert_eq!(r.stats.bottleneck, axis, "at {:.1} s", r.at);
    }
}

/// Same run with no big files first, so nothing has measured what a byte
/// costs yet: the prior prices the bytes left, and the file count still decides.
#[test]
fn a_run_of_tiny_files_from_the_start_estimates_from_the_files_left() {
    let readings = drive(&tiny_files_then_big(0), 60.0);
    assert_tracks(&readings, 15.0, 60.0, 0.1);
    assert_axis(&readings, 15.0, 60.0, ProgressAxis::Files);
}

/// Big files stream at the device's speed: the estimate is the bytes left
/// over the byte rate, as it always was, barrier and all, and the bars fill
/// by bytes, not in four jumps of a file each.
#[test]
fn big_files_estimate_from_the_bytes_left() {
    let workload = Workload::new(vec![2_000_000_000; 4], 0.005, 100_000_000.0, SPINNING_DISK_BARRIER);
    let readings = drive(&workload, 80.0);
    assert_tracks(&readings, 5.0, 80.0, 0.05);
    assert_steady(&readings, 10.0, 80.0, 0.03);
    assert_axis(&readings, 1.0, 80.0, ProgressAxis::Bytes);
}

/// Big files first, then a tail of small ones. During the big files the file
/// count alone (2,000 files left at one per 2 s) said over an hour on about a
/// minute of work; each half of the remaining work needs its own price.
///
/// A big file takes longer than the barrier interval, so every one of them
/// pays a barrier. Learned as a per-file cost, that priced each small file
/// still ahead a barrier too high: +96 % on a disk image, +435 % on a spinning
/// disk (#1149). The barrier time stays out of what the costs learn.
#[test]
fn big_files_then_small_ones_price_each_half_of_the_work() {
    for barrier in [DISK_IMAGE_BARRIER, SPINNING_DISK_BARRIER] {
        let mut sizes = vec![100_000_000; 20];
        sizes.extend(std::iter::repeat_n(4_000, 2_000));
        let workload = Workload::new(sizes, 0.01, 50_000_000.0, barrier);
        let readings = drive(&workload, 60.0);
        assert_tracks(&readings, 5.0, 40.0, 0.1);
        assert_tracks(&readings, 42.0, 60.0, 0.1);
        assert_axis(&readings, 42.0, 60.0, ProgressAxis::Files);
    }
}

/// Big and small files interleaved: the estimate lands between the two axes.
#[test]
fn interleaved_big_and_small_files_land_in_between() {
    let sizes = (0..50)
        .flat_map(|_| std::iter::once(40_000_000).chain(std::iter::repeat_n(4_000, 100)))
        .collect();
    let workload = Workload::new(sizes, 0.01, 50_000_000.0, DISK_IMAGE_BARRIER);
    let readings = drive(&workload, 80.0);
    assert_tracks(&readings, 10.0, 80.0, 0.15);
}

/// The tiny-file run with each file's sync waiting for the pass (a local disk
/// on macOS): 5 of a file's 62.5 ms paid there, a guess for a spinning disk,
/// which nobody has measured yet. The pass's syncs stay OUT of `time_spent`:
/// they're each file's own cost, which the cost model has to see. Counted as
/// barrier time, this simulation priced the files left at half their cost
/// when the syncs were most of it (50 of 62.5 ms), and the next test's big
/// half 16 % short.
///
/// The trade it pins: the syncs arrive as one lump every 2 s, so the predicted
/// finish wobbles more than with a sync per file, up to ~10 % of the time left
/// here against the 4 % above.
#[test]
fn a_run_of_tiny_files_whose_syncs_wait_for_the_pass_estimates_from_the_files_left() {
    let mut sizes = vec![8_000_000; 300];
    sizes.extend(std::iter::repeat_n(1_000, 12_000));
    sizes.extend([500_000_000; 3]);
    let workload = Workload::batched(sizes, 0.0575, 30_000_000.0, SPINNING_DISK_BARRIER, 0.005);
    let readings = drive(&workload, 175.0);
    assert_tracks(&readings, 115.0, 175.0, 0.1);
    assert_steady(&readings, 130.0, 175.0, 0.1);
    assert_axis(&readings, 115.0, 175.0, ProgressAxis::Files);
}

/// Big files then small ones, with the small files' syncs (4 of their 10 ms)
/// waiting for the pass. The big half prices as before. The small half's syncs
/// show up only once its first pass has run, and then in lumps, so its
/// estimate runs up to ~30 % short of the 10 % a sync per file holds: the
/// cost of batching on a workload where the syncs are a large share.
#[test]
fn big_files_then_small_ones_with_batched_syncs_price_each_half_of_the_work() {
    for barrier in [DISK_IMAGE_BARRIER, SPINNING_DISK_BARRIER] {
        let mut sizes = vec![100_000_000; 20];
        sizes.extend(std::iter::repeat_n(4_000, 2_000));
        let workload = Workload::batched(sizes, 0.006, 50_000_000.0, barrier, 0.004);
        let readings = drive(&workload, 60.0);
        assert_tracks(&readings, 5.0, 40.0, 0.1);
        assert_tracks(&readings, 45.0, 60.0, 0.3);
        assert_axis(&readings, 42.0, 60.0, ProgressAxis::Files);
    }
}
