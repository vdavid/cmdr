//! Per-subtree churn observability for the live FSEvents loop: which directories churn, how hard, and how that rolls up an ancestor chain.
//!
//! Read-only measurement: it writes no index state, sends no writer messages,
//! and changes no behaviour. It watches the live loop's already-deduplicated
//! batch, rolls each changed path's churn up its ancestor chain, and emits one
//! structured rollup per period so a script can turn hours of logs into a time
//! series (`docs/notes/churn-observability-spike.md`).
//!
//! Off unless `CMDR_CHURN_SPIKE` is truthy: [`ChurnMonitor::from_env`] returns
//! `None`, and the live loop's `Option` stays `None`, so a normal run pays
//! nothing.
//!
//! Shape follows `reconciler/rescan_throttle.rs`: pure aggregation, clock
//! injected per call, no interior locking (the live loop owns it), so the whole
//! engine is unit-testable without a filesystem or a running app. That's also
//! what makes it promotable into the sealed-subtrees churn accounting rather
//! than throwaway: only the sink changes.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

/// Default rollup period. Long enough that emission is negligible (a handful of
/// lines per period), short enough to see a `cargo build` start and stop.
const DEFAULT_PERIOD: Duration = Duration::from_secs(30);

/// Default number of nodes emitted per period, ranked by rolled-up event count.
///
/// Ranking by rolled-up count means a hot node's ENTIRE ancestor chain always
/// ranks at or above it (an ancestor's count is a superset sum), so a top-N cut
/// never truncates a chain in the middle. That is what makes the ratio-drop
/// question answerable from the emitted subset alone.
const DEFAULT_TOP_N: usize = 40;

/// Hard cap on tracked directories within one period. Past it, new (deeper,
/// less interesting) nodes are dropped and counted; the walk is root-first, so
/// what survives is the shallow part of every chain, which keeps the rolled-up
/// totals honest.
const MAX_NODES: usize = 10_000;

/// Per-node cap on the exact distinct-churny-children set. Beyond it the count
/// saturates and `children_capped` is set: the analysis reads that as "≥ cap",
/// and uses `direct` for magnitude. Bounds worst-case memory to ~1 KB per node.
const CHILD_CAP: usize = 128;

/// Ancestor levels walked per path. Deeper components are ignored (and counted
/// as `deep_truncated`) so a pathological path can't turn one event into
/// hundreds of map operations.
const MAX_DEPTH: usize = 40;

/// Log target for every line this module emits.
const LOG_TARGET: &str = "indexing::churn";

/// One tracked directory's churn within the current period.
#[derive(Default)]
struct Node {
    /// Events anywhere at or under this directory (the rolled-up count).
    events: u64,
    /// Events whose containing directory is exactly this one.
    direct: u64,
    /// Hashes of the distinct direct children (by name) that saw churn.
    /// Hashes, not names: the analysis only needs the cardinality, and 8 bytes
    /// per child bounds memory regardless of name length.
    children: HashSet<u64>,
    /// Whether `children` hit [`CHILD_CAP`] (so its length is a floor).
    children_capped: bool,
}

/// One node's line in a period's report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::indexing) struct ChurnNodeReport {
    pub(in crate::indexing) path: String,
    pub(in crate::indexing) events: u64,
    pub(in crate::indexing) direct: u64,
    pub(in crate::indexing) distinct_children: usize,
    pub(in crate::indexing) children_capped: bool,
}

/// One period's aggregated churn, ready to emit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::indexing) struct ChurnReport {
    /// Monotonic period number for this monitor, joining node lines to their
    /// period line in the log.
    pub(in crate::indexing) seq: u64,
    /// Actual elapsed period length (ticks drift, so this is measured).
    pub(in crate::indexing) period_ms: u128,
    /// Raw FSEvents received in the period, before per-path dedup.
    pub(in crate::indexing) raw_events: u64,
    /// Deduplicated paths observed in the period (what the rollup counts).
    pub(in crate::indexing) batch_paths: u64,
    /// Distinct directories tracked in the period.
    pub(in crate::indexing) node_count: usize,
    /// Nodes not created because [`MAX_NODES`] was reached.
    pub(in crate::indexing) nodes_dropped: u64,
    /// Paths whose chain was cut at [`MAX_DEPTH`].
    pub(in crate::indexing) deep_truncated: u64,
    /// The top nodes by rolled-up event count, count-desc then path-asc.
    pub(in crate::indexing) top: Vec<ChurnNodeReport>,
}

/// Aggregates per-subtree churn and emits one rollup per period.
pub(in crate::indexing) struct ChurnMonitor {
    period: Duration,
    top_n: usize,
    period_start: Instant,
    seq: u64,
    nodes: HashMap<String, Node>,
    raw_events: u64,
    batch_paths: u64,
    nodes_dropped: u64,
    deep_truncated: u64,
}

impl ChurnMonitor {
    /// Build a monitor if `CMDR_CHURN_SPIKE` is truthy, else `None`.
    ///
    /// `CMDR_CHURN_SPIKE_PERIOD_S` and `CMDR_CHURN_SPIKE_TOP_N` override the
    /// defaults; an unparseable value falls back rather than failing a launch.
    pub(in crate::indexing) fn from_env(now: Instant) -> Option<Self> {
        let enabled = std::env::var("CMDR_CHURN_SPIKE")
            .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        if !enabled {
            return None;
        }
        let period = std::env::var("CMDR_CHURN_SPIKE_PERIOD_S")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|s| *s > 0)
            .map_or(DEFAULT_PERIOD, Duration::from_secs);
        let top_n = std::env::var("CMDR_CHURN_SPIKE_TOP_N")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_TOP_N);
        log::info!(
            target: LOG_TARGET,
            "churn_spike_enabled period_s={} top_n={top_n} max_nodes={MAX_NODES} child_cap={CHILD_CAP}",
            period.as_secs(),
        );
        Some(Self::new(period, top_n, now))
    }

    /// Build a monitor with an explicit period and top-N. Tests use this.
    pub(in crate::indexing) fn new(period: Duration, top_n: usize, now: Instant) -> Self {
        Self {
            period,
            top_n,
            period_start: now,
            seq: 0,
            nodes: HashMap::new(),
            raw_events: 0,
            batch_paths: 0,
            nodes_dropped: 0,
            deep_truncated: 0,
        }
    }

    /// Fold one flush batch of deduplicated absolute paths into the period.
    ///
    /// `raw_events` is how many pre-dedup events the loop received since the
    /// last call, so the report can show the dedup ratio (a single log file
    /// rewritten 500×/s and 500 distinct temp files look identical after dedup,
    /// and the seal decision cares about the difference).
    pub(in crate::indexing) fn record_batch<'a>(&mut self, paths: impl Iterator<Item = &'a str>, raw_events: u64) {
        self.raw_events += raw_events;
        for path in paths {
            self.record_path(path);
        }
    }

    /// Credit one changed path to its containing directory and every ancestor.
    fn record_path(&mut self, path: &str) {
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        // A bare `/` names no child, so there is nothing to credit.
        let Some((_name, dirs)) = components.split_last() else {
            return;
        };
        self.batch_paths += 1;
        if dirs.len() + 1 > MAX_DEPTH {
            self.deep_truncated += 1;
        }

        // Walk root-first so an ancestor is always created before its
        // descendants: under the node cap, what survives is the shallow part of
        // each chain, whose rolled-up totals stay complete.
        let levels = (dirs.len() + 1).min(MAX_DEPTH);
        let mut key = String::with_capacity(path.len());
        for level in 0..levels {
            // `level` directories deep: key is `/` for level 0, `/a` for 1, …
            if level > 0 {
                key.push('/');
                key.push_str(dirs[level - 1]);
            }
            let node_key = if key.is_empty() { "/" } else { key.as_str() };
            // The next component down the chain — the direct child that churned.
            let child = components[level];
            if !self.nodes.contains_key(node_key) {
                if self.nodes.len() >= MAX_NODES {
                    self.nodes_dropped += 1;
                    continue;
                }
                self.nodes.insert(node_key.to_string(), Node::default());
            }
            let node = self.nodes.get_mut(node_key).expect("inserted directly above");
            node.events += 1;
            if level == dirs.len() {
                node.direct += 1;
            }
            if node.children.len() < CHILD_CAP {
                node.children.insert(hash_name(child));
            } else {
                node.children_capped = true;
            }
        }
    }

    /// Close the period and return its report if `now` reached the period end,
    /// resetting all accumulated state (which is what bounds memory: nothing
    /// survives a period).
    pub(in crate::indexing) fn rollup(&mut self, now: Instant) -> Option<ChurnReport> {
        let elapsed = now.saturating_duration_since(self.period_start);
        if elapsed < self.period {
            return None;
        }

        let mut ranked: Vec<ChurnNodeReport> = self
            .nodes
            .iter()
            .map(|(path, node)| ChurnNodeReport {
                path: path.clone(),
                events: node.events,
                direct: node.direct,
                distinct_children: node.children.len(),
                children_capped: node.children_capped,
            })
            .collect();
        // Count-desc, then path-asc. The path tie-break is load-bearing: it puts
        // an ancestor before a descendant that has the same count, so a top-N cut
        // can never keep a child while dropping its parent.
        ranked.sort_by(|a, b| b.events.cmp(&a.events).then_with(|| a.path.cmp(&b.path)));
        ranked.truncate(self.top_n);

        let report = ChurnReport {
            seq: self.seq,
            period_ms: elapsed.as_millis(),
            raw_events: self.raw_events,
            batch_paths: self.batch_paths,
            node_count: self.nodes.len(),
            nodes_dropped: self.nodes_dropped,
            deep_truncated: self.deep_truncated,
            top: ranked,
        };

        self.seq += 1;
        self.period_start = now;
        self.nodes.clear();
        self.nodes.shrink_to_fit();
        self.raw_events = 0;
        self.batch_paths = 0;
        self.nodes_dropped = 0;
        self.deep_truncated = 0;

        Some(report)
    }
}

/// The live batch's view of the monitor: the optional monitor plus the volume
/// it belongs to and the raw-event baseline it diffs against.
///
/// **This type exists to make forgetting the instrumentation impossible.**
/// `process_live_batch` takes one by `&mut`, and there is more than one live
/// loop (`live.rs` and `replay.rs` Phase 3 both drive live batches). Passing an
/// observer is therefore compiler-enforced at every live batch, present and
/// future; opting out has to be spelled out as [`ChurnObserver::disabled`].
pub(in crate::indexing) struct ChurnObserver {
    monitor: Option<ChurnMonitor>,
    volume_id: String,
    /// Cumulative raw events the owning loop had seen at the previous batch, so
    /// the per-period raw count is a diff of a counter the loop already keeps
    /// (no per-event work is added anywhere).
    last_raw_total: u64,
    /// Raw events accumulated since the last `observe`.
    pending_raw: u64,
}

impl ChurnObserver {
    /// An observer wired to the env-gated monitor. `None` inside (and free)
    /// unless `CMDR_CHURN_SPIKE` is truthy.
    pub(in crate::indexing) fn from_env(volume_id: &str, now: Instant) -> Self {
        Self {
            monitor: ChurnMonitor::from_env(now),
            volume_id: volume_id.to_string(),
            last_raw_total: 0,
            pending_raw: 0,
        }
    }

    /// An observer that never records. Test-only on purpose: production code
    /// has no way to opt out, so every live batch carries a real observer.
    #[cfg(test)]
    pub(in crate::indexing) fn disabled() -> Self {
        Self {
            monitor: None,
            volume_id: String::new(),
            last_raw_total: 0,
            pending_raw: 0,
        }
    }

    /// Supply the owning loop's cumulative raw-event count for this batch.
    /// Returns `&mut self` so the call reads as one expression at the
    /// `process_live_batch` call site, which is what keeps the count honest
    /// without a second thing to remember.
    pub(in crate::indexing) fn with_raw_total(&mut self, raw_total: u64) -> &mut Self {
        // Saturating: a loop that restarts its own counter must not underflow.
        self.pending_raw += raw_total.saturating_sub(self.last_raw_total);
        self.last_raw_total = raw_total;
        self
    }

    /// Fold one batch of deduplicated paths in, then emit if the period closed.
    /// Called from `process_live_batch` before the batch drains.
    pub(in crate::indexing) fn observe<'a>(&mut self, paths: impl Iterator<Item = &'a str>, now: Instant) {
        let raw = std::mem::take(&mut self.pending_raw);
        let Some(monitor) = self.monitor.as_mut() else {
            return;
        };
        monitor.record_batch(paths, raw);
        if let Some(report) = monitor.rollup(now) {
            report.log(&self.volume_id);
        }
    }

    #[cfg(test)]
    fn enabled_for_test(volume_id: &str, period: Duration, top_n: usize, now: Instant) -> Self {
        Self {
            monitor: Some(ChurnMonitor::new(period, top_n, now)),
            volume_id: volume_id.to_string(),
            last_raw_total: 0,
            pending_raw: 0,
        }
    }

    #[cfg(test)]
    fn take_report(&mut self, now: Instant) -> Option<ChurnReport> {
        self.monitor.as_mut().and_then(|m| m.rollup(now))
    }
}

/// Stable-within-a-process hash of a child name. Only cardinality is reported,
/// so collisions cost at most an undercount of one child in a 128-slot set.
fn hash_name(name: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish()
}

impl ChurnReport {
    /// Emit the report: one `churn_period` summary line plus one `churn_node`
    /// line per ranked node, joined by `seq`. Format is documented in
    /// `docs/notes/churn-observability-spike.md`; `path` stays LAST on the node
    /// line so a parser can take the rest of the line verbatim (paths contain
    /// spaces and `=`).
    pub(in crate::indexing) fn log(&self, volume_id: &str) {
        let t_ms = chrono::Local::now().timestamp_millis();
        log::debug!(
            target: LOG_TARGET,
            "churn_period seq={} t_ms={t_ms} vol={volume_id} period_ms={} raw_events={} batch_paths={} nodes={} nodes_dropped={} deep_truncated={} emitted={}",
            self.seq,
            self.period_ms,
            self.raw_events,
            self.batch_paths,
            self.node_count,
            self.nodes_dropped,
            self.deep_truncated,
            self.top.len(),
        );
        for node in &self.top {
            log::debug!(
                target: LOG_TARGET,
                "churn_node seq={} t_ms={t_ms} vol={volume_id} events={} direct={} children={} capped={} path={}",
                self.seq,
                node.events,
                node.direct,
                node.distinct_children,
                u8::from(node.children_capped),
                node.path,
            );
        }
    }
}

#[cfg(test)]
mod tests;
