//! The drive every live-search test below runs against, and the two ways to
//! search it.
//!
//! Nothing is faked below the search: a real `Index`, a real walk over a real
//! `Volume`, real rows in a real database, and the real arena loaded back off
//! disk. What the volume is made of is the only fiction — an `InMemoryVolume`, so
//! a test needs no disk, no share, and no permissions.
//!
//! `GatedDrive` is the one piece with no counterpart in production: a listing
//! that blocks until a test lets it go, which is what holds a walk (and with it
//! its frontier claim) open long enough for a second search to meet it.

use std::future::Future;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, Volume};

use cmdr_index::testing::host::FakeVolumeProvider;
use cmdr_index::{CoverageDimension, Index, NoopEventSink};

use super::super::live_run::run_live_blocking;
use super::*;
use crate::ignore_poison::IgnorePoison;
use crate::search::live::events::CollectorSearchEventSink;
use crate::search::live::{self, AnswerEnding, CoverageKind, RunOrigin, SearchPhase, SearchRunCoverage, WalkEnding};

/// A platform-appropriate mount root: read routing only sends a path to a
/// per-mount index under an external-mount prefix, and those differ per OS.
#[cfg(target_os = "macos")]
pub(super) const MOUNT_PREFIX: &str = "/Volumes";
#[cfg(not(target_os = "macos"))]
pub(super) const MOUNT_PREFIX: &str = "/media";

pub(super) const VOLUME_ID: &str = "live-search-e2e";

/// The drive: two branches, so one can be walked while the other is still
/// unknown to the index.
pub(super) fn drive(root: &str) -> Arc<dyn Volume> {
    Arc::new(InMemoryVolume::with_entries("Live search", drive_entries(root)).with_root(root))
}

/// What's on it.
pub(super) fn drive_entries(root: &str) -> Vec<FileEntry> {
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
pub(super) struct GatedDrive {
    inner: InMemoryVolume,
    gate: std::path::PathBuf,
    reached: std::sync::atomic::AtomicBool,
    released: tokio::sync::Notify,
}

impl GatedDrive {
    pub(super) fn new(root: &str, gate: &str) -> Self {
        Self {
            inner: InMemoryVolume::with_entries("Live search", drive_entries(root)).with_root(root),
            gate: std::path::PathBuf::from(gate),
            reached: std::sync::atomic::AtomicBool::new(false),
            released: tokio::sync::Notify::new(),
        }
    }

    /// Block until the walk has parked on the gated listing.
    pub(super) fn wait_until_reached(&self) {
        cmdr_fs::testing::wait_until(std::time::Duration::from_secs(10), "the walk to reach the gate", || {
            self.reached.load(std::sync::atomic::Ordering::SeqCst)
        });
    }

    /// A permit stored before the walk reaches the gate is still honored, so the
    /// test can't lose by releasing early.
    pub(super) fn release(&self) {
        self.released.notify_one();
    }
}

/// The future every `Volume` method hands back.
pub(super) type Fut<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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
pub(super) struct Answer {
    pub(super) paths: Vec<String>,
    pub(super) match_count: u32,
    pub(super) walk: WalkEnding,
    pub(super) kind: CoverageKind,
    pub(super) phases: Vec<SearchPhase>,
    pub(super) permission_denied: Vec<String>,
    pub(super) declined: Vec<String>,
    pub(super) unresolved: Vec<String>,
}

/// The one query every search below runs, scoped to `scope`.
pub(super) fn query_for(scope: &str) -> SearchQuery {
    pattern_query_for(scope, "txt")
}

/// The same query under a caller's own pattern, for the searches that have to
/// match something other than the drive's files.
pub(super) fn pattern_query_for(scope: &str, pattern: &str) -> SearchQuery {
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
pub(super) fn search(run_id: &str, scope: &str) -> Answer {
    search_for(run_id, scope, "txt")
}

/// Run one live search over `scope` for `pattern`, to completion.
pub(super) fn search_for(run_id: &str, scope: &str, pattern: &str) -> Answer {
    search_watched(run_id, scope, pattern, &CollectorSearchEventSink::default())
}

/// The same, into a sink the caller keeps — so a test can watch the run's events
/// arrive rather than only reading them once it's over.
pub(super) fn search_watched(run_id: &str, scope: &str, pattern: &str, sink: &CollectorSearchEventSink) -> Answer {
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
pub(super) struct AgentAnswer {
    pub(super) paths: Vec<String>,
    pub(super) match_count: u32,
    pub(super) dirs_found: u64,
    pub(super) ending: AnswerEnding,
}

/// Run one agent search over `scope`, waiting up to `budget` for it.
pub(super) fn agent_search(scope: &str, budget: std::time::Duration) -> AgentAnswer {
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
pub(super) fn settled(ending: &AnswerEnding) -> &SearchRunCoverage {
    match ending {
        AnswerEnding::Settled(coverage) => coverage,
        other => panic!("the run was expected to settle, and reported {other:?}"),
    }
}

/// Cover one root to the end, so its rows are in the index and the volume's token
/// has moved. The stand-in for whatever wrote behind an arena: a drive's own
/// phased first index, or another search's walk.
pub(super) fn cover_to_completion(index: &Index, root: &str) {
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
pub(super) fn drive_with_a_covered(data: &std::path::Path) -> (Index, impl Sized, String) {
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
