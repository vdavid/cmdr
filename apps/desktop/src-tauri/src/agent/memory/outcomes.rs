//! The one file in memory the agent does not author: what the user did with its suggestions.
//!
//! Every other write here happens because a model chose to make it. This one happens
//! MECHANICALLY, once per decided proposal, with no model call in the loop
//! (`agent/outcomes.rs`). That difference is the whole reason this file exists rather than the
//! lines being appended to the hub:
//!
//! - ⚠️ **A refusal on this path has nobody to relay it to.** The hub's `DirectoryFull` reaches
//!   the model as a typed token telling it to prune; a mechanical write has no turn to answer
//!   in. So this file can never be refused: it is a fixed-size RING, rewritten whole, and the
//!   tool-facing cap is lowered by exactly the room it reserves ([`OUTCOMES_MAX_BYTES`]), so
//!   the model can never eat the space the lessons need.
//! - **It can never crowd the hub out of the prompt either.** `read_for_prompt` gives the ring
//!   a bounded share and the hub the rest, so a busy week of approvals cannot displace the
//!   notes the agent wrote about the person.
//!
//! It is auto-fed like the hub, which is what keeps `../DETAILS.md`'s "no read or list tool"
//! rule true with two files instead of one: nothing here has to be discovered to be read. The
//! model may still prune it with `memory_edit` like any other note, and "Forget everything"
//! takes it with the rest.

use super::store::{MEMORY_DIR_MAX_BYTES, MemoryStore};

const LOG_TARGET: &str = "agent::memory";

/// Where the decision log lives, beside the hub.
pub const OUTCOMES_FILE: &str = "outcomes.md";

/// The most the decision log may hold on disk, reserved out of [`MEMORY_DIR_MAX_BYTES`].
///
/// ⚠️ It is a RESERVE, not just a cap: `MemoryStore::write` and `MemoryStore::edit` price
/// against the folder cap MINUS this, so a model that fills its own memory cannot silence the
/// one channel that teaches it what the user actually wants.
pub const OUTCOMES_MAX_BYTES: usize = 4 * 1024;

/// How many decisions the log keeps, newest last.
///
/// A count as well as a byte cap because the two answer different questions: the bytes keep the
/// prompt and the disk honest, and the count keeps the log RECENT. A lesson from two hundred
/// decisions ago has either been distilled into the hub by now or was never worth keeping.
pub const OUTCOMES_MAX_ENTRIES: usize = 40;

/// The heading the ring is rewritten under, so the model reads the block as a record rather
/// than as instructions. Prompt-only text: this file never reaches the UI.
const HEADING: &str = "# What the user did with the agent's suggestions";

/// The marker every entry line starts with. Anything else in the file is prose the model added
/// and is carried through untouched, which is what lets it annotate its own log.
const ENTRY_PREFIX: &str = "- ";

impl MemoryStore {
    /// Fold one decision into the ring and rewrite it.
    ///
    /// ❌ **Never returns a refusal the caller has to interpret.** The ring is bounded below the
    /// reserve, so the only way this fails is the disk itself, and there is no model turn in
    /// this path to tell about it. A failure is logged and swallowed: the user's answer is
    /// already recorded in `main.db`, and a lost lesson costs a re-proposal, never correctness.
    pub fn record_outcome(&self, entry: &str) {
        let existing = std::fs::read_to_string(self.root().join(OUTCOMES_FILE)).unwrap_or_default();
        let next = fold(&existing, entry);
        if let Err(e) = self.store_capped(OUTCOMES_FILE, &next, MEMORY_DIR_MAX_BYTES) {
            log::warn!(target: LOG_TARGET, "the agent did not get to learn from a decision: {e:?}");
        }
    }

    /// The decision log's text for a turn's prefix, cut to `max_bytes` from the OLD end.
    ///
    /// ⚠️ Cut from the front, ❌ not the back like the hub: the hub's head is the model's own
    /// summary of the person and its tail is detail, while here the tail is the freshest
    /// lesson and the head is the one already superseded.
    pub(super) fn read_outcomes_for_prompt(&self, max_bytes: usize) -> Option<String> {
        let text = std::fs::read_to_string(self.root().join(OUTCOMES_FILE)).ok()?;
        let kept = trim_to(entries_of(&text), max_bytes.saturating_sub(HEADING.len() + 2));
        if kept.is_empty() {
            return None;
        }
        Some(render(&kept))
    }
}

/// Add one entry and drop whatever no longer fits. Pure, because the two caps and the eviction
/// order are the whole of what this file promises.
fn fold(existing: &str, entry: &str) -> String {
    let mut entries = entries_of(existing);
    entries.push(one_line(entry));
    render(&trim_to(entries, OUTCOMES_MAX_BYTES.saturating_sub(HEADING.len() + 2)))
}

/// The entry lines a stored ring holds, oldest first. Anything that is not an entry line (the
/// heading, a blank, a note the model wrote in here) is dropped on rewrite: the file is the
/// ring's, and keeping stray prose would grow it without bound.
fn entries_of(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.starts_with(ENTRY_PREFIX))
        .map(|line| line.to_string())
        .collect()
}

/// Collapse an entry onto one line, prefixed. A newline inside it would split one decision into
/// two ring entries, and one of the halves would then be evicted on its own.
fn one_line(entry: &str) -> String {
    let flattened = entry.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("{ENTRY_PREFIX}{flattened}")
}

/// Drop from the OLD end until the ring fits both caps.
fn trim_to(mut entries: Vec<String>, max_bytes: usize) -> Vec<String> {
    while entries.len() > OUTCOMES_MAX_ENTRIES {
        entries.remove(0);
    }
    while !entries.is_empty() && rendered_len(&entries) > max_bytes {
        entries.remove(0);
    }
    entries
}

fn rendered_len(entries: &[String]) -> usize {
    entries.iter().map(|entry| entry.len() + 1).sum()
}

fn render(entries: &[String]) -> String {
    format!("{HEADING}\n\n{}\n", entries.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: usize) -> String {
        format!("2026-08-23 rejected: move 12 files under /Users/x/Downloads (entry {n})")
    }

    /// The ring is what makes a mechanical writer safe, so its bound is the test that matters:
    /// however many decisions land, the file stays inside the reserve it was given.
    #[test]
    fn the_log_never_grows_past_its_reserve_however_many_decisions_land() {
        let mut text = String::new();
        for n in 0..500 {
            text = fold(&text, &entry(n));
            assert!(
                text.len() <= OUTCOMES_MAX_BYTES,
                // allowed-pluralize-noun: assertion message; the loop only reaches it past the cap, never at n == 1.
                "the ring reached {} bytes after {n} decisions",
                text.len()
            );
        }
    }

    /// Both caps bite, and the entry count is the one that bites first on short lines. Without
    /// it a hundred terse decisions would ride every turn for as long as they fit.
    #[test]
    fn the_log_keeps_the_newest_decisions_and_forgets_the_oldest() {
        let mut text = String::new();
        for n in 0..(OUTCOMES_MAX_ENTRIES + 5) {
            text = fold(&text, &entry(n));
        }

        assert_eq!(entries_of(&text).len(), OUTCOMES_MAX_ENTRIES);
        assert!(text.contains("entry 44"), "the newest decision is there");
        assert!(!text.contains("entry 0)"), "the oldest was evicted");
    }

    /// ⚠️ A decision with a newline in it would otherwise become two ring entries, and eviction
    /// would eventually keep one half of a sentence and drop the other.
    #[test]
    fn a_multiline_decision_becomes_one_entry() {
        let text = fold("", "rejected: move\n12 files\n");

        assert_eq!(entries_of(&text), vec!["- rejected: move 12 files".to_string()]);
    }

    /// The heading is rewritten rather than accumulated: a fold that kept the old one would
    /// stack a heading per decision until the ring held nothing else.
    #[test]
    fn folding_rewrites_the_heading_rather_than_stacking_it() {
        let once = fold("", &entry(1));
        let twice = fold(&once, &entry(2));

        assert_eq!(twice.matches(HEADING).count(), 1);
    }

    /// The prompt slice cuts from the OLD end, so the lesson a turn carries is the freshest
    /// one. Cutting the hub's way round would feed the agent exactly what it has already
    /// learned from.
    #[test]
    fn the_prompt_slice_keeps_the_newest_end() {
        let mut entries = Vec::new();
        for n in 0..10 {
            entries.push(one_line(&entry(n)));
        }
        let kept = trim_to(entries, 200);

        assert!(!kept.is_empty(), "something survives a small slice");
        assert!(
            kept.last().expect("an entry").contains("entry 9"),
            "the newest entry is the one kept"
        );
    }
}
