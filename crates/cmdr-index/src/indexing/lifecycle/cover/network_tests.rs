//! The cover driver over a volume the index can only reach through the `Volume`
//! trait: a share, a phone, and whatever backend comes next.
//!
//! The local half lives in `tests.rs` and reads a real temp tree, because the
//! guarded walker reads the disk. Nothing here touches a disk at all: the ground
//! is an `InMemoryVolume`, which is exactly the shape a future backend arrives in.

use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, Volume};

use super::*;
use crate::indexing::read::coverage::CoverageMap;

// ── Fixture ──────────────────────────────────────────────────────────

/// A platform-appropriate mount root. Read routing sends a path to a per-mount
/// index only when it sits under an external-mount prefix, and those differ per
/// OS, so a hardcoded `/Volumes/…` would route back to `root`'s index on Linux.
#[cfg(target_os = "macos")]
const MOUNT_PREFIX: &str = "/Volumes";
#[cfg(not(target_os = "macos"))]
const MOUNT_PREFIX: &str = "/media";

/// A share the index has never seen, driven through the public handle.
///
/// Everything behind the handle is process-wide, so this holds the test lock for
/// its whole life and forgets the volume on the way out. ⚠️ Field order IS
/// teardown order: the seam guard has to drop before the lock guard, or it
/// restores the previous data directory over the top of whichever test took the
/// lock next.
struct Share {
    _installed: crate::indexing::handle::TestInstallGuard,
    index: crate::indexing::handle::Index,
    volume_id: &'static str,
    root: String,
    _data: tempfile::TempDir,
    _serialized: std::sync::MutexGuard<'static, ()>,
}

impl Share {
    /// A share whose contents are what `build` puts under its mount root,
    /// registered as a network mount (which is what keeps the LOCAL guarded
    /// walker off it).
    fn new(volume_id: &'static str, build: impl FnOnce(&Tree) -> Vec<FileEntry>) -> Self {
        Self::with_volume(volume_id, |root| {
            let entries = build(&Tree(root.to_string()));
            Arc::new(InMemoryVolume::with_entries("Share", entries).with_root(root))
        })
    }

    /// The same, with the registered volume built by `describe` — a wrapper that
    /// counts scan sessions, one that stalls, whatever the test needs.
    fn with_volume(volume_id: &'static str, describe: impl FnOnce(&str) -> Arc<dyn Volume>) -> Self {
        let serialized = crate::indexing::handle::test_lock();
        let data = tempfile::tempdir().expect("index data dir");
        let root = format!("{MOUNT_PREFIX}/{volume_id}");

        let volumes = crate::indexing::host::volumes::FakeVolumeProvider::shared();
        volumes.register(volume_id, describe(&root)).mark_network(&root);

        let events = Arc::new(crate::indexing::events::RecordingSink::new());
        let (index, installed) = crate::indexing::handle::Index::builder()
            .data_dir(data.path())
            .volumes(Arc::clone(&volumes) as Arc<_>)
            .events(events as Arc<dyn crate::indexing::events::EventSink>)
            .install_for_test();

        Self {
            _installed: installed,
            index,
            volume_id,
            root,
            _data: data,
            _serialized: serialized,
        }
    }

    fn path(&self, relative: &str) -> String {
        Tree(self.root.clone()).path(relative)
    }

    fn coverage(&self, path: &str) -> CoverageMap {
        self.index
            .coverage(self.volume_id, path, CoverageDimension::Listing)
            .expect("the volume answers for its own coverage")
    }

    /// Start a walk over one scope.
    fn walk(&self, scope: &str) -> CoverWalk {
        self.index
            .cover(self.volume_id, vec![scope.to_string()], CoverageDimension::Listing)
            .expect("the share is walkable")
    }

    /// Walk one scope to the end, waiting for the rows to land.
    fn cover(&self, scope: &str) -> (Vec<CoveredEntry>, CoverOutcome) {
        let (entries, outcome) = drain(self.walk(scope));
        cmdr_fs::testing::wait_until(
            std::time::Duration::from_secs(10),
            "the walked scope to read as covered",
            || {
                let covered = self.coverage(scope);
                covered.frontier.is_empty() && covered.unreadable.is_empty()
            },
        );
        (entries, outcome)
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        let _ = self.index.forget_volume(self.volume_id);
    }
}

/// Builds absolute paths under a share's mount root, so a test names its ground
/// the way a user's scope does.
struct Tree(String);

impl Tree {
    fn path(&self, relative: &str) -> String {
        if relative.is_empty() {
            self.0.clone()
        } else {
            format!("{}/{relative}", self.0)
        }
    }

    fn dir(&self, relative: &str) -> FileEntry {
        FileEntry::new(leaf(relative), self.path(relative), true, false)
    }

    fn file(&self, relative: &str, size: u64) -> FileEntry {
        FileEntry {
            size: Some(size),
            ..FileEntry::new(leaf(relative), self.path(relative), false, false)
        }
    }
}

fn leaf(relative: &str) -> String {
    relative.rsplit('/').next().unwrap_or(relative).to_string()
}

/// Drain a walk, collecting every entry it emitted.
fn drain(walk: CoverWalk) -> (Vec<CoveredEntry>, CoverOutcome) {
    let mut entries = Vec::new();
    while let Some(batch) = walk.next_batch() {
        entries.extend(batch);
    }
    (entries, walk.finish())
}

// ── The scoped walk ──────────────────────────────────────────────────

/// A walk over a share covers the folder it was pointed at, hands every entry to
/// its consumer, and claims nothing else on the volume.
///
/// The second half is what "scoped" means, and it's the whole milestone: the only
/// walk `network_scanner` had was the whole volume, so a search of one folder on a
/// 10 TB NAS would have walked the NAS.
#[test]
fn a_walk_over_a_share_covers_the_folder_it_was_pointed_at() {
    let share = Share::new("cover-share-scoped-test", |t| {
        vec![
            t.dir("scope"),
            t.dir("elsewhere"),
            t.file("scope/one.txt", 4),
            t.dir("scope/inner"),
            t.file("scope/inner/two.txt", 2),
            t.file("elsewhere/other.txt", 9),
        ]
    });
    let scope = share.path("scope");

    let cold = share.coverage(&scope);
    assert_eq!(cold.frontier, vec![scope.clone()], "nothing is covered yet");

    let (entries, outcome) = share.cover(&scope);

    assert!(!outcome.cancelled, "the walk ran to the end");
    assert_eq!(outcome.roots_covered, 1);
    assert_eq!(outcome.entries_found, 3, "one.txt, inner/, inner/two.txt");
    assert_eq!(outcome.dirs_found, 1, "inner/ is the only directory among them");

    let mut emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    emitted.sort();
    assert_eq!(
        emitted,
        vec![
            share.path("scope/inner"),
            share.path("scope/inner/two.txt"),
            share.path("scope/one.txt")
        ],
        "every entry the walk wrote reached the consumer"
    );
    let one = entries
        .iter()
        .find(|e| e.path.ends_with("one.txt"))
        .expect("one.txt emitted");
    assert_eq!(one.logical_size, Some(4), "with the size a result row shows");

    assert_eq!(
        share.coverage(&share.path("")).frontier,
        vec![share.path("")],
        "and the rest of the share is untouched: nothing may claim coverage it didn't earn"
    );
}
