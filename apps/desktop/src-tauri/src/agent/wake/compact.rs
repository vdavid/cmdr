//! Digest compaction: what the agent is told when it wakes, inside a hard token budget.
//!
//! The budget is the whole difficulty. A wake can cover two folders or two thousand, and the
//! digest rides in the same prompt as the agent's tools and its instructions, so a digest that
//! overran would push the rest of the turn out of the window — the same failure that once cost
//! a rename turn the evidence it was reasoning from (`agent/chat/DETAILS.md`).
//!
//! Three properties hold at every budget and input size, and the tests pin each one:
//!
//! - **The rendered digest never exceeds its budget.** Lines, rollups, and the degenerate case
//!   where nothing fits at all.
//! - **The budget is spent in interest order**, so noise cannot crowd out the one folder worth
//!   waking for.
//! - **Nothing is silently dropped.** Whatever misses a line is rolled up and COUNTED, so the
//!   agent knows the size of what it is not seeing.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::{ChangeCounters, EventBundle, Interest};
use crate::agent::chat::budget::estimate_tokens_str;

/// Past this many distinct parents, per-parent rollups stop being a summary and become their
/// own wall of text, so the leftovers collapse into ONE line at their common ancestor instead.
const MAX_ROLLUP_GROUPS: usize = 3;

/// One bundle with its interest score: what the compactor ranks by.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredBundle {
    pub bundle: EventBundle,
    pub interest: Interest,
}

/// One folder that earned its own line.
#[derive(Debug, Clone, PartialEq)]
pub struct DigestLine {
    pub folder: String,
    pub counters: ChangeCounters,
    pub interest: Interest,
}

/// Folders that did not fit, summarized under a shared ancestor.
#[derive(Debug, Clone, PartialEq)]
pub struct Rollup {
    pub ancestor: String,
    pub folders: u32,
    pub counters: ChangeCounters,
}

/// What the agent reads when it wakes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Digest {
    pub lines: Vec<DigestLine>,
    pub rollups: Vec<Rollup>,
}

impl Digest {
    /// The digest as the model reads it. An empty digest renders an empty string, never a
    /// header saying there is nothing to report: that would spend budget to say nothing.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&render_line(line));
        }
        for rollup in &self.rollups {
            out.push_str(&render_rollup(rollup));
        }
        out
    }
}

/// Fit scored bundles into `budget_tokens`, rolling up whatever does not fit.
pub fn compact(scored: &[ScoredBundle], budget_tokens: usize) -> Digest {
    if scored.is_empty() {
        return Digest::default();
    }
    let mut ranked: Vec<&ScoredBundle> = scored.iter().collect();
    // Interest first, then folder, so the answer is deterministic even when scores tie.
    ranked.sort_by(|a, b| {
        b.interest
            .value()
            .partial_cmp(&a.interest.value())
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.bundle.folder.cmp(&b.bundle.folder))
    });

    // How many lines the budget holds before the rollups are accounted for. An upper bound,
    // not the answer: it exists so the shrink below starts near the fit rather than at the top
    // of a two-thousand-folder list.
    let mut kept = 0;
    let mut spent = 0;
    for candidate in &ranked {
        spent += estimate_tokens_str(&render_line(&line_of(candidate)));
        if spent > budget_tokens {
            break;
        }
        kept += 1;
    }

    // Then give lines back until the WHOLE digest fits, rollups included. Measuring the real
    // rendered string each time is what makes the budget a fact rather than an estimate of an
    // estimate: per-line costs do not sum to the cost of the whole.
    loop {
        let digest = build(&ranked, kept);
        if estimate_tokens_str(&digest.render()) <= budget_tokens {
            return digest;
        }
        if kept == 0 {
            // Not even one rollup line fits. Saying nothing is a legitimate answer to an
            // impossible budget; overrunning it is not.
            return Digest::default();
        }
        kept -= 1;
    }
}

/// The digest with the top `kept` folders as lines and everything else rolled up.
fn build(ranked: &[&ScoredBundle], kept: usize) -> Digest {
    Digest {
        lines: ranked[..kept].iter().map(|scored| line_of(scored)).collect(),
        rollups: roll_up(&ranked[kept..]),
    }
}

fn line_of(scored: &ScoredBundle) -> DigestLine {
    DigestLine {
        folder: scored.bundle.folder.clone(),
        counters: scored.bundle.counters,
        interest: scored.interest,
    }
}

/// Summarize the folders that missed a line, by shared parent — or, when there are too many
/// parents for that to read as a summary, as one line at the common ancestor.
fn roll_up(leftovers: &[&ScoredBundle]) -> Vec<Rollup> {
    if leftovers.is_empty() {
        return Vec::new();
    }
    let mut by_parent: BTreeMap<&str, Rollup> = BTreeMap::new();
    for scored in leftovers {
        let parent = parent_of(&scored.bundle.folder);
        let rollup = by_parent.entry(parent).or_insert_with(|| Rollup {
            ancestor: parent.to_string(),
            folders: 0,
            counters: ChangeCounters::default(),
        });
        rollup.folders = rollup.folders.saturating_add(1);
        rollup.counters.merge(&scored.bundle.counters);
    }
    if by_parent.len() <= MAX_ROLLUP_GROUPS {
        return by_parent.into_values().collect();
    }

    let mut all = Rollup {
        ancestor: common_ancestor(leftovers.iter().map(|scored| scored.bundle.folder.as_str())),
        folders: 0,
        counters: ChangeCounters::default(),
    };
    for scored in leftovers {
        all.folders = all.folders.saturating_add(1);
        all.counters.merge(&scored.bundle.counters);
    }
    vec![all]
}

/// The folder holding `path`. The root holds itself, so a top-level folder rolls up to the
/// root rather than to nothing.
fn parent_of(path: &str) -> &str {
    match path.trim_end_matches('/').rfind('/') {
        Some(0) | None => "/",
        Some(cut) => &path[..cut],
    }
}

/// The deepest folder every path lies under, compared by COMPONENT so that two siblings share
/// their parent rather than the longest matching prefix of their names.
fn common_ancestor<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let mut shared: Option<Vec<&str>> = None;
    for path in paths {
        let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        shared = Some(match shared {
            None => parts,
            Some(current) => current
                .iter()
                .zip(parts.iter())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| *a)
                .collect(),
        });
    }
    let joined = shared.unwrap_or_default().join("/");
    if joined.is_empty() { "/".to_string() } else { joined }
}

/// One folder line: where, and what happened there.
fn render_line(line: &DigestLine) -> String {
    format!("{}: {}\n", line.folder, counts_text(&line.counters))
}

/// One rollup line. The count is the point: it is how the agent knows the size of what it is
/// not being shown.
fn render_rollup(rollup: &Rollup) -> String {
    format!(
        "+ {} more folders under {}: {} changes\n",
        rollup.folders,
        rollup.ancestor,
        rollup.counters.total()
    )
}

/// Only the kinds that actually happened: a line of zeroes is budget spent on nothing.
fn counts_text(counters: &ChangeCounters) -> String {
    let parts = [
        (counters.created, "new"),
        (counters.modified, "changed"),
        (counters.removed, "removed"),
        (counters.renamed, "renamed"),
    ];
    let text = parts
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, label)| format!("{count} {label}"))
        .collect::<Vec<_>>()
        .join(", ");
    if text.is_empty() {
        "no changes".to_string()
    } else {
        text
    }
}
