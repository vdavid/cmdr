//! A live search, end to end, over a drive the index has never seen.
//!
//! Nothing is faked below the search: a real `Index`, a real walk over a real
//! `Volume`, real rows in a real database, and the real arena loaded back off
//! disk. What the volume is made of is the only fiction — an `InMemoryVolume`, so
//! the test needs no disk, no share, and no permissions.
//!
//! It pins the two properties that fail SILENTLY:
//!
//! 1. **A search reads back what the walk wrote.** Convergence: the second search
//!    over the same folder answers from the index and returns the same files.
//! 2. **A search after a walk doesn't return FEWER results** (Decision 12). The
//!    arena is a snapshot; a walk writes rows behind it; the next query prunes
//!    that ground as covered. Without the invalidation the answer is empty, and
//!    nothing anywhere says so.

use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, Volume};

use cmdr_index::testing::host::{FakeVolumeProvider, test_lock};
use cmdr_index::{CoverageDimension, Index, NoopEventSink};

use super::*;
use crate::ignore_poison::IgnorePoison;
use crate::search::live::events::CollectorSearchEventSink;
use crate::search::live::{self, AnswerEnding, RunOrigin, SearchPhase, WalkEnding};

/// A platform-appropriate mount root: read routing only sends a path to a
/// per-mount index under an external-mount prefix, and those differ per OS.
#[cfg(target_os = "macos")]
const MOUNT_PREFIX: &str = "/Volumes";
#[cfg(not(target_os = "macos"))]
const MOUNT_PREFIX: &str = "/media";

const VOLUME_ID: &str = "live-search-e2e";

/// The drive: two branches, so one can be walked while the other is still
/// unknown to the index.
fn drive(root: &str) -> Arc<dyn Volume> {
    let entries = vec![
        FileEntry::new("a".into(), format!("{root}/a"), true, false),
        FileEntry::new("nested".into(), format!("{root}/a/nested"), true, false),
        FileEntry {
            size: Some(11),
            ..FileEntry::new("one.txt".into(), format!("{root}/a/one.txt"), false, false)
        },
        FileEntry {
            size: Some(22),
            ..FileEntry::new("two.txt".into(), format!("{root}/a/nested/two.txt"), false, false)
        },
        FileEntry::new("b".into(), format!("{root}/b"), true, false),
        // A NAS snapshot tree: rows, but nothing walks inside one (hardlinked per
        // snapshot, 44 TB reported on a 10 TB volume), so it's the settled
        // "nothing is coming for this" case the user has to be told about.
        FileEntry::new("@eaDir".into(), format!("{root}/b/@eaDir"), true, false),
        FileEntry {
            size: Some(44),
            ..FileEntry::new("hidden.txt".into(), format!("{root}/b/@eaDir/hidden.txt"), false, false)
        },
        FileEntry {
            size: Some(33),
            ..FileEntry::new("three.txt".into(), format!("{root}/b/three.txt"), false, false)
        },
    ];
    Arc::new(InMemoryVolume::with_entries("Live search", entries).with_root(root))
}

/// What one live search over `scope` came back with.
struct Answer {
    paths: Vec<String>,
    match_count: u32,
    walk: WalkEnding,
    phases: Vec<SearchPhase>,
    permission_denied: Vec<String>,
    declined: Vec<String>,
    unresolved: Vec<String>,
}

/// The one query every search below runs, scoped to `scope`.
fn query_for(scope: &str) -> SearchQuery {
    SearchQuery {
        name_pattern: Some("txt".to_string()),
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: Some(vec![scope.to_string()]),
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: Some(false),
        exclude_system_dirs: Some(false),
    }
}

/// Run one live search over `scope`, to completion, and report what the frontend
/// would have seen.
fn search(run_id: &str, scope: &str) -> Answer {
    let query = query_for(scope);
    let target = Target {
        volume_id: VOLUME_ID.to_string(),
        include_paths: vec![scope.to_string()],
        from_scope: true,
    };
    let run = live::register(run_id, VOLUME_ID, RunOrigin::Dialog);
    let sink = CollectorSearchEventSink::default();
    run_live_blocking(query, target, &run, &sink);
    live::deregister(run_id);

    let progress = sink.progress.lock_ignore_poison();
    let complete = sink.complete.lock_ignore_poison();
    assert!(
        sink.errors.lock_ignore_poison().is_empty(),
        "the run failed: {:?}",
        sink.errors.lock_ignore_poison()
    );
    let terminal = complete.first().expect("every run ends with a terminal event");
    let mut paths: Vec<String> = progress
        .iter()
        .flat_map(|event| event.entries.iter().map(|entry| entry.path.clone()))
        .collect();
    paths.sort();
    Answer {
        paths,
        match_count: terminal.match_count,
        walk: terminal.coverage.walk,
        phases: progress.iter().map(|event| event.phase).collect(),
        permission_denied: terminal.coverage.permission_denied.clone(),
        declined: terminal.coverage.declined.clone(),
        unresolved: terminal.coverage.unresolved_scopes.clone(),
    }
}

/// What one agent search over `scope` came back with, through the entry point
/// the MCP tools take.
struct AgentAnswer {
    paths: Vec<String>,
    match_count: u32,
    dirs_found: u64,
    ending: AnswerEnding,
}

/// Run one agent search over `scope`, waiting up to `budget` for it.
fn agent_search(scope: &str, budget: std::time::Duration) -> AgentAnswer {
    let mut query = query_for(scope);
    query.include_paths = Some(vec![scope.to_string()]);
    let answer = run_live_collected(query, budget).expect("routing resolves to the one fake volume");
    let mut paths: Vec<String> = answer.entries.iter().map(|entry| entry.path.clone()).collect();
    paths.sort();
    AgentAnswer {
        paths,
        match_count: answer.match_count,
        dirs_found: answer.dirs_found,
        ending: answer.ending,
    }
}

/// The coverage report of a settled answer.
fn settled(ending: &AnswerEnding) -> &SearchRunCoverage {
    match ending {
        AnswerEnding::Settled(coverage) => coverage,
        other => panic!("the run was expected to settle, and reported {other:?}"),
    }
}

#[test]
fn an_agent_search_walks_the_same_ground_and_gets_the_same_union() {
    // Decision 10: the MCP tools are a thin wrapper on the same path, so an
    // agent's search walks exactly like a person's. The transport is the only
    // difference — one reply instead of a stream — and this is what says the
    // ANSWER is the same one, over a volume where half the ground is indexed and
    // half is not.
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let root = format!("{MOUNT_PREFIX}/{VOLUME_ID}");

    let volumes = FakeVolumeProvider::shared();
    volumes.register(VOLUME_ID, drive(&root)).mark_network(&root);
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(NoopEventSink::shared())
        .install_for_test();

    // Half the drive indexed, by a person's search.
    let dialog = search("agent-e2e-dialog", &format!("{root}/a"));
    assert_eq!(dialog.walk, WalkEnding::Completed);

    // Now the agent asks for the whole drive: the arena answers for `a`, the walk
    // covers `b`, and the reply carries the union.
    let whole = agent_search(&root, std::time::Duration::from_secs(30));
    assert_eq!(
        whole.paths,
        vec![
            format!("{root}/a/nested/two.txt"),
            format!("{root}/a/one.txt"),
            format!("{root}/b/three.txt"),
        ],
        "every file once, from whichever half found it"
    );
    assert_eq!(whole.match_count, 3, "and counted once each");
    let coverage = settled(&whole.ending);
    assert_eq!(coverage.walk, WalkEnding::Completed);
    assert!(whole.dirs_found > 0, "the agent's search really walked");
    // The typed coverage signal MCP renders, carried through the fold: M8's
    // declined list reaches an agent exactly as it reaches the dialog.
    assert_eq!(coverage.declined, vec![format!("{root}/b/@eaDir")]);
    assert!(coverage.permission_denied.is_empty());
    assert!(coverage.unresolved_scopes.is_empty());

    // And the agent's walk converged like a person's: the same search again has
    // nothing left to walk.
    let again = agent_search(&root, std::time::Duration::from_secs(30));
    assert_eq!(settled(&again.ending).walk, WalkEnding::NothingToWalk);
    assert_eq!(again.paths, whole.paths);
    assert_eq!(again.dirs_found, 0, "an index-served answer walked nothing");

    volumes::forget_volume_for_test(VOLUME_ID);
    let _ = index.forget_volume(VOLUME_ID);
}

#[test]
fn an_agent_that_stops_waiting_does_not_stop_the_walk() {
    // The half of "streaming" a one-shot transport can still carry: handing back
    // an answer is not a cancel. The walk's rows land in the index either way, so
    // the next call picks up ground this one never saw — which is what makes
    // "run it again" honest advice rather than "start over".
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let root = format!("{MOUNT_PREFIX}/{VOLUME_ID}");

    let volumes = FakeVolumeProvider::shared();
    volumes.register(VOLUME_ID, drive(&root)).mark_network(&root);
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(NoopEventSink::shared())
        .install_for_test();

    // A budget too short for any run to settle in.
    let cut_short = agent_search(&root, std::time::Duration::from_millis(1));
    assert!(
        matches!(cut_short.ending, AnswerEnding::StillWalking),
        "the answer says the walk is still going: {:?}",
        cut_short.ending
    );

    // The walk nobody is waiting for covers the drive anyway.
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(30),
        "the abandoned walk to cover its frontier",
        || {
            index
                .coverage(VOLUME_ID, &root, CoverageDimension::Listing)
                .is_ok_and(|map| map.frontier.is_empty())
        },
    );

    // So the next call answers from the index, in full.
    let next = agent_search(&root, std::time::Duration::from_secs(30));
    assert_eq!(settled(&next.ending).walk, WalkEnding::NothingToWalk);
    assert_eq!(
        next.paths,
        vec![
            format!("{root}/a/nested/two.txt"),
            format!("{root}/a/one.txt"),
            format!("{root}/b/three.txt"),
        ],
        "and it holds everything the abandoned walk wrote"
    );

    volumes::forget_volume_for_test(VOLUME_ID);
    let _ = index.forget_volume(VOLUME_ID);
}

#[test]
fn a_drive_with_no_index_is_walked_live_then_read_back_from_what_the_walk_wrote() {
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let root = format!("{MOUNT_PREFIX}/{VOLUME_ID}");

    let volumes = FakeVolumeProvider::shared();
    volumes.register(VOLUME_ID, drive(&root)).mark_network(&root);
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(NoopEventSink::shared())
        .install_for_test();

    // 1. Nothing is indexed, so the whole scope is frontier and every result
    //    comes off the walk.
    let first = search("e2e-1", &format!("{root}/a"));
    assert_eq!(
        first.paths,
        vec![format!("{root}/a/nested/two.txt"), format!("{root}/a/one.txt")],
        "the walk found both files under the scope"
    );
    assert_eq!(first.walk, WalkEnding::Completed);
    assert_eq!(first.match_count, 2);
    assert!(
        first.phases.contains(&SearchPhase::ResolvingCoverage) && first.phases.contains(&SearchPhase::Walking),
        "and said which phase it was in: {:?}",
        first.phases
    );

    // 2. Convergence: the same search again has nothing left to walk, and the
    //    index answers for what the first one covered.
    let second = search("e2e-2", &format!("{root}/a"));
    assert_eq!(second.walk, WalkEnding::NothingToWalk, "the frontier is gone");
    assert_eq!(second.paths, first.paths, "and the answer is the same one");
    assert!(
        !second.phases.contains(&SearchPhase::Walking),
        "with no walk at all: {:?}",
        second.phases
    );

    // The arena is warm now, and about to go out of date.
    // 3. A different branch, never listed: walked live, its rows landing behind
    //    the arena the search above loaded.
    let third = search("e2e-3", &format!("{root}/b"));
    assert_eq!(third.paths, vec![format!("{root}/b/three.txt")]);
    assert_eq!(third.walk, WalkEnding::Completed);
    // The scope had no row at all — the index couldn't resolve it — and the walk
    // has now been there. ❌ Not reported as "Cmdr doesn't cover this folder": the
    // walk IS the probe, and it just answered, which is the whole point of the
    // milestone.
    assert!(
        third.unresolved.is_empty(),
        "a folder the walk covered is not an unresolved scope: {:?}",
        third.unresolved
    );
    // And the walk says what it won't read, in the same breath it says it
    // finished: the snapshot tree it just stamped, not silence. Read back AFTER
    // the walk, because nothing had tried before it.
    assert_eq!(
        third.declined,
        vec![format!("{root}/b/@eaDir")],
        "a directory nothing is coming for is reported, never swallowed"
    );
    assert!(
        third.permission_denied.is_empty(),
        "and as ground Cmdr declines to read, never as a permission the user could grant: {:?}",
        third.permission_denied
    );

    // 4. THE anchor. `b` now reads as covered, so nothing walks it — and the
    //    warm arena predates every row in it. Without Decision 12's reload this
    //    comes back empty, silently, one keystroke after showing the file.
    let fourth = search("e2e-4", &format!("{root}/b"));
    assert_eq!(fourth.walk, WalkEnding::NothingToWalk);
    assert_eq!(
        fourth.paths,
        vec![format!("{root}/b/three.txt")],
        "the second search after a walk must not return fewer results than the first"
    );

    // 5. The union. The volume root itself was never listed (only materialized so
    //    the walk had somewhere to start), so a search of the whole drive walks it
    //    while the arena already holds the rows underneath — each file exactly
    //    once, counted once.
    let whole = search("e2e-5", &root);
    assert_eq!(
        whole.paths,
        vec![
            format!("{root}/a/nested/two.txt"),
            format!("{root}/a/one.txt"),
            format!("{root}/b/three.txt"),
        ],
        "every file once, from whichever half found it"
    );
    assert_eq!(whole.match_count, 3, "and counted once each");

    // 6. And now the whole drive is covered, so a search of it walks nothing.
    let settled = search("e2e-6", &root);
    assert_eq!(settled.walk, WalkEnding::NothingToWalk);
    assert_eq!(settled.paths, whole.paths);
    assert!(
        index
            .coverage(VOLUME_ID, &root, CoverageDimension::Listing)
            .expect("the drive answers for its own coverage")
            .frontier
            .is_empty(),
        "searching a drive is what indexed it"
    );

    volumes::forget_volume_for_test(VOLUME_ID);
    let _ = index.forget_volume(VOLUME_ID);
}
