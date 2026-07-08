//! The corpus side of the eval suite: snapshot a real drive index into an
//! anonymized [`Scenario`], and load any labeled corpus scenarios found locally.
//!
//! ## Why a corpus at all
//!
//! Synthetic scenarios ([`super::scenarios`]) pin the scorer against homes we made
//! up. A corpus captures David's REAL folder structure so tuning optimizes against
//! reality, not a guess. But real folder names are PII, so a snapshot is
//! anonymized before it's ever written: the scorer only reads a folder's name
//! through the classifiers (denylist / hidden / path-class / marker), so every name
//! that doesn't feed a classifier can become a stable placeholder with ZERO effect
//! on the score. What survives is structure, extension histograms, bucketed
//! timestamps, and flags — never a file's content or a personal folder name.
//!
//! ## The privacy contract (what's kept vs. stripped)
//!
//! A name is kept VERBATIM only when the scorer's classification depends on it:
//!
//! - **Denylist hits** (`node_modules`, `.git`, build output — anything
//!   [`is_denylisted`]): the denylist floors these, so the name must survive.
//! - **Hidden markers** (a leading `.`): hidden/system detection keys off it.
//! - **Path-class anchors** that are direct children of the home root
//!   (`Downloads`, `Desktop`, `Documents`, `Library`): [`path_class`] matches these
//!   by name, so they must survive to keep the prior.
//! - **Project markers** (`.git`, `.hg`, `.svn` as directory names): they raise a
//!   project root.
//!
//! Every OTHER name becomes `dir-<8 hex>`, a stable hash of the original (so the
//! same real name maps to the same placeholder across a dump — structure stays
//! legible — but the original is unrecoverable). The result: the scorer produces
//! the identical ranking on the anonymized dump as on the real tree, and no
//! personal name leaves the machine.
//!
//! ## Never committed
//!
//! A snapshot lands in a GITIGNORED corpus dir (see the guide). Real-derived dumps
//! are David's to review and keep locally; the committed suite is green with zero
//! corpus files present. [`load_corpus_scenarios`] returns an empty vec when the
//! dir is absent, so CI (which has no corpus) stays green.
//!
//! [`is_denylisted`]: crate::importance::classify::is_denylisted
//! [`path_class`]: crate::importance::classify::path_class

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::scenario::{Availability, Scenario, ScenarioFolder};
use crate::importance::classify::{is_denylisted, is_project_marker};

/// The path-class anchor names the scorer matches by name (must survive
/// anonymization when they're a direct child of the home root). Kept in sync with
/// [`crate::importance::classify::path_class`] — the same three user-content
/// anchors plus `Library`.
const PATH_CLASS_ANCHORS: &[&str] = &["Downloads", "Desktop", "Documents", "Library"];

/// Whether a folder NAME must survive anonymization verbatim because the scorer's
/// classification reads it. `is_home_child` says whether the folder is a direct
/// child of the home root (only there does a path-class anchor name matter).
///
/// Everything this returns `true` for is a NON-personal, structural name (a
/// denylisted machine-output dir, a dotfile, a well-known anchor, a VCS marker) —
/// never a name that could carry PII.
pub fn name_is_classification_relevant(name: &str, is_home_child: bool) -> bool {
    // A leading dot drives hidden/system detection, and covers .git/.hg/.svn too.
    if name.starts_with('.') {
        return true;
    }
    let folded = name.to_lowercase();
    // Denylisted machine output (node_modules, caches, build dirs).
    if is_denylisted(name) {
        return true;
    }
    // Directory-form project markers (redundant with the dot check for .git/.hg/.svn,
    // but explicit so a future non-dot marker stays covered).
    if is_project_marker(&folded) {
        return true;
    }
    // A path-class anchor only classifies when it's a direct child of home.
    if is_home_child && PATH_CLASS_ANCHORS.contains(&name) {
        return true;
    }
    false
}

/// The anonymized replacement for one folder name: the name itself when it's
/// classification-relevant, otherwise a stable `dir-<8 hex>` placeholder derived
/// from the original. The placeholder is deterministic (the same input always maps
/// to the same output within a run), so repeated real names stay visibly repeated
/// in the dump while the original is unrecoverable.
pub fn anonymize_name(name: &str, is_home_child: bool) -> String {
    if name_is_classification_relevant(name, is_home_child) {
        return name.to_string();
    }
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    format!("dir-{:08x}", (hasher.finish() as u32))
}

/// A folder David has labeled as genuinely important (the ground truth for
/// personalized soft constraints). Stored in the labels template beside a dump; the
/// harness turns each into a soft "this folder is in the top N" expectation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabeledFolder {
    /// The anonymized path of a folder David marked important (copy it from the
    /// dump's paths).
    pub path: String,
    /// How important, 1 (most) to 3 (mildly). Drives how strict the generated soft
    /// constraint is. Optional; defaults to 2.
    #[serde(default = "default_importance")]
    pub importance: u8,
}

fn default_importance() -> u8 {
    2
}

/// The labels template written beside a dump. David fills `important` with the
/// paths (from the dump) of folders that genuinely matter to him; the harness reads
/// it back to build personalized soft constraints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabelsTemplate {
    /// A human note explaining what to do (written on export).
    #[serde(default)]
    pub note: String,
    /// The folders David marks important. Empty in the template; he fills it in.
    #[serde(default)]
    pub important: Vec<LabeledFolder>,
}

impl LabelsTemplate {
    /// Serialize to pretty JSON (the labels-file format).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// The default corpus directory: `importance-corpus/` under the desktop app's
/// `tests/` tree. GITIGNORED — real dumps live here and are never committed. The
/// path is relative to the repo root (the caller resolves it).
pub const CORPUS_DIR_REL: &str = "apps/desktop/src-tauri/tests/importance-corpus";

/// Resolve the corpus dir under `repo_root`.
pub fn corpus_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(CORPUS_DIR_REL)
}

// ── Snapshot a real index into an anonymized scenario ─────────────────────────

/// Anonymize a full absolute `path` under `home`: keep the home prefix as a fixed
/// synthetic root (`/home` or `/volume`) and anonymize each segment BELOW it,
/// preserving classification-relevant names. The home root itself is replaced by a
/// stable synthetic root so no username or mount path leaks, yet path-class anchors
/// (direct children of home) still line up under it.
///
/// Returns the rebuilt anonymized path. A path not under `home` (shouldn't happen
/// for a home-rooted index, but guard it) anonymizes every segment under a
/// `/volume` root.
fn anonymize_path(path: &str, home: &str, synthetic_root: &str) -> String {
    // Strip the home prefix; the remaining segments are what we anonymize, and the
    // FIRST of them is a home child (where an anchor name matters).
    let rel = path.strip_prefix(home).unwrap_or(path);
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() {
        return synthetic_root.to_string();
    }
    let mut out = String::from(synthetic_root);
    for (depth, segment) in rel.split('/').enumerate() {
        out.push('/');
        out.push_str(&anonymize_name(segment, depth == 0));
    }
    out
}

/// Snapshot a real drive index at `index_db_path` into an anonymized [`Scenario`].
///
/// Reads the index READ-ONLY through the same walk a recompute uses
/// ([`walk_index_folders`](crate::importance::scheduler::walk_index_folders)) and
/// derives each folder's [`FolderSignals`] through the SAME production assembly
/// ([`signals_for_dir`](crate::importance::signals::signals_for_dir)), so the
/// dumped signals match what the live scheduler would compute. Then it anonymizes
/// every folder path (keeping only classification-relevant names) before returning,
/// so nothing personal is in the result.
///
/// `home` is the real home root of the indexed volume (for path classification);
/// `availability` is the volume's kind mask (local vs. listing-only). No visits or
/// Spotlight values are pulled in — a corpus dump captures the LISTING-derived
/// signal shape, and David's personalization comes from his labels, not from
/// replaying his visit history into a committed-shaped file.
///
/// The scenario carries NO expectations: a fresh dump is unlabeled. David adds soft
/// constraints by filling the labels template (see [`labels_template_for`]).
pub fn snapshot_index_to_scenario(
    index_db_path: &Path,
    home: &str,
    availability: Availability,
    name: &str,
    now_secs: u64,
) -> Result<Scenario, String> {
    use crate::importance::scheduler::walk_index_folders;
    use crate::importance::signals::{OptionalSignals, signals_for_dir};

    let conn = crate::indexing::store::IndexStore::open_read_connection(index_db_path)
        .map_err(|e| format!("couldn't open index DB read-only: {e}"))?;
    let folders = walk_index_folders(&conn, home)?;

    let synthetic_root = synthetic_root_for(&availability);
    let scenario_folders = folders
        .iter()
        .map(|f| {
            // Derive the real signals exactly as production does, then anonymize the
            // path. The signals themselves are counts/flags/timestamps — no names —
            // so they need no scrubbing; only the path carries names.
            let signals = signals_for_dir(
                &f.entry,
                f.children,
                &f.path,
                home,
                f.has_marker_below,
                OptionalSignals::default(),
            );
            ScenarioFolder {
                path: anonymize_path(&f.path, home, synthetic_root),
                signals,
            }
        })
        .collect();

    Ok(Scenario {
        name: name.to_string(),
        description: format!(
            "Anonymized corpus snapshot of a real {} volume. Personal names stripped; structure and signal shape kept.",
            match availability {
                Availability::Local => "local",
                Availability::ListingOnly => "network (SMB/NAS)",
            }
        ),
        availability,
        now_secs,
        folders: scenario_folders,
        hard: Vec::new(),
        soft: Vec::new(),
    })
}

/// The synthetic root a dump's paths hang off, so no real mount path or username
/// leaks. Local homes root at `/home`; network volumes at `/volume`.
fn synthetic_root_for(availability: &Availability) -> &'static str {
    match availability {
        Availability::Local => "/home",
        Availability::ListingOnly => "/volume",
    }
}

/// Build the labels template that ships beside a dump: a note plus an empty
/// `important` list David fills with the (anonymized) paths of folders that
/// genuinely matter to him.
pub fn labels_template_for(scenario_name: &str) -> LabelsTemplate {
    LabelsTemplate {
        note: format!(
            "Mark the folders that genuinely matter to you in '{scenario_name}'. Copy their anonymized paths from the \
             matching *.scenario.json dump into the 'important' list below (importance 1 = most, 3 = mildly). The eval \
             harness reads this file to build personalized soft constraints and score how well the weights rank YOUR \
             important folders near the top."
        ),
        important: Vec::new(),
    }
}

// ── Load labeled corpus scenarios for the harness ─────────────────────────────

/// A corpus scenario file pairs a dump with its labels: `<name>.scenario.json`
/// (the anonymized folders) and `<name>.labels.json` (David's important-folder
/// marks). The loader reads both, turning each labeled folder into a soft "this
/// folder ranks in the top N" constraint personalized to David's ground truth.
const SCENARIO_SUFFIX: &str = ".scenario.json";
const LABELS_SUFFIX: &str = ".labels.json";

/// Load every LABELED corpus scenario in `dir`, applying its labels as soft
/// constraints. Only dumps with a non-empty `important` label list load: an
/// unlabeled dump measures nothing (no hard constraints, no soft constraints), so
/// parsing it would be wasted work — and a real dump can be hundreds of MB (David's
/// root index is ~650k folders), which would blow a per-test time budget for zero
/// signal. So the labels file is read FIRST (tiny), and the big dump is parsed only
/// when there's ground truth to score against. A missing dir yields an empty vec
/// (the committed suite stays green with no corpus present).
pub fn load_corpus_scenarios(dir: &Path) -> Vec<Scenario> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut scenarios = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(stem) = file_name.strip_suffix(SCENARIO_SUFFIX) else {
            continue;
        };

        // Read the (tiny) labels file first and skip the dump entirely unless it
        // carries at least one important-folder label — an unlabeled dump adds no
        // constraints, so parsing its (possibly huge) JSON would be pure cost.
        let labels_path = dir.join(format!("{stem}{LABELS_SUFFIX}"));
        let labels = std::fs::read_to_string(&labels_path)
            .ok()
            .and_then(|t| serde_json::from_str::<LabelsTemplate>(&t).ok())
            .filter(|l| !l.important.is_empty());
        let Some(labels) = labels else {
            continue;
        };

        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut scenario) = Scenario::from_json(&text) else {
            continue;
        };
        // Each important folder becomes a soft top-N constraint, N scaled by how
        // important David marked it.
        scenario
            .soft
            .extend(soft_constraints_from_labels(&labels, scenario.folders.len()));
        scenarios.push(scenario);
    }
    // Deterministic order (read_dir is unordered), so a failing test names a stable
    // scenario.
    scenarios.sort_by(|a, b| a.name.cmp(&b.name));
    scenarios
}

/// Turn David's labels into soft constraints: an important folder should rank in
/// the top `k` of the volume, where `k` tightens with importance (1 ⇒ top 5%, 2 ⇒
/// top 15%, 3 ⇒ top 30%), floored at a small minimum so a tiny volume still asks
/// for a real top slot.
fn soft_constraints_from_labels(labels: &LabelsTemplate, folder_count: usize) -> Vec<super::Constraint> {
    labels
        .important
        .iter()
        .map(|labeled| {
            let fraction = match labeled.importance {
                1 => 0.05,
                2 => 0.15,
                _ => 0.30,
            };
            let n = ((folder_count as f64 * fraction).ceil() as usize).max(3);
            super::Constraint::TopN {
                path: labeled.path.clone(),
                n,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
