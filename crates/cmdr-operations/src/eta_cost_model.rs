//! What one file costs against what one byte costs, learned from the
//! transfer's own history: `working time ≈ a · files + b · bytes`.
//!
//! The estimator in `eta.rs` uses only the RATIO of `a` to `b`, to weigh the
//! two halves of the remaining work against each other; its two short-memory
//! rates still decide how fast that work goes right now. So this model can
//! afford a long memory, and needs one: a run of tiny files says almost nothing
//! about the cost of a byte, and the byte cost it learned on the big files
//! before has to survive the run to price the big files after it.
//!
//! ## Recursive least squares with directional forgetting
//!
//! Each sample is the counters' movement over a stretch of working time:
//! `x = (Δfiles, ΔMiB)`, `t = Δsecs`. The model keeps the least-squares
//! information matrix `M` (Σ x xᵀ) and the estimate `θ = (a, b)`. Plain
//! exponential forgetting decays ALL of `M`, so a long run that only ever
//! moves files would wash out what the big files taught about bytes, and
//! leave `b` to noise. Directional forgetting (Kulhavý) forgets only along the
//! direction the new sample re-measures:
//!
//! - `M' = M − β · (M x)(M x)ᵀ / (xᵀ M x) + x xᵀ`
//! - `θ' = M'⁻¹ · ((M − β · (M x)(M x)ᵀ / (xᵀ M x)) · θ + x t)`
//!
//! with `β = 1 − exp(−span / τ)`, time-weighted like the rates' EWMA. The
//! forgetting term is positive semi-definite for `β ∈ [0, 1]`, and the prior
//! keeps `M` positive definite, so `M'` always inverts.
//!
//! ## The prior, and why bytes are counted in MiB
//!
//! A run whose every file is the same size can't separate the two costs: any
//! split along `a + b · size = t` fits. The weak isotropic prior (in files and
//! MiB) settles that split by which term dominates: kilobyte files land on
//! `a`, gigabyte files on `b`. 1 MiB is roughly where the two costs cross on a
//! local disk (a millisecond of open-close-sync per file, a millisecond per MiB
//! of streaming), which is what makes it the fair pivot. Data in any direction
//! outweighs the prior within one sample.

/// Bytes enter the model in MiB, the pivot between the two costs (module doc).
const BYTES_PER_UNIT: f64 = 1_048_576.0;

/// Memory of the cost estimate along whatever direction the samples keep
/// re-measuring. Longer than the rates' 3 s: the costs are the device's, and
/// they change far less often than its speed does.
const COST_TAU_SECS: f64 = 10.0;

/// Where the costs start: 5 ms per file, 10 ms per MiB (100 MiB/s). Only the
/// direction the data never reaches keeps it.
const PRIOR_SECS_PER_FILE: f64 = 0.005;
const PRIOR_SECS_PER_MIB: f64 = 0.01;

/// How much the prior weighs: as much as one sample that moved one file, or
/// one MiB. Any real stream of either outweighs it within a second, while a
/// run of kilobyte files (a few KiB per emit, so a millionth of that weight
/// each) can't drag the byte cost around with its timing noise. Weighed at
/// 1e-3, that noise priced the bytes left in a tiny-file run at 15x their cost.
const PRIOR_WEIGHT: f64 = 1.0;

#[derive(Debug, Clone, Copy)]
pub(super) struct CostModel {
    /// The information matrix, symmetric: `[[ff, fb], [fb, bb]]`.
    info: [[f64; 2]; 2],
    /// Seconds per file, seconds per MiB. Unconstrained here; the accessors
    /// clamp at zero.
    cost: [f64; 2],
    /// Movement not yet folded in, until it spans `min_span` of working time.
    pending: [f64; 3],
}

impl CostModel {
    pub(super) fn new() -> Self {
        Self {
            info: [[PRIOR_WEIGHT, 0.0], [0.0, PRIOR_WEIGHT]],
            cost: [PRIOR_SECS_PER_FILE, PRIOR_SECS_PER_MIB],
            pending: [0.0; 3],
        }
    }

    /// Accumulates one stretch of movement and folds it in once the stretch
    /// covers `min_span`. A completion an off-cadence event reports
    /// microseconds after the last one would otherwise read as a free file.
    /// A stretch that moved nothing (a device barrier, a stall) is dropped:
    /// its time is nobody's per-file or per-byte cost, and the rates already
    /// count it as slowness.
    pub(super) fn observe(&mut self, files: f64, bytes: f64, secs: f64, min_span: f64) {
        self.pending[0] += files;
        self.pending[1] += bytes / BYTES_PER_UNIT;
        self.pending[2] += secs;
        if self.pending[2] < min_span {
            return;
        }
        let [files, mib, secs] = std::mem::take(&mut self.pending);
        if files > 0.0 || mib > 0.0 {
            self.fold([files, mib], secs);
        }
    }

    /// Seconds one file costs over and above its bytes.
    pub(super) fn secs_per_file(&self) -> f64 {
        self.cost[0].max(0.0)
    }

    /// Seconds one byte costs.
    pub(super) fn secs_per_byte(&self) -> f64 {
        self.cost[1].max(0.0) / BYTES_PER_UNIT
    }

    fn fold(&mut self, x: [f64; 2], secs: f64) {
        let m = self.info;
        let mx = [m[0][0] * x[0] + m[0][1] * x[1], m[1][0] * x[0] + m[1][1] * x[1]];
        let xmx = x[0] * mx[0] + x[1] * mx[1];
        if xmx <= 0.0 {
            return;
        }
        let beta = 1.0 - (-secs / COST_TAU_SECS).exp();
        let mut kept = m;
        for (i, row) in kept.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell -= beta * mx[i] * mx[j] / xmx;
            }
        }
        let rhs = [
            kept[0][0] * self.cost[0] + kept[0][1] * self.cost[1] + x[0] * secs,
            kept[1][0] * self.cost[0] + kept[1][1] * self.cost[1] + x[1] * secs,
        ];
        let mut next = kept;
        for (i, row) in next.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell += x[i] * x[j];
            }
        }
        let det = next[0][0] * next[1][1] - next[0][1] * next[1][0];
        if det <= f64::EPSILON * next[0][0] * next[1][1] {
            return;
        }
        self.cost = [
            (next[1][1] * rhs[0] - next[0][1] * rhs[1]) / det,
            (next[0][0] * rhs[1] - next[1][0] * rhs[0]) / det,
        ];
        self.info = next;
    }
}
