//! The interest scorer: how much a bundle is worth waking the agent for, and how soon.

use std::time::Duration;

use super::super::*;
use super::bundle;

/// A bundle of `created` files in a folder, the shape the flagship scenario produces.
fn arrivals(folder: &str, created: u32) -> EventBundle {
    bundle(
        folder,
        ChangeCounters {
            created,
            ..ChangeCounters::default()
        },
    )
}

/// A bundle of pure churn: a build or a log writing to itself.
fn churn(folder: &str, modified: u32) -> EventBundle {
    bundle(
        folder,
        ChangeCounters {
            modified,
            ..ChangeCounters::default()
        },
    )
}

/// THE load-bearing case for the whole stage. `WeightLookup` answers three ways and its
/// `score()` collapses two of them to `0.0`; the scorer must NOT inherit that collapse.
///
/// A folder the importance scorer hasn't reached yet (a project cloned five minutes ago, a
/// volume still scanning) is UNKNOWN, not junk. If unknown collapsed into zero it would score
/// identically to `node_modules`, and the agent would silently ignore every new project folder
/// on the disk — a bug that reads as "the agent just isn't very good".
#[test]
fn an_unknown_folder_outranks_a_floored_one_on_identical_change() {
    let new_project = interest(
        &arrivals("/Users/someone/code/fresh-clone", 8),
        FolderImportance::Unknown,
    );
    let junk = interest(
        &arrivals("/Users/someone/code/thing/node_modules", 8),
        FolderImportance::Floored,
    );

    assert!(
        new_project.value() > junk.value(),
        "unknown ({}) must outrank floored ({}): they are different answers, not the same zero",
        new_project.value(),
        junk.value()
    );
    assert!(
        new_project.value() > 0.0,
        "an unknown folder that saw real change scores something, or it can never be surfaced"
    );
}

/// Floored folders are deliberately-junk ground (`node_modules`, `.git`, caches). Change there
/// is worth knowing about at zero urgency, never at speed.
#[test]
fn a_floored_folder_scores_zero_however_much_happens_in_it() {
    let quiet = interest(&churn("/x/node_modules", 3), FolderImportance::Floored);
    let storm = interest(&churn("/x/node_modules", 5_000_000), FolderImportance::Floored);

    assert_eq!(quiet.value(), 0.0);
    assert_eq!(storm.value(), 0.0, "a build storm in junk is still junk");
    assert_eq!(
        wake_delay(storm),
        COLD_DELAY,
        "it rides along on the next wake rather than causing one"
    );
}

/// Intent beats churn at equal volume: files APPEARING is the signal the agent acts on, files
/// being written to again is the noise it exists to absorb.
#[test]
fn arrivals_outrank_churn_at_the_same_volume() {
    let appeared = interest(&arrivals("/Users/someone/Downloads", 20), FolderImportance::Scored(0.8));
    let rewritten = interest(&churn("/Users/someone/Downloads", 20), FolderImportance::Scored(0.8));

    assert!(
        appeared.value() > rewritten.value(),
        "20 new files ({}) must beat 20 rewrites ({})",
        appeared.value(),
        rewritten.value()
    );
}

/// The flagship scenario: ONE file landing in Downloads is hot. If a single arrival in an
/// important folder didn't wake the agent promptly, the feature's headline case would take an
/// hour to notice.
#[test]
fn one_arrival_in_an_important_folder_is_hot() {
    let downloads = interest(&arrivals("/Users/someone/Downloads", 1), FolderImportance::Scored(0.9));

    assert!(downloads.value() >= HOT_THRESHOLD, "scored {}", downloads.value());
    assert_eq!(wake_delay(downloads), HOT_DELAY);
}

/// Volume saturates rather than running away: the difference between 50 and 5,000,000 changes
/// is real but bounded, so one pathological folder can't out-shout every other bundle in the
/// inbox and monopolize the digest.
#[test]
fn volume_saturates_instead_of_running_away() {
    let some = interest(&churn("/tmp/log", 50), FolderImportance::Scored(0.5));
    let flood = interest(&churn("/tmp/log", 5_000_000), FolderImportance::Scored(0.5));

    assert!(flood.value() > some.value(), "more change is more interesting");
    assert!(
        flood.value() <= 1.0,
        "interest stays in 0..=1 however extreme the input: {}",
        flood.value()
    );
}

/// An empty bundle is worth nothing, in any folder. Nothing happened, so there is nothing to
/// tell the agent, and a nonzero score here would wake it for silence.
#[test]
fn a_bundle_with_no_changes_scores_zero() {
    let empty = bundle("/Users/someone/Downloads", ChangeCounters::default());
    assert_eq!(interest(&empty, FolderImportance::Scored(1.0)).value(), 0.0);
    assert_eq!(interest(&empty, FolderImportance::Unknown).value(), 0.0);
}

/// The scorer is a pure function of its two values: same bundle, same importance, same answer,
/// forever. The deterministic layer is what makes the agent's behaviour reproducible and its
/// tests meaningful, and it's why no clock or store reaches in here.
#[test]
fn the_same_inputs_always_score_the_same() {
    let bundle = arrivals("/Users/someone/Downloads", 7);
    let first = interest(&bundle, FolderImportance::Scored(0.6));
    let second = interest(&bundle, FolderImportance::Scored(0.6));

    assert_eq!(first.value(), second.value());
}

/// More interest never means a longer wait. The tiers are coarse (§6.2's hot / warm / cold)
/// but they have to be monotonic, or a hotter bundle could sit behind a colder one.
#[test]
fn a_hotter_bundle_never_waits_longer_than_a_colder_one() {
    let scores = [0.0, 0.1, 0.3, 0.5, 0.7, 0.9, 1.0];
    let delays: Vec<Duration> = scores.iter().map(|s| wake_delay(Interest::of(*s))).collect();

    for pair in delays.windows(2) {
        assert!(
            pair[0] >= pair[1],
            "delays must fall as interest rises, got {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
    assert_eq!(delays[0], COLD_DELAY);
    assert_eq!(delays[delays.len() - 1], HOT_DELAY);
}
