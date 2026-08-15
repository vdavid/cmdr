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

use std::future::Future;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, Volume};

use cmdr_index::testing::host::{FakeVolumeProvider, test_lock};
use cmdr_index::{CoverageDimension, Index, NoopEventSink};

use super::super::coverage::{AfterAnotherWalk, arena_for_coverage, coverage_of};
use super::super::live_run::run_live_blocking;
use super::*;
use crate::ignore_poison::IgnorePoison;
use crate::search::live::events::CollectorSearchEventSink;
use crate::search::live::{self, AnswerEnding, CoverageKind, RunOrigin, SearchPhase, SearchRunCoverage, WalkEnding};

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
    Arc::new(InMemoryVolume::with_entries("Live search", drive_entries(root)).with_root(root))
}

/// What's on it.
fn drive_entries(root: &str) -> Vec<FileEntry> {
    vec![
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
    ]
}

/// The same drive with ONE listing that blocks until the test lets it go.
///
/// That's what holds a walk — and with it, its frontier claim — open for as long
/// as a second search needs to meet it. A walk over an in-memory drive is
/// otherwise over before anything else can observe it.
struct GatedDrive {
    inner: InMemoryVolume,
    gate: std::path::PathBuf,
    reached: std::sync::atomic::AtomicBool,
    released: tokio::sync::Notify,
}

impl GatedDrive {
    fn new(root: &str, gate: &str) -> Self {
        Self {
            inner: InMemoryVolume::with_entries("Live search", drive_entries(root)).with_root(root),
            gate: std::path::PathBuf::from(gate),
            reached: std::sync::atomic::AtomicBool::new(false),
            released: tokio::sync::Notify::new(),
        }
    }

    /// Block until the walk has parked on the gated listing.
    fn wait_until_reached(&self) {
        cmdr_fs::testing::wait_until(std::time::Duration::from_secs(10), "the walk to reach the gate", || {
            self.reached.load(std::sync::atomic::Ordering::SeqCst)
        });
    }

    /// A permit stored before the walk reaches the gate is still honored, so the
    /// test can't lose by releasing early.
    fn release(&self) {
        self.released.notify_one();
    }
}

/// The future every `Volume` method hands back.
type Fut<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;

impl Volume for GatedDrive {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &std::path::Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a std::path::Path,
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Fut<'a, Result<Vec<FileEntry>, cmdr_fs::volume::VolumeError>> {
        Box::pin(async move {
            if path == self.gate {
                self.reached.store(true, std::sync::atomic::Ordering::SeqCst);
                self.released.notified().await;
            }
            self.inner.list_directory(path, on_progress).await
        })
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a std::path::Path,
    ) -> Fut<'a, Result<FileEntry, cmdr_fs::volume::VolumeError>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a std::path::Path) -> Fut<'a, bool> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(&'a self, path: &'a std::path::Path) -> Fut<'a, Result<bool, cmdr_fs::volume::VolumeError>> {
        self.inner.is_directory(path)
    }
}

/// What one live search over `scope` came back with.
struct Answer {
    paths: Vec<String>,
    match_count: u32,
    walk: WalkEnding,
    kind: CoverageKind,
    phases: Vec<SearchPhase>,
    permission_denied: Vec<String>,
    declined: Vec<String>,
    unresolved: Vec<String>,
}

/// The one query every search below runs, scoped to `scope`.
fn query_for(scope: &str) -> SearchQuery {
    pattern_query_for(scope, "txt")
}

/// The same query under a caller's own pattern, for the searches that have to
/// match something other than the drive's files.
fn pattern_query_for(scope: &str, pattern: &str) -> SearchQuery {
    SearchQuery {
        name_pattern: Some(pattern.to_string()),
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
        sort_by: None,
    }
}

/// Run one live search over `scope`, to completion, and report what the frontend
/// would have seen.
fn search(run_id: &str, scope: &str) -> Answer {
    search_for(run_id, scope, "txt")
}

/// Run one live search over `scope` for `pattern`, to completion.
fn search_for(run_id: &str, scope: &str, pattern: &str) -> Answer {
    search_watched(run_id, scope, pattern, &CollectorSearchEventSink::default())
}

/// The same, into a sink the caller keeps — so a test can watch the run's events
/// arrive rather than only reading them once it's over.
fn search_watched(run_id: &str, scope: &str, pattern: &str, sink: &CollectorSearchEventSink) -> Answer {
    let query = pattern_query_for(scope, pattern);
    let target = Target {
        volume_id: VOLUME_ID.to_string(),
        include_paths: vec![scope.to_string()],
        from_scope: true,
    };
    let run = live::register(run_id, VOLUME_ID, RunOrigin::Dialog);
    run_live_blocking(query, target, &run, sink);
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
        kind: terminal.coverage.kind,
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

// ── How many arenas one coverage answer costs (Decision 12) ──────────

/// Cover one root to the end, so its rows are in the index and the volume's token
/// has moved. The stand-in for whatever wrote behind an arena: a drive's own
/// phased first index, or another search's walk.
fn cover_to_completion(index: &Index, root: &str) {
    let walk = index
        .cover(
            VOLUME_ID,
            vec![root.to_string()],
            CoverageDimension::Listing,
            tokio_util::sync::CancellationToken::new(),
        )
        .expect("the drive is walkable");
    while walk.next_batch().is_some() {}
    assert!(!walk.finish().cancelled, "the walk ran to the end");
}

/// A volume with `a` already covered, ready for a second walk to move its token.
/// Returns the index, its guards, and the mount root.
fn drive_with_a_covered(data: &std::path::Path) -> (Index, impl Sized, String) {
    let root = format!("{MOUNT_PREFIX}/{VOLUME_ID}");
    let provider = FakeVolumeProvider::shared();
    provider.register(VOLUME_ID, drive(&root)).mark_network(&root);
    let (index, installed) = Index::builder()
        .data_dir(data)
        .volumes(Arc::clone(&provider) as Arc<_>)
        .events(NoopEventSink::shared())
        .install_for_test();
    cover_to_completion(&index, &format!("{root}/a"));
    (index, installed, root)
}

#[test]
fn a_cold_arena_is_built_once_even_though_the_index_moved_while_it_loaded() {
    // A coverage answer needs an arena holding every row it calls covered, and
    // there are two ways to have one: the same rows (equal tokens), or an arena
    // built AFTER the answer was taken. `ensure_volume` on a cold volume gives the
    // second — it reads the database once the answer is already in hand — so the
    // arena it hands back is honorable on arrival.
    //
    // The token can't see that. It's a watermark, and any write between the answer
    // and the load moves it, so a cold load whose own seconds overlapped a walk
    // read as "out of step" and was thrown away for a second, identical build. On
    // a drive being indexed for the first time the token moves several times a
    // second, so that was every first search of a session, paying twice.
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let (index, _installed, root) = drive_with_a_covered(data.path());

    // Nothing warm, and the answer is taken before anything loads it.
    volumes::forget_volume_for_test(VOLUME_ID);
    let question = coverage_of(VOLUME_ID, std::slice::from_ref(&root));

    // Then a walk writes rows and moves the token, exactly as a first index does
    // while the arena underneath it is still loading.
    cover_to_completion(&index, &format!("{root}/b"));
    volumes::mark_walked_behind(VOLUME_ID);

    let before = volumes::arenas_built_for_test();
    let load = arena_for_coverage(VOLUME_ID, &question, AfterAnotherWalk::No);
    assert_eq!(
        volumes::arenas_built_for_test() - before,
        1,
        "one arena, not two: the one a cold load builds is already newer than the answer"
    );

    let VolumeLoad::Loaded(loaded) = load else {
        panic!("the volume has an index to load");
    };
    assert_eq!(
        loaded.coverage_token,
        index.coverage_token(VOLUME_ID),
        "and it is the current state of the index, so nothing the answer calls covered is missing from it"
    );

    volumes::forget_volume_for_test(VOLUME_ID);
    let _ = index.forget_volume(VOLUME_ID);
}

#[test]
fn a_warm_arena_a_walk_wrote_behind_is_rebuilt_before_it_answers() {
    // The other half of Decision 12, and the reason the check can't simply go: a
    // WARM arena predates its answer, so nothing about when it was built says it
    // holds the rows the answer calls covered. Here a walk writes behind it, and
    // it has to be rebuilt — once — before the answer may be honored against it.
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let (index, _installed, root) = drive_with_a_covered(data.path());

    volumes::forget_volume_for_test(VOLUME_ID);
    let VolumeLoad::Loaded(warm) = volumes::ensure_volume(VOLUME_ID) else {
        panic!("the volume has an index to load");
    };

    // A walk writes behind that arena, and only THEN is the answer taken.
    cover_to_completion(&index, &format!("{root}/b"));
    volumes::mark_walked_behind(VOLUME_ID);
    let question = coverage_of(VOLUME_ID, std::slice::from_ref(&root));

    let before = volumes::arenas_built_for_test();
    let load = arena_for_coverage(VOLUME_ID, &question, AfterAnotherWalk::No);
    assert_eq!(
        volumes::arenas_built_for_test() - before,
        1,
        "the stale arena is rebuilt, and once"
    );

    let VolumeLoad::Loaded(loaded) = load else {
        panic!("the volume still has an index to load");
    };
    assert_ne!(
        loaded.coverage_token, warm.coverage_token,
        "and what comes back is not the arena the walk wrote behind"
    );
    assert_eq!(
        loaded.coverage_token,
        index.coverage_token(VOLUME_ID),
        "it is the state the answer describes"
    );

    volumes::forget_volume_for_test(VOLUME_ID);
    let _ = index.forget_volume(VOLUME_ID);
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
    // The typed coverage signal MCP renders, carried through the fold: the
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
fn a_scoped_search_answers_with_its_own_folder_whether_or_not_the_drive_is_indexed() {
    // The definition of done, on the folder the user pointed at: the same result
    // set indexed or not. A scope root matches its own query as readily as
    // anything under it, and the index has always answered with it — the walk
    // writes its row (`ensure_walkable`) and used to emit every entry BUT that
    // one, so the same search answered "1 result" over an indexed drive and "no
    // files found" over an unindexed one, then "1 result" again once its own walk
    // had been through. Three answers to one question, one search apart.
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let root = format!("{MOUNT_PREFIX}/{VOLUME_ID}");
    let scope = format!("{root}/a/nested");

    let volumes = FakeVolumeProvider::shared();
    volumes.register(VOLUME_ID, drive(&root)).mark_network(&root);
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(NoopEventSink::shared())
        .install_for_test();

    // 1. Indexed. One search covers the branch, so the next one reads `nested`
    //    out of the arena like any other row.
    let covering = search("scope-root-covering", &format!("{root}/a"));
    assert_eq!(covering.walk, WalkEnding::Completed);
    let indexed = search_for("scope-root-indexed", &scope, "nested");
    assert_eq!(indexed.walk, WalkEnding::NothingToWalk, "the branch is covered");
    assert_eq!(
        indexed.paths,
        vec![scope.clone()],
        "the scope root matches its own query"
    );
    assert_eq!(indexed.match_count, 1);

    // 2. Unindexed. The drive forgets everything it learned, so the same search
    //    runs over ground nothing has ever listed.
    volumes::forget_volume_for_test(VOLUME_ID);
    index.forget_volume(VOLUME_ID).expect("the drive forgets its index");
    let unindexed = search_for("scope-root-unindexed", &scope, "nested");
    assert_eq!(unindexed.walk, WalkEnding::Completed, "and it walked to the end");
    assert_eq!(
        unindexed.paths, indexed.paths,
        "an unindexed drive answers the same question the same way"
    );
    assert_eq!(unindexed.match_count, indexed.match_count);

    // 3. And again, now that walk has covered the ground: the third answer to one
    //    question, which is what makes a difference look like a bug in whatever
    //    the user did in between.
    let converged = search_for("scope-root-converged", &scope, "nested");
    assert_eq!(converged.walk, WalkEnding::NothingToWalk);
    assert_eq!(converged.paths, indexed.paths, "and it stays the same answer");
    assert_eq!(converged.match_count, indexed.match_count);

    volumes::forget_volume_for_test(VOLUME_ID);
    let _ = index.forget_volume(VOLUME_ID);
}

#[test]
fn a_run_whose_whole_scope_another_walk_claimed_waits_for_it_rather_than_answering_empty() {
    // Two searches over the same uncovered folder: one walk takes the ground and
    // the other is told it's taken (`cover/live.rs` — two walkers over one
    // directory orphan each other's subtrees, so only one may have it). The one
    // that loses used to finish immediately, having walked nothing, and present as
    // "No files found" under a note promising the files would turn up in a moment.
    // They never turned up in that run.
    //
    // Nothing of the scope is covered here, so there is nothing to show while
    // waiting and nothing lost by waiting: the run holds until the ground is
    // free, and answers from what the other walk wrote.
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let root = format!("{MOUNT_PREFIX}/{VOLUME_ID}");
    let scope = format!("{root}/a");

    let gated = Arc::new(GatedDrive::new(&root, &scope));
    let volumes = FakeVolumeProvider::shared();
    volumes
        .register(VOLUME_ID, Arc::clone(&gated) as Arc<dyn Volume>)
        .mark_network(&root);
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(NoopEventSink::shared())
        .install_for_test();

    // Somebody else's walk takes the scope and parks inside it. Its claim is taken
    // on THIS thread before `cover` returns, so there is no race to lose.
    let held = index
        .cover(
            VOLUME_ID,
            vec![scope.clone()],
            CoverageDimension::Listing,
            tokio_util::sync::CancellationToken::new(),
        )
        .expect("the drive is walkable");
    gated.wait_until_reached();

    // The search that lost the race, on a thread of its own because it is
    // expected to wait rather than answer.
    let searched_scope = scope.clone();
    let watched = Arc::new(CollectorSearchEventSink::default());
    let sink = Arc::clone(&watched);
    let searcher = std::thread::spawn(move || search_watched("claim-race", &searched_scope, "txt", &sink));

    // Wait until it has asked about the ground more than once — which only a run
    // that is WAITING for it does. A run that answered instead reaches its
    // terminal event here and never announces coverage twice.
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the run to wait for the ground another walk is covering",
        || {
            watched
                .progress
                .lock_ignore_poison()
                .iter()
                .filter(|event| event.phase == SearchPhase::ResolvingCoverage)
                .count()
                > 1
        },
    );

    // Let the other walk finish. Its rows land in the same index, which is where
    // the waiting run reads them from.
    gated.release();
    while held.next_batch().is_some() {}
    let outcome = held.finish();
    assert!(!outcome.cancelled, "the other walk ran to the end");

    let answer = searcher.join().expect("the waiting run finishes");
    assert_eq!(
        answer.paths,
        vec![format!("{root}/a/nested/two.txt"), format!("{root}/a/one.txt")],
        "the run that waited answers with what the other walk wrote, not with nothing"
    );
    assert_eq!(answer.match_count, 2);

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
    assert_eq!(
        first.kind,
        CoverageKind::Live,
        "the scope root was itself frontier, so every row came off the walk"
    );
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
    assert_eq!(second.kind, CoverageKind::Covered, "and the index answered all of it");
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
