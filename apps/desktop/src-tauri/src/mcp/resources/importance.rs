//! The `cmdr://importance` resource builder.
//!
//! Reports folder-importance scores to LLM readers, offline-capable: the importance
//! stores outlive their volume's mount, so an agent can ask which folders matter on
//! an unmounted drive (the importance subsystem's headline). Four query modes:
//!
//! - `?path=<abs-path>` — one folder's `WeightLookup`: Scored (score + the `explain`
//!   signal breakdown), Floored (with the floor reason), or Unscored. `~` expands.
//! - `?top=<n>&volume=<id>` — the top-N folders by score (volume optional ⇒ all
//!   scored volumes, merged).
//! - `?threshold=<f>` — folders scoring at or above `f`, row count capped with a
//!   truncation note (a low threshold can match every scored folder).
//! - no query — a usage summary plus a per-volume overview, so an agent's first
//!   blind read teaches it the syntax.
//!
//! Every read goes through [`ImportanceIndex`](crate::importance::ImportanceIndex)
//! (never raw SQLite — the subsystem's consumer-entry-point invariant). The snapshot
//! functions do the reads (the seam where the data dir and the clock enter); the
//! `build_*` functions are pure over the snapshot + an injected `now_secs`, so the
//! formatting is unit-testable without a live app (the `resources/indexing.rs`
//! snapshot-then-format precedent).

use std::path::Path;

use crate::importance::read::scored_volume_ids;
use crate::importance::{Explanation, FloorReason, ImportanceIndex, ScoredWeight, SignalKind, SignalSet, WeightLookup};

use super::indexing::format_number;

/// The most rows a `?threshold=` read returns before truncating, keeping a low
/// threshold from dumping every scored folder. The build notes when it truncates.
const THRESHOLD_ROW_CAP: usize = 100;

/// A ceiling on `?top=<n>` so a huge `n` can't ask every volume for its whole table.
const TOP_N_MAX: usize = 500;

// ── Snapshots (the reads; data dir + clock enter here) ───────────────────────

/// One folder's importance, resolved through [`ImportanceIndex::lookup`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PathImportance {
    /// The folder has a stored weight on `volume_id`, with its `explain` breakdown.
    Scored {
        volume_id: String,
        weight: ScoredWeight,
        explanation: Option<Explanation>,
    },
    /// The folder floors by its path (no row), with the reason why.
    Floored { reason: FloorReason },
    /// The folder isn't scored on any volume and doesn't floor.
    Unscored,
}

/// One ranked folder in a top-N / threshold list: which volume it lives on, its
/// path, and its score.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RankedFolder {
    pub volume_id: String,
    pub path: String,
    pub score: f64,
}

/// One volume's line in the no-query overview.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VolumeOverview {
    pub volume_id: String,
    /// `local` / `smb`, or `None` for an offline volume whose kind isn't registered.
    pub kind: Option<&'static str>,
    pub generation: u64,
    pub folder_count: u64,
}

/// The signal-availability mask to open a volume's index with, so `explain`
/// redistributes exactly as the recompute that wrote the weights did. Offline
/// volumes (kind unregistered) fall back to the local mask — the breakdown is a
/// nicety, and a path lookup is almost always the local volume anyway.
fn available_for(volume_id: &str) -> SignalSet {
    crate::indexing::volume_kind(volume_id)
        .and_then(crate::importance::signal_availability)
        .unwrap_or_else(SignalSet::all)
}

/// The kind token for a volume, or `None` when it isn't a registered index
/// (offline). Never string-matches the id — routes through the typed `volume_kind`.
fn kind_token(volume_id: &str) -> Option<&'static str> {
    use crate::indexing::IndexVolumeKind;
    crate::indexing::volume_kind(volume_id).map(|k| match k {
        IndexVolumeKind::Local => "local",
        IndexVolumeKind::Smb => "smb",
        IndexVolumeKind::Mtp => "mtp",
    })
}

/// Expand a leading `~` to `$HOME` (agents send `~/Downloads`). Only the tilde
/// prefix — no `~user` form, which the importance paths never use.
fn expand_tilde(path: &str, home: &str) -> String {
    if path == "~" {
        return home.to_string();
    }
    match path.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None => path.to_string(),
    }
}

/// Look one folder up across every scored volume: the first volume with a stored
/// row wins (absolute paths are volume-distinct, so at most one matches). With no
/// stored row anywhere, the floored-vs-unscored classification is volume-independent
/// (path + home), so it derives once against the root index (whose DB may be
/// absent — the read guard handles that).
pub(crate) fn snapshot_path(data_dir: &Path, raw_path: &str, now_secs: u64) -> PathImportance {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = expand_tilde(raw_path, &home);

    for volume_id in scored_volume_ids(data_dir) {
        let index = ImportanceIndex::open(data_dir, &volume_id, available_for(&volume_id));
        if let Ok(WeightLookup::Scored(weight)) = index.lookup(&path) {
            let explanation = index.explain(&path, now_secs).ok().flatten();
            return PathImportance::Scored {
                volume_id,
                weight,
                explanation,
            };
        }
    }

    // No stored row on any volume: derive floored vs unscored from the path alone.
    let index = ImportanceIndex::open(data_dir, crate::indexing::ROOT_VOLUME_ID, SignalSet::all());
    match index.lookup(&path) {
        Ok(WeightLookup::Floored(reason)) => PathImportance::Floored { reason },
        _ => PathImportance::Unscored,
    }
}

/// The scored volumes to read for a ranked query: the one named `volume`, or all of
/// them. Returns an honest error when a named volume has no importance data.
fn ranked_volumes(data_dir: &Path, volume: Option<&str>) -> Result<Vec<String>, String> {
    let all = scored_volume_ids(data_dir);
    match volume {
        Some(v) if all.iter().any(|id| id == v) => Ok(vec![v.to_string()]),
        Some(v) => Err(format!("No importance data for volume '{v}'.")),
        None => Ok(all),
    }
}

/// Merge per-volume `ScoredWeight`s into a ranked `RankedFolder` list, highest score
/// first (ties by path for determinism), capped at `cap`. Returns the list and
/// whether it was truncated (more rows existed than `cap`).
fn merge_ranked(per_volume: Vec<(String, Vec<ScoredWeight>)>, cap: usize) -> (Vec<RankedFolder>, bool) {
    let mut folders: Vec<RankedFolder> = per_volume
        .into_iter()
        .flat_map(|(volume_id, weights)| {
            weights.into_iter().map(move |w| RankedFolder {
                volume_id: volume_id.clone(),
                path: w.path,
                score: w.score.value(),
            })
        })
        .collect();
    folders.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    let truncated = folders.len() > cap;
    folders.truncate(cap);
    (folders, truncated)
}

/// The top `n` folders by score across the requested volume(s).
pub(crate) fn snapshot_top(data_dir: &Path, n: usize, volume: Option<&str>) -> Result<Vec<RankedFolder>, String> {
    let n = n.min(TOP_N_MAX);
    let volumes = ranked_volumes(data_dir, volume)?;
    let per_volume = volumes
        .into_iter()
        .map(|volume_id| {
            let index = ImportanceIndex::open(data_dir, &volume_id, available_for(&volume_id));
            let weights = index.top_n(n).unwrap_or_default();
            (volume_id, weights)
        })
        .collect();
    // Merge to a global top-N: each volume gave its own top-n, so n of the union.
    Ok(merge_ranked(per_volume, n).0)
}

/// Folders scoring at or above `threshold` across the requested volume(s), capped.
/// Returns the (capped) list and whether more existed than the cap.
pub(crate) fn snapshot_threshold(
    data_dir: &Path,
    threshold: f64,
    volume: Option<&str>,
) -> Result<(Vec<RankedFolder>, bool), String> {
    let volumes = ranked_volumes(data_dir, volume)?;
    let per_volume = volumes
        .into_iter()
        .map(|volume_id| {
            let index = ImportanceIndex::open(data_dir, &volume_id, available_for(&volume_id));
            // Fetch cap+1 per volume so the merge can tell it truncated without
            // loading the whole above-threshold tail.
            let weights = index
                .top_above_threshold(THRESHOLD_ROW_CAP + 1, threshold)
                .unwrap_or_default();
            (volume_id, weights)
        })
        .collect();
    Ok(merge_ranked(per_volume, THRESHOLD_ROW_CAP))
}

/// Every scored volume's overview line (id, kind, generation, folder count).
pub(crate) fn snapshot_overview(data_dir: &Path) -> Vec<VolumeOverview> {
    scored_volume_ids(data_dir)
        .into_iter()
        .map(|volume_id| {
            let index = ImportanceIndex::open(data_dir, &volume_id, available_for(&volume_id));
            VolumeOverview {
                kind: kind_token(&volume_id),
                generation: index.recompute_generation().unwrap_or(0),
                folder_count: index.scored_folder_count().unwrap_or(0),
                volume_id,
            }
        })
        .collect()
}

// ── Builders (pure over the snapshot + injected clock) ───────────────────────

/// Format a `0.0..=1.0` score to three decimals.
fn format_score(score: f64) -> String {
    format!("{score:.3}")
}

/// The camelCase label for a signal, matching the JSON serialization agents see
/// elsewhere.
fn signal_label(signal: SignalKind) -> &'static str {
    match signal {
        SignalKind::ExtensionDiversity => "extensionDiversity",
        SignalKind::MtimeRecency => "mtimeRecency",
        SignalKind::ProjectMarker => "projectMarker",
        SignalKind::PathClass => "pathClass",
        SignalKind::Visibility => "visibility",
        SignalKind::VisitActivity => "visitActivity",
        SignalKind::LastUsed => "lastUsed",
    }
}

/// The camelCase label for a floor reason.
fn floor_reason_label(reason: FloorReason) -> &'static str {
    match reason {
        FloorReason::NameDenylisted => "nameDenylisted",
        FloorReason::HiddenOrSystem => "hiddenOrSystem",
        FloorReason::UnderFlooredAncestor => "underFlooredAncestor",
    }
}

/// A one-line human explanation of a floor reason, for the reader who doesn't know
/// the scorer's internals.
fn floor_reason_note(reason: FloorReason) -> &'static str {
    match reason {
        FloorReason::NameDenylisted => {
            "its name is denylisted (build output / cache / VCS internals like node_modules or .git)"
        }
        FloorReason::HiddenOrSystem => "it's hidden (dot-prefixed) or system-owned",
        FloorReason::UnderFlooredAncestor => "a denylisted, hidden, or system ancestor floors its whole subtree",
    }
}

/// Build the `?path=` block for one folder's importance.
pub(crate) fn build_path_text(raw_path: &str, snapshot: &PathImportance) -> String {
    let mut lines = vec![format!("path: {raw_path}")];
    match snapshot {
        PathImportance::Scored {
            volume_id,
            weight,
            explanation,
        } => {
            lines.push("status: scored".to_string());
            lines.push(format!("volume: {volume_id}"));
            lines.push(format!("score: {}", format_score(weight.score.value())));
            lines.push(format!("generation: {}", weight.as_of_generation));
            if let Some(explanation) = explanation {
                lines.push("signals:".to_string());
                for c in &explanation.contributions {
                    lines.push(format!(
                        "  - {}: weight {:.3}, raw {:.3}, +{:.3}",
                        signal_label(c.signal),
                        c.weight,
                        c.raw,
                        c.contribution
                    ));
                }
            }
        }
        PathImportance::Floored { reason } => {
            lines.push("status: floored".to_string());
            lines.push(format!("reason: {}", floor_reason_label(*reason)));
            lines.push(format!("score: {}", format_score(0.0)));
            lines.push(format!(
                "note: Floored to 0 by design because {}.",
                floor_reason_note(*reason)
            ));
        }
        PathImportance::Unscored => {
            lines.push("status: unscored".to_string());
            lines.push(format!("score: {}", format_score(0.0)));
            lines.push(
                "note: No stored weight. Either this volume hasn't been scored yet, or the folder isn't in the index."
                    .to_string(),
            );
        }
    }
    lines.join("\n")
}

/// Build the `?top=` / `?threshold=` ranked list. Each row is
/// `rank. score  volume  path` so an agent can scan by score and see which volume a
/// folder lives on.
pub(crate) fn build_ranked_text(header: &str, folders: &[RankedFolder], truncated_note: Option<&str>) -> String {
    let mut lines = vec![header.to_string()];
    if folders.is_empty() {
        lines.push("  (no folders match)".to_string());
    }
    for (i, f) in folders.iter().enumerate() {
        lines.push(format!(
            "  {}. {}  {}  {}",
            i + 1,
            format_score(f.score),
            f.volume_id,
            f.path
        ));
    }
    if let Some(note) = truncated_note {
        lines.push(String::new());
        lines.push(note.to_string());
    }
    lines.join("\n")
}

/// Build the no-query overview: the usage summary plus a per-volume roster. Written
/// so an agent's first blind read teaches it the query syntax.
pub(crate) fn build_overview_text(volumes: &[VolumeOverview]) -> String {
    let mut lines = vec![
        "cmdr://importance — folder-importance scores (which folders matter), offline-capable.".to_string(),
        String::new(),
        "Query modes:".to_string(),
        "  ?path=<abs-path>        one folder's score + signal breakdown, or why it floors (~ expands)".to_string(),
        "  ?top=<n>&volume=<id>    the top-N folders by score (volume optional ⇒ all scored volumes)".to_string(),
        "  ?threshold=<f>          folders scoring at or above f (0.0-1.0), capped".to_string(),
        String::new(),
    ];

    if volumes.is_empty() {
        lines.push(
            "No importance data yet. Volumes score in the background once indexed (local + SMB; MTP is excluded)."
                .to_string(),
        );
    } else {
        lines.push("Scored volumes:".to_string());
        for v in volumes {
            let kind = v.kind.map(|k| format!(" ({k})")).unwrap_or_default();
            lines.push(format!(
                "  {}{}: generation {}, {} folders",
                v.volume_id,
                kind,
                v.generation,
                format_number(v.folder_count)
            ));
        }
    }

    lines.join("\n")
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

/// Parse `cmdr://importance`'s query and render the matching mode. `now_secs` is
/// injected so the `?path=` recency breakdown is deterministic under test.
pub(crate) fn build_importance_resource(data_dir: &Path, query: Option<&str>, now_secs: u64) -> String {
    let q = super::parse_query(query);

    if let Some(path) = q.get("path") {
        let snapshot = snapshot_path(data_dir, path, now_secs);
        return build_path_text(path, &snapshot);
    }

    if let Some(top) = q.get("top") {
        let Ok(n) = top.parse::<usize>() else {
            return format!("Invalid `top` value '{top}': expected a whole number, for example `?top=20`.");
        };
        let volume = q.get("volume").map(String::as_str);
        return match snapshot_top(data_dir, n, volume) {
            Ok(folders) => {
                let scope = volume.map(|v| format!(" on volume '{v}'")).unwrap_or_default();
                let header = format!("Top {n} folders by importance{scope}:");
                build_ranked_text(&header, &folders, None)
            }
            Err(msg) => msg,
        };
    }

    if let Some(threshold) = q.get("threshold") {
        let Ok(value) = threshold.parse::<f64>() else {
            return format!(
                "Invalid `threshold` value '{threshold}': expected a number 0.0-1.0, for example `?threshold=0.5`."
            );
        };
        let volume = q.get("volume").map(String::as_str);
        return match snapshot_threshold(data_dir, value, volume) {
            Ok((folders, truncated)) => {
                let scope = volume.map(|v| format!(" on volume '{v}'")).unwrap_or_default();
                let header = format!("Folders scoring at or above {}{scope}:", format_score(value));
                let note = truncated.then(|| {
                    format!(
                        "Showing the top {THRESHOLD_ROW_CAP}; more folders match. Raise the threshold to narrow the list."
                    )
                });
                build_ranked_text(&header, &folders, note.as_deref())
            }
            Err(msg) => msg,
        };
    }

    build_overview_text(&snapshot_overview(data_dir))
}

#[cfg(test)]
mod tests;
