//! Per-directory read-latency observability for the serial local reconcile walk.
//!
//! Read-only measurement: it writes no index state and changes no behaviour. It
//! wraps every [`GuardedReader::read`](super::GuardedReader::read) with a
//! wall-clock timer and aggregates a duration histogram, the timeout count, the
//! slowest directories by path, and a synthetic-vs-real split (File Provider
//! roots list far slower than local disk, and the walk pays that serially).
//!
//! Off unless `CMDR_RECONCILE_LATENCY_SPIKE` is truthy: [`LatencyProbe::from_env`]
//! returns `None` and the reader's `Option` stays `None`, so a normal run pays
//! nothing.
//!
//! Shape follows `watch/churn_monitor.rs`: pure aggregation with the clock injected per
//! call, so the whole engine is unit-testable without a filesystem or a running
//! app.

use std::path::Path;
use std::time::{Duration, Instant};

/// Default rollup period: frequent enough to watch a long walk live, rare enough
/// that emission is free.
const DEFAULT_PERIOD: Duration = Duration::from_secs(30);

/// Default number of slowest directories kept and emitted.
const DEFAULT_TOP_N: usize = 30;

/// Log target for every line this module emits.
const LOG_TARGET: &str = "indexing::reconcile_latency";

/// Upper edges of the histogram buckets, in microseconds. The last bucket is
/// everything at or above the final edge (a 15 s timeout lands there).
const BUCKET_EDGES_US: [u128; 9] = [
    1_000,      // <1ms
    5_000,      // 1-5ms
    10_000,     // 5-10ms
    50_000,     // 10-50ms
    100_000,    // 50-100ms
    500_000,    // 100-500ms
    1_000_000,  // 500ms-1s
    5_000_000,  // 1-5s
    15_000_000, // 5-15s
];

/// Human labels for the buckets, parallel to [`BUCKET_EDGES_US`] plus the overflow.
const BUCKET_LABELS: [&str; 10] = [
    "<1ms",
    "1-5ms",
    "5-10ms",
    "10-50ms",
    "50-100ms",
    "100-500ms",
    "500ms-1s",
    "1-5s",
    "5-15s",
    ">=15s",
];

/// Home-relative prefixes treated as synthetic (File Provider / cloud-sync)
/// filesystems, whose listings go through an XPC round trip rather than the local
/// disk.
const SYNTHETIC_SUFFIXES: [&str; 4] = [
    "Library/Containers/com.google.drivefs.fpext",
    "Library/CloudStorage",
    "Dropbox",
    "Library/Mobile Documents",
];

/// Running totals for one class of paths (synthetic or real).
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
struct ClassTotals {
    reads: u64,
    total_us: u128,
    timeouts: u64,
}

impl ClassTotals {
    fn record(&mut self, dur: Duration, timed_out: bool) {
        self.reads += 1;
        self.total_us += dur.as_micros();
        if timed_out {
            self.timeouts += 1;
        }
    }

    fn mean_ms(&self) -> f64 {
        if self.reads == 0 {
            return 0.0;
        }
        self.total_us as f64 / self.reads as f64 / 1000.0
    }
}

/// Aggregates per-directory read latency across a whole reconcile walk.
pub(super) struct LatencyProbe {
    period: Duration,
    top_n: usize,
    started: Instant,
    last_report: Instant,
    buckets: [u64; 10],
    synthetic: ClassTotals,
    real: ClassTotals,
    /// Slowest reads seen so far, kept sorted slowest-first and truncated to
    /// `top_n`. A tiny sorted vec beats a heap at this size and keeps emission
    /// trivial.
    slowest: Vec<(Duration, String)>,
    /// Absolute synthetic roots, resolved once from the home directory.
    synthetic_roots: Vec<String>,
}

impl LatencyProbe {
    /// Build a probe if `CMDR_RECONCILE_LATENCY_SPIKE` is truthy, else `None`.
    ///
    /// `CMDR_RECONCILE_LATENCY_SPIKE_PERIOD_S` and `..._TOP_N` override the
    /// defaults; an unparseable value falls back rather than failing a launch.
    pub(super) fn from_env(now: Instant) -> Option<Self> {
        let enabled = std::env::var("CMDR_RECONCILE_LATENCY_SPIKE")
            .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        if !enabled {
            return None;
        }
        let period = std::env::var("CMDR_RECONCILE_LATENCY_SPIKE_PERIOD_S")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|s| *s > 0)
            .map_or(DEFAULT_PERIOD, Duration::from_secs);
        let top_n = std::env::var("CMDR_RECONCILE_LATENCY_SPIKE_TOP_N")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_TOP_N);
        let roots = default_synthetic_roots();
        log::info!(
            target: LOG_TARGET,
            "reconcile_latency_spike_enabled period_s={} top_n={top_n} synthetic_roots={}",
            period.as_secs(),
            roots.join(","),
        );
        Some(Self::new(period, top_n, roots, now))
    }

    /// Build a probe with explicit settings. Tests use this.
    pub(super) fn new(period: Duration, top_n: usize, synthetic_roots: Vec<String>, now: Instant) -> Self {
        Self {
            period,
            top_n,
            started: now,
            last_report: now,
            buckets: [0; 10],
            synthetic: ClassTotals::default(),
            real: ClassTotals::default(),
            slowest: Vec::new(),
            synthetic_roots,
        }
    }

    /// Record one directory read, then emit a rollup if the period elapsed.
    pub(super) fn record(&mut self, path: &Path, dur: Duration, timed_out: bool, now: Instant) {
        self.buckets[bucket_index(dur)] += 1;
        let path_str = path.to_string_lossy();
        if self.is_synthetic(&path_str) {
            self.synthetic.record(dur, timed_out);
        } else {
            self.real.record(dur, timed_out);
        }
        self.note_slow(dur, &path_str);
        if now.duration_since(self.last_report) >= self.period {
            self.last_report = now;
            self.emit("periodic", now);
        }
    }

    /// Emit the final rollup. Called when the walk's reader is dropped.
    pub(super) fn finish(&self, now: Instant) {
        self.emit("final", now);
    }

    fn is_synthetic(&self, path: &str) -> bool {
        self.synthetic_roots.iter().any(|root| path.starts_with(root.as_str()))
    }

    fn note_slow(&mut self, dur: Duration, path: &str) {
        if self.slowest.len() >= self.top_n && self.slowest.last().is_some_and(|(d, _)| *d >= dur) {
            return;
        }
        let at = self.slowest.partition_point(|(d, _)| *d > dur);
        self.slowest.insert(at, (dur, path.to_string()));
        self.slowest.truncate(self.top_n);
    }

    fn emit(&self, kind: &str, now: Instant) {
        let reads = self.synthetic.reads + self.real.reads;
        let wall = now.duration_since(self.started);
        let dirs_per_s = if wall.as_secs_f64() > 0.0 {
            reads as f64 / wall.as_secs_f64()
        } else {
            0.0
        };
        let in_read_us = self.synthetic.total_us + self.real.total_us;
        let timeouts = self.synthetic.timeouts + self.real.timeouts;
        log::info!(
            target: LOG_TARGET,
            "reconcile_latency {kind} reads={reads} wall_s={:.1} in_read_s={:.1} dirs_per_s={dirs_per_s:.1} timeouts={timeouts}",
            wall.as_secs_f64(),
            in_read_us as f64 / 1e6,
        );
        let hist = BUCKET_LABELS
            .iter()
            .zip(self.buckets.iter())
            .map(|(label, count)| format!("{label}={count}"))
            .collect::<Vec<_>>()
            .join(" ");
        log::info!(target: LOG_TARGET, "reconcile_latency {kind} hist {hist}");
        log::info!(
            target: LOG_TARGET,
            "reconcile_latency {kind} split synthetic_reads={} synthetic_mean_ms={:.2} synthetic_total_s={:.1} synthetic_timeouts={} real_reads={} real_mean_ms={:.2} real_total_s={:.1} real_timeouts={}",
            self.synthetic.reads,
            self.synthetic.mean_ms(),
            self.synthetic.total_us as f64 / 1e6,
            self.synthetic.timeouts,
            self.real.reads,
            self.real.mean_ms(),
            self.real.total_us as f64 / 1e6,
            self.real.timeouts,
        );
        for (rank, (dur, path)) in self.slowest.iter().enumerate() {
            log::info!(
                target: LOG_TARGET,
                "reconcile_latency {kind} slow #{rank} ms={:.1} path={path}",
                dur.as_secs_f64() * 1000.0,
            );
        }
    }
}

/// The absolute synthetic roots for this machine, or an empty list when there's
/// no home directory (everything then counts as real, which is honest).
fn default_synthetic_roots() -> Vec<String> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    SYNTHETIC_SUFFIXES
        .iter()
        .map(|suffix| home.join(suffix).to_string_lossy().to_string())
        .collect()
}

/// The histogram slot a duration falls into.
fn bucket_index(dur: Duration) -> usize {
    let us = dur.as_micros();
    BUCKET_EDGES_US.iter().position(|edge| us < *edge).unwrap_or(9)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(now: Instant) -> LatencyProbe {
        LatencyProbe::new(Duration::from_secs(30), 3, vec!["/Users/x/Dropbox".to_string()], now)
    }

    #[test]
    fn buckets_split_at_their_edges() {
        assert_eq!(bucket_index(Duration::from_micros(999)), 0);
        assert_eq!(bucket_index(Duration::from_micros(1_000)), 1);
        assert_eq!(bucket_index(Duration::from_millis(9)), 2);
        assert_eq!(bucket_index(Duration::from_millis(49)), 3);
        assert_eq!(bucket_index(Duration::from_millis(99)), 4);
        assert_eq!(bucket_index(Duration::from_millis(499)), 5);
        assert_eq!(bucket_index(Duration::from_millis(999)), 6);
        assert_eq!(bucket_index(Duration::from_secs(4)), 7);
        assert_eq!(bucket_index(Duration::from_secs(14)), 8);
        assert_eq!(bucket_index(Duration::from_secs(15)), 9);
    }

    #[test]
    fn synthetic_and_real_totals_stay_separate() {
        let now = Instant::now();
        let mut p = probe(now);
        p.record(Path::new("/Users/x/Dropbox/a"), Duration::from_millis(100), false, now);
        p.record(Path::new("/Users/x/code"), Duration::from_millis(2), false, now);
        assert_eq!(p.synthetic.reads, 1);
        assert_eq!(p.real.reads, 1);
        assert!((p.synthetic.mean_ms() - 100.0).abs() < 0.01);
        assert!((p.real.mean_ms() - 2.0).abs() < 0.01);
    }

    #[test]
    fn slowest_keeps_the_top_n_in_order() {
        let now = Instant::now();
        let mut p = probe(now);
        for (name, ms) in [("a", 5), ("b", 900), ("c", 30), ("d", 400), ("e", 1)] {
            p.record(Path::new(name), Duration::from_millis(ms), false, now);
        }
        let paths: Vec<&str> = p.slowest.iter().map(|(_, path)| path.as_str()).collect();
        assert_eq!(paths, vec!["b", "d", "c"]);
    }

    #[test]
    fn timeouts_are_counted_per_class() {
        let now = Instant::now();
        let mut p = probe(now);
        p.record(Path::new("/Users/x/Dropbox/a"), Duration::from_secs(15), true, now);
        assert_eq!(p.synthetic.timeouts, 1);
        assert_eq!(p.real.timeouts, 0);
        assert_eq!(p.buckets[9], 1);
    }
}
