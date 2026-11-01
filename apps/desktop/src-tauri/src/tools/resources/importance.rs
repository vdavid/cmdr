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
//! - Both ranked modes take `&under=<path>` (only that folder and below), fold
//!   near-duplicate descendants into the row above them ([`collapse_descendants`]),
//!   and say why each row scores ([`why_text`]).
//! - no query — a usage summary plus a per-volume overview, so an agent's first
//!   blind read teaches it the syntax.
//!
//! Every read goes through [`ImportanceIndex`]
//! (never raw SQLite — the subsystem's consumer-entry-point invariant). The snapshot
//! functions do the reads (the seam where the data dir and the clock enter); the
//! `build_*` functions are pure over the snapshot + an injected `now_secs`, so the
//! formatting is unit-testable without a live app (the `resources/indexing.rs`
//! snapshot-then-format precedent).

use std::path::Path;

use cmdr_index::importance::read::scored_volume_ids;
use cmdr_index::importance::{
    Explanation, FloorReason, FolderSignals, ImportanceIndex, PathClass, ScoredWeight, SignalKind, SignalSet,
    WeightLookup, Weights, explain,
};

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
/// path, its score, why it scores (its top signals, terse), and how many of its
/// descendants were folded into it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RankedFolder {
    pub volume_id: String,
    pub path: String,
    pub score: f64,
    /// Its strongest signals in reading order, like `projectRoot, changed today, 6 visits`.
    /// Filled once the list is final, so a folded-away row never pays for it.
    pub why: String,
    /// Descendants within the read window folded into this row ([`collapse_descendants`]).
    pub folded: usize,
    /// The stored signals `why` is phrased from.
    pub signals: FolderSignals,
}

/// Where a ranked read looks: one volume or all, and optionally only at or below one
/// folder. `under` is a path ON `volume` (or on each scored volume when `volume` is
/// `None`), in the store's own path space; `~` expands.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RankedScope<'a> {
    pub volume: Option<&'a str>,
    pub under: Option<&'a str>,
    /// With no `volume`, read only these volume ids: the ones a caller can place
    /// (the agent's rows need a root to join onto). `None` reads every scored one.
    pub among: Option<&'a [String]>,
}

/// How much higher than an ancestor in the list a descendant must score to keep its
/// own row. A descendant scoring lower (`cmdr/apps/desktop` under `cmdr`) always
/// folds; one scoring only slightly higher is a near-duplicate too. On the live root
/// volume (2026-10-26) a folder of projects scored within 0.01 of its best project
/// (`~/projects-git/vdavid` 0.84, `cmdr` 0.85), so without a margin the projects
/// split arbitrarily between their own rows and the parent's by score noise.
const CLEARLY_HIGHER: f64 = 0.05;

/// The first read window, in rows per returned row. Folding can eat most of a window
/// (on the live root volume, 2026-10-26, 153 of the top 160 rows sat under
/// `~/projects-git/vdavid`), so a window that folds below `n` grows by this factor
/// until it fills or reaches [`MAX_FOLD_WINDOW`].
const COLLAPSE_OVERFETCH: usize = 8;

/// The most rows one ranked read takes per volume. Each row is a small JSON parse, so
/// this bounds a read to tens of milliseconds even on a home whose top rows all sit in
/// one tree.
const MAX_FOLD_WINDOW: usize = 20_000;

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
///
/// Shared with the wake loop's importance cache, which opens the same indexes for
/// the same reason and must not fork the mask.
pub(crate) fn available_for(volume_id: &str) -> SignalSet {
    crate::index_host::index()
        .volume_kind(volume_id)
        .and_then(cmdr_index::importance::signal_availability)
        .unwrap_or_else(SignalSet::all)
}

/// The kind token for a volume, or `None` when it isn't a registered index
/// (offline). Never string-matches the id — routes through the typed `volume_kind`.
fn kind_token(volume_id: &str) -> Option<&'static str> {
    use cmdr_index::IndexVolumeKind;
    crate::index_host::index().volume_kind(volume_id).map(|k| match k {
        IndexVolumeKind::Local => "local",
        IndexVolumeKind::LocalExternal => "external",
        IndexVolumeKind::Smb => "smb",
        IndexVolumeKind::Mtp => "mtp",
        IndexVolumeKind::Adb => "adb",
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
        let found = snapshot_path_on(data_dir, &volume_id, &path, now_secs);
        if matches!(found, PathImportance::Scored { .. }) {
            return found;
        }
    }

    // No stored row on any volume: derive floored vs unscored from the path alone.
    let index = ImportanceIndex::open(data_dir, cmdr_index::ROOT_VOLUME_ID, SignalSet::all());
    match index.lookup(&path) {
        Ok(WeightLookup::Floored(reason)) => PathImportance::Floored { reason },
        _ => PathImportance::Unscored,
    }
}

/// One folder's importance on ONE volume's store, `path` in that store's own path
/// space (volume-relative, absolute only on the boot disk). Reads no other store,
/// so a share's `/papers` never answers for another share's.
pub(crate) fn snapshot_path_on(data_dir: &Path, volume_id: &str, path: &str, now_secs: u64) -> PathImportance {
    let index = ImportanceIndex::open(data_dir, volume_id, available_for(volume_id));
    match index.lookup(path) {
        Ok(WeightLookup::Scored(weight)) => PathImportance::Scored {
            volume_id: volume_id.to_string(),
            weight,
            explanation: index.explain(path, now_secs).ok().flatten(),
        },
        Ok(WeightLookup::Floored(reason)) => PathImportance::Floored { reason },
        Ok(WeightLookup::Unscored) | Err(_) => PathImportance::Unscored,
    }
}

/// The scored volumes to read for a ranked query: the one named `volume`, or all of
/// them (only those in `among`, when given). Returns an honest error when a named
/// volume has no importance data.
fn ranked_volumes(data_dir: &Path, scope: RankedScope<'_>) -> Result<Vec<String>, String> {
    let all = scored_volume_ids(data_dir);
    match scope.volume {
        Some(v) if all.iter().any(|id| id == v) => Ok(vec![v.to_string()]),
        Some(v) => Err(format!("No importance data for volume '{v}'.")),
        None => Ok(all
            .into_iter()
            .filter(|id| scope.among.is_none_or(|among| among.contains(id)))
            .collect()),
    }
}

/// Read up to `fetch` rows per requested volume (at or under `scope.under`, at or
/// above `threshold`), shape each into a [`RankedFolder`] (with no `why` yet), and
/// merge them highest score first (ties by path for determinism). The `bool` says
/// whether some volume had more rows than `fetch`, so a caller can tell its window was
/// cut.
fn read_ranked(
    data_dir: &Path,
    scope: RankedScope<'_>,
    under: &str,
    fetch: usize,
    threshold: Option<f64>,
) -> Result<(Vec<RankedFolder>, bool), String> {
    let mut window_cut = false;
    let mut folders = Vec::new();
    for volume_id in ranked_volumes(data_dir, scope)? {
        let index = ImportanceIndex::open(data_dir, &volume_id, available_for(&volume_id));
        let weights = index.top_under(fetch + 1, threshold, under).unwrap_or_default();
        window_cut |= weights.len() > fetch;
        folders.extend(weights.into_iter().take(fetch).map(|w| RankedFolder {
            volume_id: volume_id.clone(),
            path: w.path,
            score: w.score.value(),
            why: String::new(),
            folded: 0,
            signals: w.signals,
        }));
    }
    folders.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok((folders, window_cut))
}

/// Read, fold, and grow the window until folding leaves MORE than `keep` rows, the
/// volume runs out, or the window reaches [`MAX_FOLD_WINDOW`]. Returns the folded
/// list (untruncated, `why` still empty) and whether the last window was cut.
fn read_folded(
    data_dir: &Path,
    scope: RankedScope<'_>,
    keep: usize,
    threshold: Option<f64>,
) -> Result<(Vec<RankedFolder>, bool), String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let under = scope.under.map(|u| expand_tilde(u, &home));
    let mut fetch = (keep * COLLAPSE_OVERFETCH).clamp(1, MAX_FOLD_WINDOW);
    loop {
        let (ranked, window_cut) = read_ranked(data_dir, scope, under.as_deref().unwrap_or(""), fetch, threshold)?;
        let folders = collapse_descendants(ranked, under.as_deref(), &home);
        if folders.len() > keep || !window_cut || fetch == MAX_FOLD_WINDOW {
            return Ok((folders, window_cut));
        }
        fetch = (fetch * COLLAPSE_OVERFETCH).min(MAX_FOLD_WINDOW);
    }
}

/// Phrase each final row's `why` against its own volume's signal availability, so the
/// reasons match the breakdown that produced its score.
fn annotate_why(folders: &mut [RankedFolder], now_secs: u64) {
    for folder in folders {
        let available = available_for(&folder.volume_id);
        let explanation = explain(&folder.signals, &available, &Weights::default(), now_secs);
        folder.why = why_text(&folder.signals, &explanation, now_secs);
    }
}

/// Fold near-duplicate descendants into the folder above them, over a list ranked
/// highest first. A row folds into a kept ancestor (same volume) unless it scores at
/// least [`CLEARLY_HIGHER`] above it; the ancestor's `folded` counts it (plus whatever
/// had folded into it). Rows keep their ranked order.
///
/// Some folders never absorb, so their children keep their own rows: the `under`
/// folder the caller asked about (listing its children is what asking means), and
/// `$HOME`, its ancestors, and a volume root, which would otherwise swallow
/// `Documents` and friends into one generic row. Their children's own descendants
/// still fold into them.
pub(crate) fn collapse_descendants(ranked: Vec<RankedFolder>, under: Option<&str>, home: &str) -> Vec<RankedFolder> {
    let anchor = under.map(|u| u.trim_end_matches('/'));
    let home = home.trim_end_matches('/');
    let absorbs = |folder: &RankedFolder| {
        let path = folder.path.trim_end_matches('/');
        !path.is_empty()
            && anchor.is_none_or(|a| !path.eq_ignore_ascii_case(a))
            && !(home == path || is_strictly_under(home, path))
    };
    let mut kept: Vec<RankedFolder> = Vec::new();
    for mut row in ranked {
        // A kept ancestor ranks at or above this row, so this row can't be clearly higher.
        if let Some(ancestor) = kept
            .iter_mut()
            .find(|k| k.volume_id == row.volume_id && absorbs(k) && is_strictly_under(&row.path, &k.path))
        {
            ancestor.folded += 1 + row.folded;
            continue;
        }
        // Kept descendants that ranked above this row only by a hair fold into it.
        if absorbs(&row) {
            kept.retain(|k| {
                let near_duplicate = k.volume_id == row.volume_id
                    && is_strictly_under(&k.path, &row.path)
                    && k.score < row.score + CLEARLY_HIGHER;
                if near_duplicate {
                    row.folded += 1 + k.folded;
                }
                !near_duplicate
            });
        }
        kept.push(row);
    }
    kept
}

/// Whether `path` sits strictly below `ancestor` (`/` is everything's ancestor).
fn is_strictly_under(path: &str, ancestor: &str) -> bool {
    path.strip_prefix(ancestor.trim_end_matches('/'))
        .is_some_and(|rest| rest.starts_with('/') && rest.len() > 1)
}

/// The top `n` folders by score across the requested scope, near-duplicates folded.
pub(crate) fn snapshot_top(
    data_dir: &Path,
    n: usize,
    scope: RankedScope<'_>,
    now_secs: u64,
) -> Result<Vec<RankedFolder>, String> {
    let n = n.min(TOP_N_MAX);
    let (mut folders, _) = read_folded(data_dir, scope, n, None)?;
    folders.truncate(n);
    annotate_why(&mut folders, now_secs);
    Ok(folders)
}

/// Folders scoring at or above `threshold` across the requested scope, near-duplicates
/// folded, capped. Returns the (capped) list and whether more existed than the cap.
pub(crate) fn snapshot_threshold(
    data_dir: &Path,
    threshold: f64,
    scope: RankedScope<'_>,
    now_secs: u64,
) -> Result<(Vec<RankedFolder>, bool), String> {
    let (mut folders, window_cut) = read_folded(data_dir, scope, THRESHOLD_ROW_CAP, Some(threshold))?;
    let truncated = window_cut || folders.len() > THRESHOLD_ROW_CAP;
    folders.truncate(THRESHOLD_ROW_CAP);
    annotate_why(&mut folders, now_secs);
    Ok((folders, truncated))
}

/// A folder's strongest signals as a terse phrase list, the ones that actually added
/// points, strongest first: `projectRoot, changed today, 6 visits`. At most three.
pub(crate) fn why_text(signals: &FolderSignals, explanation: &Explanation, now_secs: u64) -> String {
    const MAX_REASONS: usize = 3;
    let mut contributions: Vec<_> = explanation
        .contributions
        .iter()
        .filter(|c| c.contribution > 0.0)
        .collect();
    contributions.sort_by(|a, b| {
        b.contribution
            .partial_cmp(&a.contribution)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    contributions
        .into_iter()
        .filter_map(|c| signal_reason(c.signal, signals, now_secs))
        .take(MAX_REASONS)
        .collect::<Vec<_>>()
        .join(", ")
}

/// One signal's reason in a few words, or `None` when it says nothing useful (the
/// constant visibility term, a neutral path class).
fn signal_reason(signal: SignalKind, signals: &FolderSignals, now_secs: u64) -> Option<String> {
    match signal {
        SignalKind::PathClass => match signals.path_class {
            PathClass::ProjectRoot => Some("projectRoot".to_string()),
            PathClass::UserContent => Some("userContent".to_string()),
            PathClass::SystemOrCache | PathClass::Neutral => None,
        },
        SignalKind::ProjectMarker => match signals.marker_depth? {
            0 => Some("has project files".to_string()),
            _ => Some("holds projects".to_string()),
        },
        SignalKind::MtimeRecency => signals.mtime_secs.map(|t| format!("changed {}", age(now_secs, t))),
        SignalKind::VisitActivity => signals
            .visit_count
            .map(|n| crate::pluralize::pluralize(u64::from(n), "visit")),
        SignalKind::LastUsed => signals.last_used_secs.map(|t| format!("opened {}", age(now_secs, t))),
        SignalKind::ExtensionDiversity => Some(format!(
            "{}, {}",
            crate::pluralize::pluralize(u64::from(signals.file_count), "file"),
            crate::pluralize::pluralize(u64::from(signals.distinct_extension_count), "type")
        )),
        SignalKind::Visibility => None,
    }
}

/// How long ago `then` was, in days: `today`, `yesterday`, or `12d ago`.
fn age(now_secs: u64, then: u64) -> String {
    const DAY: u64 = 24 * 60 * 60;
    match now_secs.saturating_sub(then) / DAY {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        days => format!("{days}d ago"),
    }
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
/// `rank. score  volume  path  (why) [+N folded]` so an agent can scan by score, see
/// which volume a folder lives on, and see what it hides.
pub(crate) fn build_ranked_text(header: &str, folders: &[RankedFolder], truncated_note: Option<&str>) -> String {
    let mut lines = vec![header.to_string()];
    if folders.is_empty() {
        lines.push("  (no folders match)".to_string());
    }
    for (i, f) in folders.iter().enumerate() {
        let why = if f.why.is_empty() {
            String::new()
        } else {
            format!("  ({})", f.why)
        };
        let folded = if f.folded == 0 {
            String::new()
        } else {
            format!(" [+{} folded]", f.folded)
        };
        lines.push(format!(
            "  {}. {}  {}  {}{why}{folded}",
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
        "  &under=<path>           with top or threshold: only folders at or below path; its children get own rows"
            .to_string(),
        "Near-duplicate descendants fold into the folder above them ([+N folded]).".to_string(),
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

/// The ` on volume 'x' under '/p'` tail of a ranked header, empty for the whole set.
fn scope_label(scope: RankedScope<'_>) -> String {
    let volume = scope.volume.map(|v| format!(" on volume '{v}'")).unwrap_or_default();
    let under = scope.under.map(|u| format!(" under '{u}'")).unwrap_or_default();
    format!("{volume}{under}")
}

/// Parse `cmdr://importance`'s query and render the matching mode. `now_secs` is
/// injected so the `?path=` recency breakdown is deterministic under test.
pub(crate) fn build_importance_resource(data_dir: &Path, query: Option<&str>, now_secs: u64) -> String {
    let q = super::parse_query(query);

    if let Some(path) = q.get("path") {
        let snapshot = snapshot_path(data_dir, path, now_secs);
        return build_path_text(path, &snapshot);
    }

    let ranked_scope = RankedScope {
        volume: q.get("volume").map(String::as_str),
        under: q.get("under").map(String::as_str),
        among: None,
    };

    if let Some(top) = q.get("top") {
        let Ok(n) = top.parse::<usize>() else {
            return format!("Invalid `top` value '{top}': expected a whole number, for example `?top=20`.");
        };
        return match snapshot_top(data_dir, n, ranked_scope, now_secs) {
            Ok(folders) => {
                let scope = scope_label(ranked_scope);
                let header = format!(
                    "Top {} by importance{scope}:",
                    crate::pluralize::pluralize(n as u64, "folder")
                );
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
        return match snapshot_threshold(data_dir, value, ranked_scope, now_secs) {
            Ok((folders, truncated)) => {
                let scope = scope_label(ranked_scope);
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
pub(crate) mod tests;
