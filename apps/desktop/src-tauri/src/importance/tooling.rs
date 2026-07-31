//! Everything the importance subsystem exposes for developer tooling, in one
//! place, behind one feature.
//!
//! ❌ **Not part of the API, and not part of the app.** Nothing here runs in a
//! shipped build: it's the corpus and measurement machinery the `index-query`
//! binaries drive to tune and audit folder scoring
//! (`importance-snapshot`, `importance-measure`, `importance-diff`).
//!
//! It's a feature rather than a `#[cfg(test)]` module because these are separate
//! BINARIES in another crate, not tests: `cfg(test)` is set only while a crate
//! compiles its own test target, so the items would be invisible to them.
//! Separate from `testing` because the two answer different questions — a test
//! needs fakes and guards, a tool needs the real scoring pipeline plus a corpus.

/// The evaluation corpus: labelled folders, synthetic scenarios, hard and soft
/// constraints, and the scoring harness that ranks a scenario against them.
pub use crate::importance::evals;

/// Re-scoring a whole index into a database and comparing two walks, which is
/// what a measurement run does. In the app the scheduler drives the same code
/// through its own incremental path.
pub use crate::importance::scheduler::{
    MeasureOutcome, OriginComparison, compare_walks_for_incremental, recompute_index_to_db, sample_origins,
};
