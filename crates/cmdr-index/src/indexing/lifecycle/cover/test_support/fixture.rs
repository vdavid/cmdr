//! A temp tree plus an index over it, for the LOCAL half of the cover walk.
//!
//! Shared by `tests.rs` and `repair_tests.rs`, which drive the same driver over
//! the same shape of disk. The `Volume`-trait fakes are the parent module.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::super::{CoverContext, FlushOnFinish};
use crate::indexing::IndexPathSpace;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::volume::IndexVolumeKind;
use crate::indexing::writer::IndexWriter;

/// A temp tree plus an index over it, with the ancestor chain down to the tree
/// root already seeded so a frontier path resolves.
pub(in crate::indexing::lifecycle::cover) struct Fixture {
    pub(in crate::indexing::lifecycle::cover) tree: tempfile::TempDir,
    _db_dir: tempfile::TempDir,
    pub(in crate::indexing::lifecycle::cover) db_path: PathBuf,
    pub(in crate::indexing::lifecycle::cover) writer: IndexWriter,
    /// A volume id of its own, because the in-flight frontier claims
    /// (`live.rs`) are keyed by one and these tests run in parallel over paths
    /// that would otherwise look like each other's ground.
    pub(in crate::indexing::lifecycle::cover) volume_id: String,
}

impl Fixture {
    pub(in crate::indexing::lifecycle::cover) fn new() -> Self {
        // In the CWD rather than `/tmp`: `/tmp` is excluded on Linux and is a
        // symlink on macOS, and both would fight the path space.
        let tree = tempfile::Builder::new()
            .prefix("cmdr-cover-test-")
            .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .expect("temp tree");
        let db_dir = tempfile::tempdir().expect("temp db dir");
        let db_path = db_dir.path().join("cover-test-index.db");
        IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");

        let fixture = Self {
            tree,
            _db_dir: db_dir,
            db_path,
            writer,
            volume_id: format!("cover-fixture-{}", next_fixture_id()),
        };
        fixture.seed_chain(fixture.tree.path());
        fixture
    }

    /// Insert the ancestor chain down to `path`, and sync the writer's id counter.
    pub(in crate::indexing::lifecycle::cover) fn seed_chain(&self, path: &Path) -> i64 {
        let conn = IndexStore::open_write_connection(&self.db_path).expect("write connection");
        let path_str = path.to_string_lossy();
        let mut parent_id = ROOT_ID;
        for component in path_str.split('/').filter(|c| !c.is_empty()) {
            parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
                Ok(Some(id)) => id,
                _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None, None)
                    .expect("insert chain component"),
            };
        }
        let next_id = IndexStore::get_next_id(&conn).expect("next id");
        self.writer.next_id().fetch_max(next_id, Ordering::Relaxed);
        parent_id
    }

    pub(in crate::indexing::lifecycle::cover) fn context(&self) -> CoverContext {
        CoverContext {
            volume_id: self.volume_id.clone(),
            writer: self.writer.clone(),
            space: IndexPathSpace::root(),
            kind: IndexVolumeKind::Local,
            flush: FlushOnFinish::default(),
        }
    }

    pub(in crate::indexing::lifecycle::cover) fn path(&self, relative: &str) -> String {
        self.tree.path().join(relative).to_string_lossy().to_string()
    }

    pub(in crate::indexing::lifecycle::cover) fn id_of(&self, path: &str) -> i64 {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        crate::indexing::store::resolve_path(&conn, path)
            .expect("resolve")
            .unwrap_or_else(|| panic!("{path} should have a row"))
    }

    pub(in crate::indexing::lifecycle::cover) fn child_ids(&self, path: &str) -> Vec<i64> {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        let Some(id) = crate::indexing::store::resolve_path(&conn, path).expect("resolve") else {
            return Vec::new();
        };
        let mut ids: Vec<i64> = IndexStore::list_children_on(id, &conn)
            .expect("list children")
            .iter()
            .map(|row| row.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    pub(in crate::indexing::lifecycle::cover) fn listed_epoch(&self, path: &str) -> u64 {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        IndexStore::get_listed_epoch_by_id(&conn, self.id_of(path))
            .expect("listed epoch")
            .expect("row")
    }
}

/// A fresh volume id per fixture, so parallel tests never look like each other's
/// in-flight walk.
fn next_fixture_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
