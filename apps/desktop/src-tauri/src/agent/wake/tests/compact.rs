//! Digest compaction: fitting what happened into a hard token budget without lying about
//! what got left out.

use super::super::*;
use super::bundle;

/// Room for everything these tests throw at it.
const ROOMY: usize = 4_000;

/// One scored folder, with `created` arrivals and a given interest.
fn scored(folder: &str, created: u32, interest: f64) -> ScoredBundle {
    ScoredBundle {
        bundle: bundle(
            folder,
            ChangeCounters {
                created,
                ..ChangeCounters::default()
            },
        ),
        interest: Interest::of(interest),
    }
}

/// Every folder the digest accounts for, whether by its own line or inside a rollup.
fn folders_accounted_for(digest: &Digest) -> usize {
    digest.lines.len() + digest.rollups.iter().map(|r| r.folders as usize).sum::<usize>()
}

/// With room to spare, every folder gets its own line and nothing is rolled up. The rollup
/// machinery is for pressure, and it must not fire when there is none.
#[test]
fn everything_that_fits_gets_its_own_line() {
    let digest = compact(
        &[
            scored("/Users/someone/Downloads", 5, 0.9),
            scored("/Users/someone/Documents", 2, 0.5),
        ],
        ROOMY,
    );

    assert_eq!(digest.lines.len(), 2);
    assert!(digest.rollups.is_empty(), "no pressure, no rollups: {digest:?}");
    assert_eq!(
        digest.lines[0].folder, "/Users/someone/Downloads",
        "most interesting first"
    );
}

/// The budget is spent in INTEREST order, not arrival order. Otherwise a noisy, uninteresting
/// folder that happened to be coalesced first eats the budget and the one thing worth waking
/// for is the thing that gets rolled up.
#[test]
fn the_budget_goes_to_the_most_interesting_folders_first() {
    let noisy_first = [
        scored("/tmp/build-noise-one", 900, 0.10),
        scored("/tmp/build-noise-two", 900, 0.11),
        scored("/tmp/build-noise-three", 900, 0.12),
        scored("/Users/someone/Downloads", 3, 0.95),
    ];

    // Room for roughly one line plus a rollup, so the ordering decides who gets the line.
    let digest = compact(&noisy_first, 40);

    assert_eq!(
        digest.lines.first().map(|l| l.folder.as_str()),
        Some("/Users/someone/Downloads"),
        "the interesting folder must get the line: {digest:?}"
    );
}

/// What doesn't fit is rolled up and COUNTED, never dropped. The agent is entitled to know how
/// much it isn't being shown; a digest that silently truncated would have it reason about a
/// partial picture as if it were whole.
#[test]
fn what_does_not_fit_is_rolled_up_rather_than_dropped() {
    let many: Vec<ScoredBundle> = (0..60)
        .map(|i| scored(&format!("/Users/someone/code/project-{i:03}/src"), 4, 0.4))
        .collect();

    let digest = compact(&many, 200);

    assert!(!digest.rollups.is_empty(), "60 folders in 200 tokens must roll up");
    assert_eq!(
        folders_accounted_for(&digest),
        60,
        "every folder is either a line or inside a rollup: {digest:?}"
    );
    let rolled_changes: u64 = digest.rollups.iter().map(|r| r.counters.total()).sum();
    let line_changes: u64 = digest.lines.iter().map(|l| l.counters.total()).sum();
    assert_eq!(rolled_changes + line_changes, 60 * 4, "no change goes uncounted");
}

/// A rollup is anchored at a shared ancestor, so the line reads as a place the user recognizes
/// rather than as an arbitrary bag of folders.
#[test]
fn a_rollup_is_anchored_at_a_shared_ancestor() {
    let siblings: Vec<ScoredBundle> = (0..40)
        .map(|i| scored(&format!("/Users/someone/code/pkg-{i:03}"), 3, 0.3))
        .collect();

    let digest = compact(&siblings, 120);
    let rollup = digest.rollups.first().expect("something rolled up");

    assert!(
        rollup.ancestor.starts_with("/Users/someone/code"),
        "anchored at the shared parent, got {}",
        rollup.ancestor
    );
}

/// THE budget property, across every shape: the rendered digest never exceeds what it was
/// given. Not with lines, not with the rollup lines that describe the leftovers, and not when
/// the budget is far too small to say anything at all.
#[test]
fn the_digest_never_exceeds_its_budget() {
    let many: Vec<ScoredBundle> = (0..500)
        .map(|i| scored(&format!("/Users/someone/deep/nest-{i:04}/inner/leaf"), 7, 0.5))
        .collect();

    for budget in [0, 1, 5, 20, 100, 1_000, 10_000] {
        let digest = compact(&many, budget);
        let rendered = digest.render();
        let cost = crate::agent::chat::budget::estimate_tokens_str(&rendered);
        assert!(
            cost <= budget,
            // allowed-pluralize-noun: a test diagnostic, not user copy; a blown budget is never one token.
            "budget {budget} exceeded: rendered {cost} tokens ({} lines, {} rollups)",
            digest.lines.len(),
            digest.rollups.len()
        );
    }
}

/// A budget too small for even one rollup line yields an EMPTY digest rather than an overrun.
/// Nothing to say is a legitimate answer to an impossible budget; going over it is not, since
/// the overrun would push the rest of the turn out of the prompt.
#[test]
fn an_impossible_budget_yields_nothing_rather_than_an_overrun() {
    let digest = compact(&[scored("/Users/someone/Downloads", 5, 0.9)], 1);

    assert!(digest.render().is_empty(), "got: {:?}", digest.render());
    assert!(digest.lines.is_empty());
    assert!(digest.rollups.is_empty());
}

/// Nothing happened, nothing to say — and no empty scaffolding either, which would spend
/// budget telling the agent there was nothing to tell it.
#[test]
fn an_empty_input_produces_an_empty_digest() {
    let digest = compact(&[], ROOMY);

    assert!(digest.lines.is_empty());
    assert!(digest.rollups.is_empty());
    assert!(digest.render().is_empty());
}

/// The rendered digest names the folder and its counts, because those two facts are the whole
/// point: WHERE something happened and HOW MUCH. The agent pulls the file names itself with a
/// `list_dir` if it decides it needs them.
#[test]
fn a_line_names_the_folder_and_what_happened_in_it() {
    let digest = compact(&[scored("/Users/someone/Downloads", 12, 0.9)], ROOMY);
    let rendered = digest.render();

    assert!(rendered.contains("/Users/someone/Downloads"), "got: {rendered}");
    assert!(rendered.contains("12"), "the count has to be in there: {rendered}");
}
