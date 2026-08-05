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
use crate::search::live::{self, SearchPhase, WalkEnding};

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
    unreadable: Vec<String>,
    unresolved: Vec<String>,
}

/// Run one live search over `scope`, to completion, and report what the frontend
/// would have seen.
fn search(run_id: &str, scope: &str) -> Answer {
    let query = SearchQuery {
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
    };
    let target = Target {
        volume_id: VOLUME_ID.to_string(),
        include_paths: vec![scope.to_string()],
        from_scope: true,
    };
    let run = live::register(run_id, VOLUME_ID);
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
        unreadable: terminal.coverage.unreadable.clone(),
        unresolved: terminal.coverage.unresolved_scopes.clone(),
    }
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
        third.unreadable,
        vec![format!("{root}/b/@eaDir")],
        "a directory nothing is coming for is reported, never swallowed"
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
