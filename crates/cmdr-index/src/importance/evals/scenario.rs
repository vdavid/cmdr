//! The serde scenario format — the on-disk shape a corpus dump and a committed
//! scenario share, and the in-memory [`Scenario`] the harness scores.
//!
//! A scenario is deliberately NOT a synthetic filesystem: it's the folders plus
//! their already-derived [`FolderSignals`] plus the ranking expectations. That's
//! all the pure scorer needs, and it's exactly what the corpus tool can export
//! from a real index (deriving the same signals production derives). So one type
//! serves both sources: hand-authored synthetic trees and anonymized real dumps
//! load into the same [`Scenario`] and score through the same path.
//!
//! ## Why store signals, not a tree
//!
//! The scorer is pure over [`FolderSignals`] (values in, score out). Persisting
//! the derived signals — not a synthetic directory tree we'd have to re-walk —
//! keeps a scenario cheap to author and makes the corpus dump a faithful capture:
//! the tool derives signals through the SAME production code
//! (`signals::signals_for_dir`) the scheduler uses, so a dumped scenario scores
//! identically to how the live volume would. It also means the privacy contract is
//! simple: a `FolderSignals` holds counts, flags, and bucketed timestamps, never
//! file contents (see the corpus tool for the name-anonymization pass).

use serde::{Deserialize, Serialize};

use super::constraints::Constraint;
use crate::importance::scorer::{FolderSignals, SignalSet};

/// The availability mask a scenario scores under, serialized compactly. A local
/// scenario is `Local` (both optional signals available); an SMB/NAS scenario is
/// `ListingOnly` (no Spotlight, so `last_used` redistributes — the same
/// degradation the SMB scheduler applies). Kept as a named enum rather than the
/// raw [`SignalSet`] bools so a scenario file reads clearly and a new backend kind
/// stays a one-line addition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    /// A local macOS volume: visits and Spotlight last-used both available.
    Local,
    /// A network volume (SMB/NAS): listing signals only, Spotlight unavailable, so
    /// its weight redistributes (never fabricated).
    ListingOnly,
}

impl Availability {
    /// The scorer [`SignalSet`] this availability maps to.
    pub fn signal_set(self) -> SignalSet {
        match self {
            Availability::Local => SignalSet::all(),
            Availability::ListingOnly => SignalSet::listing_only(),
        }
    }
}

/// One folder in a scenario: its path (identity, and the input to the path-class
/// and name classifiers when a corpus tool derives signals) and its derived
/// signal vector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioFolder {
    /// The folder's absolute path within the scenario. For a synthetic scenario a
    /// readable made-up path (`/home/projects/webapp`); for a corpus dump the
    /// anonymized path (classification-relevant names kept, everything else a
    /// stable placeholder — see the corpus tool).
    pub path: String,
    /// The signal vector the scorer consumes. Already derived (by the fixture
    /// builder or by production signal-assembly), so scoring is a pure re-run.
    pub signals: FolderSignals,
}

/// A complete scenario: its folders + signals, the availability mask to score
/// under, and the two tiers of ranking expectations. The shared unit for synthetic
/// and corpus scenarios; [`super::score_scenario`] turns one into a quality score.
///
/// `now_secs` is the wall-clock "now" the recency signals score against. A
/// synthetic scenario pins it to the same value its mtimes were built from (so
/// recency is deterministic); a corpus dump records the snapshot time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    /// A short, stable identifier (the dev-home, the media-home, a corpus volume).
    /// Used in test names and the report, so keep it kebab-case and terse.
    pub name: String,
    /// A one-line story of what the scenario represents, for the guide and the
    /// scenario file's own readability.
    pub description: String,
    /// The signal-availability mask (local vs. listing-only).
    pub availability: Availability,
    /// The "now" (Unix seconds) recency signals decay from.
    pub now_secs: u64,
    /// The scenario's folders with their derived signals.
    pub folders: Vec<ScenarioFolder>,
    /// Ordering facts that must ALWAYS hold. The harness asserts each as a hard
    /// `#[test]`; a violation fails CI.
    #[serde(default)]
    pub hard: Vec<Constraint>,
    /// Desirable orderings counted into the scalar quality score. A violation
    /// lowers the score but doesn't fail a test (until the score drops below the
    /// pinned floor).
    #[serde(default)]
    pub soft: Vec<Constraint>,
}

impl Scenario {
    /// Serialize to pretty JSON (the committed + dumped scenario file format).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Parse from JSON.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}
