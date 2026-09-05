//! The cover driver over a drive nobody has ever indexed, driven through the
//! PUBLIC handle rather than the internals.
//!
//! Everything here runs the real activation: the walk stands the database, epoch,
//! writer, and read handles up for itself, and the assertions are about what a
//! caller can observe afterwards (coverage, freshness, branches, whether a scan
//! ever ran). `tests.rs` next door drives the same walk over an index that already
//! exists.

use std::path::PathBuf;
use std::sync::Arc;

use super::test_support::drain;
use super::*;
use crate::indexing::lifecycle::rescan_request::RescanOutcome;
use crate::indexing::store::IndexStore;

/// A drive with no index, as the host reports one, plus the handle to reach it
/// through.
///
/// Everything behind the handle is process-wide, so this holds the test lock for
/// its whole life and forgets the volume on the way out; a leaked registry entry
/// would follow the next test into its own drive.
/// ⚠️ Field order IS the teardown order: struct fields drop in declaration
/// order, so the seam guard has to come before the lock guard. The other way
/// round, this restores the previous data directory AFTER releasing the lock —
/// over the top of whichever test took it next, which then fails with "no index
/// data directory configured".
struct ColdDrive {
    _installed: crate::indexing::handle::TestInstallGuard,
    data: tempfile::TempDir,
    tree: tempfile::TempDir,
    index: crate::indexing::handle::Index,
    events: Arc<crate::indexing::events::RecordingSink>,
    volume_id: &'static str,
    _serialized: std::sync::MutexGuard<'static, ()>,
}

impl ColdDrive {
    /// A local drive as the host reports one: readable through the local
    /// filesystem, no smb2 session, a local mount. Its contents come off the real
    /// temp tree, because the LOCAL walker reads the disk rather than the volume.
    fn new(volume_id: &'static str) -> Self {
        Self::with_volume(volume_id, |volume| volume.with_local_fs_access())
    }

    /// The same drive with drive indexing turned OFF in settings, which is the
    /// master switch's `false`. The handle's own guard puts the process-wide
    /// atomic back when the fixture drops.
    fn with_indexing_disabled(volume_id: &'static str) -> Self {
        Self::build(volume_id, |volume| volume.with_local_fs_access(), Some(false))
    }

    /// The same, with the registered volume shaped by `describe` — a share, a
    /// phone, whatever the refusal under test needs.
    fn with_volume(
        volume_id: &'static str,
        describe: impl FnOnce(cmdr_fs::volume::InMemoryVolume) -> cmdr_fs::volume::InMemoryVolume,
    ) -> Self {
        Self::build(volume_id, describe, None)
    }

    fn build(
        volume_id: &'static str,
        describe: impl FnOnce(cmdr_fs::volume::InMemoryVolume) -> cmdr_fs::volume::InMemoryVolume,
        indexing_enabled: Option<bool>,
    ) -> Self {
        let serialized = crate::indexing::handle::test_lock();
        let data = tempfile::tempdir().expect("index data dir");
        // In the CWD rather than `/tmp`, for the reasons `Fixture` names.
        let tree = tempfile::Builder::new()
            .prefix("cmdr-cold-cover-")
            .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .expect("temp tree");

        let volumes = crate::indexing::host::volumes::FakeVolumeProvider::shared();
        volumes.register(
            volume_id,
            Arc::new(describe(
                cmdr_fs::volume::InMemoryVolume::new("Cold").with_root(tree.path()),
            )),
        );
        let events = Arc::new(crate::indexing::events::RecordingSink::new());
        let mut builder = crate::indexing::handle::Index::builder()
            .data_dir(data.path())
            .volumes(Arc::clone(&volumes) as Arc<_>)
            .events(Arc::clone(&events) as Arc<dyn crate::indexing::events::EventSink>);
        if let Some(enabled) = indexing_enabled {
            builder = builder.indexing_enabled(Some(enabled));
        }
        let (index, installed) = builder.install_for_test();

        Self {
            _installed: installed,
            data,
            tree,
            index,
            events,
            volume_id,
            _serialized: serialized,
        }
    }

    fn path(&self, relative: &str) -> String {
        self.tree.path().join(relative).to_string_lossy().to_string()
    }

    /// This drive's index database, whether or not anything has created it yet.
    fn db_path(&self) -> PathBuf {
        self.data.path().join(format!("index-{}.db", self.volume_id))
    }

    /// How many full scans this drive has announced.
    fn scans_started(&self) -> usize {
        self.events
            .kinds_for(self.volume_id)
            .iter()
            .filter(|kind| **kind == crate::indexing::events::IndexEventKind::ScanStarted)
            .count()
    }

    /// Whether the drive's index holds a row for this absolute path.
    fn is_indexed(&self, path: &str) -> bool {
        let Ok(conn) = IndexStore::open_read_connection(&self.db_path()) else {
            return false;
        };
        let Some(relative) = IndexPathSpace::mount_rooted(self.path("")).index_relative(path) else {
            return false;
        };
        crate::indexing::store::resolve_path(&conn, &relative)
            .ok()
            .flatten()
            .is_some()
    }

    /// The epoch the drive's rows are being written against. A truncating rescan
    /// bumps it, so it reads as "something blanked this index" from outside.
    fn current_epoch(&self) -> u64 {
        let conn = IndexStore::open_read_connection(&self.db_path()).expect("read connection");
        IndexStore::read_current_epoch(&conn).expect("current epoch")
    }

    /// Mark this drive's index as one whose scan completed.
    ///
    /// What makes a "Rescan now" on it a full (re)scan rather than a phased build,
    /// and so the shape the deferred-rescan mechanism answers for: a drive that IS
    /// indexed, with a search walk live over a hole in it. A drive with no
    /// completion marker is the phase machine's, and the machine composes with a
    /// live walk instead of waiting for one.
    fn mark_scan_completed(&self) {
        let conn = IndexStore::open_write_connection(&self.db_path()).expect("write conn");
        IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp scan_completed_at");
    }

    /// What the drive's own database says it walked, as stored (index-relative).
    fn persisted_branches(&self) -> Option<String> {
        let conn = IndexStore::open_read_connection(&self.db_path()).ok()?;
        IndexStore::get_meta(&conn, crate::indexing::watch::branches::COVERED_BRANCHES_KEY)
            .ok()
            .flatten()
            .filter(|stored| !stored.is_empty())
    }

    fn coverage(&self, path: &str) -> crate::indexing::read::coverage::CoverageMap {
        self.index
            .coverage(self.volume_id, path, CoverageDimension::Listing)
            .expect("the volume answers for its own coverage")
    }

    /// Walk one scope to the end, waiting for the rows to land.
    fn cover(&self, scope: &str) -> CoverOutcome {
        let walk = self
            .index
            .cover(
                self.volume_id,
                vec![scope.to_string()],
                CoverageDimension::Listing,
                CancellationToken::new(),
            )
            .expect("the drive is walkable");
        let (_, outcome) = drain(walk);
        cmdr_fs::testing::wait_until(
            std::time::Duration::from_secs(10),
            "the walked scope to read as covered",
            || {
                let covered = self.coverage(scope);
                covered.frontier.is_empty() && covered.permission_denied.is_empty() && covered.declined.is_empty()
            },
        );
        outcome
    }
}

impl Drop for ColdDrive {
    fn drop(&mut self) {
        let _ = self.index.forget_volume(self.volume_id);
    }
}

// ── The test files, by subject ───────────────────────────────────────

/// What a walk stands up on a drive with no index, and what a later enable
/// does to it.
mod activation;

/// Both indexing switches govern background work only; a search walks either
/// way.
mod switches;

/// Per-drive intent: only a user's ask writes it.
mod intent;

/// What the walk leaves watched, and every path that releases it.
mod branches;

/// Which drives can be walked at all, and by which walker.
mod walkable;

/// The rescan a live walk defers, and the walk that fires it.
mod rescans;

/// Turning a drive's indexing off and back on faster than a teardown finishes.
mod toggles;
