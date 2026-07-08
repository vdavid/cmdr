//! Pure input and output types for the importance scorer.
//!
//! Everything here is values-in / values-out: no `rusqlite`, no `Volume`, no
//! filesystem, no clocks. A caller assembles a [`FolderSignals`] from whatever
//! source it has (the drive index, a synthetic fixture), passes the current
//! time in as a value, and gets a [`Score`] back. This keeps the formula
//! unit-testable and tunable without a running app (plan Decision 3, agent-spec
//! §6.3 / §15 testability seams).

use serde::{Deserialize, Serialize};

/// A folder's importance score, normalized to `0.0..=1.0`.
///
/// `0.0` is "ignore this folder" (a `node_modules`, a cache); `1.0` is "this
/// folder matters a lot" (an active project root, a Documents subtree). The
/// scale is deliberately unit-free so consumers can threshold it however they
/// like (the agent's summary gate, media-ML's enrich-important-first ordering).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct Score(pub f64);

impl Score {
    /// The lowest possible score. A folder the denylist floors lands here.
    pub const FLOOR: Score = Score(0.0);
    /// The highest possible score.
    pub const CEILING: Score = Score(1.0);

    /// Clamps an arbitrary float into the valid `0.0..=1.0` range.
    pub fn clamped(value: f64) -> Score {
        Score(value.clamp(0.0, 1.0))
    }

    /// The raw `f64`, for comparisons and formatting.
    pub fn value(self) -> f64 {
        self.0
    }
}

/// Which path class a folder sits in, as a typed prior.
///
/// Kept as a typed enum rather than a path-substring branch (the
/// `no-string-matching` rule): the caller classifies the path once, up front,
/// and the scorer reads the resulting variant. `Neutral` is the default when no
/// class applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PathClass {
    /// A user-content root the person actively works in: Downloads, Desktop,
    /// Documents, and their subtrees. Raises the prior.
    UserContent,
    /// The root of a detected project (a `.git` and similar sits here or above).
    /// Raises the prior strongly.
    ProjectRoot,
    /// A system / library / cache location: `~/Library`, caches, application
    /// support. Lowers the prior.
    SystemOrCache,
    /// No class applies; the scorer leans on the other signals.
    #[default]
    Neutral,
}

/// The raw signal vector a folder scores from (agent-spec §5.1).
///
/// This is the **serde-serializable** type M2 persists as the stored raw signal
/// vector (plan Decision 2), so a future consumer can re-weight the same signals
/// under its own profile without a rescan. Optional fields model signals that
/// are not always available: a signal that is `None` is redistributed by the
/// scorer, never fabricated (plan Decision 3). `visit_count` and
/// `last_used_secs` are typed here but stay `None` in M1 — they wire in M2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderSignals {
    /// `true` when the folded folder name is on the known-unimportant denylist
    /// (`node_modules`, `.git`, caches, build output). A denylisted folder is
    /// floored regardless of its other signals.
    pub name_denylisted: bool,
    /// `true` when the folder is hidden (dotfile) or OS/system-owned. Lowers the
    /// score: hidden/system folders are rarely what a person means by "important".
    pub hidden_or_system: bool,
    /// `true` when a floored ancestor (a denylisted, hidden, or system folder) sits
    /// somewhere above this folder — so the whole subtree under a `node_modules`,
    /// a `.git`, or a cache is floored, not just the named folder itself. A FLOOR
    /// override like the two above.
    ///
    /// `#[serde(default)]` so a `FolderSignals` persisted before this field existed
    /// still deserializes (its absence reads as `false`); such a stored vector is a
    /// stale generation anyway and gets overwritten on the next full pass.
    #[serde(default)]
    pub under_floored_ancestor: bool,
    /// Distinct file extensions directly under this folder. A folder with many
    /// kinds of files reads as a working area; a monoculture (one extension) reads
    /// as machine output (a logs folder, a frame dump). See [`extension_count`].
    pub distinct_extension_count: u32,
    /// Total files directly under this folder. Pairs with
    /// `distinct_extension_count` to compute diversity: 200 files of one extension
    /// is a strong monoculture signal; three files of one extension is not.
    pub file_count: u32,
    /// Seconds since the Unix epoch of the folder's most recent modification, if
    /// known. Recency raises the score. `None` when the source has no mtime.
    pub mtime_secs: Option<u64>,
    /// `true` when a project marker (`.git`, `Cargo.toml`, `package.json`, …) sits
    /// in this folder or a descendant, marking this as (at or above) a project
    /// root. Raises the whole subtree (plan Decision 3).
    pub has_project_marker: bool,
    /// The typed path-class prior for this folder (see [`PathClass`]).
    pub path_class: PathClass,
    /// How many times the user has navigated into this folder, if the visit
    /// signal is available. `None` in M1 (the backend visit store lands in M2);
    /// the scorer treats its absence as one missing term.
    pub visit_count: Option<u32>,
    /// Seconds since the Unix epoch of the folder's sampled `kMDItemLastUsedDate`
    /// (macOS Spotlight last-used), if available. `None` on SMB/MTP (no Spotlight)
    /// and `None` in M1 (sampling lands in M2).
    pub last_used_secs: Option<u64>,
}

impl FolderSignals {
    /// A neutral baseline: nothing denylisted, nothing hidden, no files, no
    /// markers, no optional signals. Tests and fixture builders start here and
    /// set only the fields they care about.
    pub fn neutral() -> Self {
        Self {
            name_denylisted: false,
            hidden_or_system: false,
            under_floored_ancestor: false,
            distinct_extension_count: 0,
            file_count: 0,
            mtime_secs: None,
            has_project_marker: false,
            path_class: PathClass::Neutral,
            visit_count: None,
            last_used_secs: None,
        }
    }
}

/// Which optional signals are AVAILABLE for a folder, independent of their value.
///
/// This is the availability mask the scorer uses to redistribute weight (plan
/// Decision 3): on SMB there is no Spotlight, so `last_used` is unavailable and
/// its weight spreads across the signals that ARE present, rather than the folder
/// being penalized for a signal its backend can't produce. Availability is
/// distinct from the value being `None`: a locally-mounted folder whose
/// `kMDItemLastUsedDate` sampling simply hasn't run yet is *available but not yet
/// sampled*, whereas an SMB folder is *unavailable*. M1 only distinguishes the
/// two optional, backend-dependent signals; the always-present listing signals
/// (name, hidden, extensions, mtime, markers, path class) are always available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SignalSet {
    /// Whether the navigation-visit signal can be produced for this volume.
    pub visit_available: bool,
    /// Whether the Spotlight last-used signal can be produced for this volume
    /// (local macOS only; `false` on SMB/MTP).
    pub last_used_available: bool,
}

impl SignalSet {
    /// Every optional signal available (a local macOS volume, all sources wired).
    pub fn all() -> Self {
        Self {
            visit_available: true,
            last_used_available: true,
        }
    }

    /// Only the always-present listing signals (an SMB/MTP volume, or M1 before
    /// the optional sources are wired). Both backend-dependent signals off.
    pub fn listing_only() -> Self {
        Self {
            visit_available: false,
            last_used_available: false,
        }
    }
}

/// One signal's contribution to the final score, for the explain breakdown.
///
/// The list of these for a folder sums (with the base) to exactly the folder's
/// [`Score`] — that invariant is what makes `explain` honest and is pinned by a
/// test. `weight` is the (possibly redistributed) coefficient this signal
/// carried; `raw` is its `0.0..=1.0` normalized signal value; `contribution` is
/// their product, the points this signal added to the score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SignalContribution {
    /// The signal this line describes.
    pub signal: SignalKind,
    /// The coefficient applied, after any redistribution of missing signals.
    pub weight: f64,
    /// The signal's normalized `0.0..=1.0` value.
    pub raw: f64,
    /// `weight * raw`: the points this signal added to the score.
    pub contribution: f64,
}

/// The named signals the scorer weighs. Typed (not stringly) so the explain
/// breakdown and any per-consumer profile key off variants, not labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SignalKind {
    /// Extension diversity: mixed folders score above monocultures.
    ExtensionDiversity,
    /// Modification recency.
    MtimeRecency,
    /// A project marker raising the folder.
    ProjectMarker,
    /// The path-class prior.
    PathClass,
    /// Not hidden / not system-owned.
    Visibility,
    /// Navigation-visit recency/frequency (optional; `None` in M1).
    VisitActivity,
    /// Spotlight last-used recency (optional; `None` on SMB, `None` in M1).
    LastUsed,
}

impl SignalKind {
    /// The seven weighted signals, in a stable order for the explain breakdown.
    /// Excludes the denylist and hidden FLOOR overrides, which are not additive
    /// terms — they cap the score rather than contribute to it.
    pub const ALL: [SignalKind; 7] = [
        SignalKind::ExtensionDiversity,
        SignalKind::MtimeRecency,
        SignalKind::ProjectMarker,
        SignalKind::PathClass,
        SignalKind::Visibility,
        SignalKind::VisitActivity,
        SignalKind::LastUsed,
    ];
}

/// The full explain breakdown for one folder: the score plus every signal's
/// contribution to it (plan Decision 3 / D6, agent-spec radical-transparency).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Explanation {
    /// The final score, identical to what [`super::score`] returns for the same
    /// inputs.
    pub score: Score,
    /// Per-signal contributions. Their sum (clamped) equals `score`.
    pub contributions: Vec<SignalContribution>,
    /// `true` when a FLOOR override (denylist, hidden/system, or a floored
    /// ancestor) capped the score, so a reader understands why the additive terms
    /// don't sum to the score in that case.
    pub floored: bool,
}

/// Counts the distinct extensions a folder holds, given its files' names.
///
/// A convenience for callers assembling [`FolderSignals`] from a listing: it
/// folds each extension to lowercase and counts the distinct set. Files with no
/// extension count as a single "no extension" bucket. Pure and allocation-light.
pub fn extension_count<'a>(file_names: impl IntoIterator<Item = &'a str>) -> u32 {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    for name in file_names {
        let ext = std::path::Path::new(name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        seen.insert(ext);
    }
    seen.len() as u32
}
