//! Suggested-ops tool tests, and the fixtures they share.
//!
//! Everything here runs without a Tauri app: the handlers are thin shells over
//! `apply_planned_sweep`, `shape_list`, and `shape_group`, each of which takes a connection
//! (or plain data), so the logic is exercised directly.
//!
//! - `input.rs`: what a call must survive before anything is written.
//! - `propose.rs`: the write path, against a real migrated `main.db`.
//! - `read.rs`: the two read shapers, and the store reader they lean on.
//! - `place.rs`: where a proposal may point, over fixture volume facts.
//! - `names.rs`: names on a move or copy, and every name a proposal refuses.
//! - `server.rs`: `propose_server`, its checks over fixture saved servers, and its one write.

mod input;
mod names;
mod place;
mod propose;
mod read;
mod server;

use rusqlite::Connection;
use serde_json::{Value, json};

use super::input::plan_sweep;
use super::propose::{ApplyRefusal, apply_planned_sweep};
use crate::agent::store::{MIGRATIONS, run_migrations};
use crate::agent::suggested_ops::{IndexedFile, OpSelector, SelectorIndex, SelectorRefusal};
use crate::location::resolve::{IndexSources, VolumeFact, VolumeFacts};
use crate::tools::resources::volumes::VolumeKind;

/// A fixed "now", so an age predicate lands on a number a test can name.
const NOW: i64 = 1_800_000_000;

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    run_migrations(&conn, MIGRATIONS).expect("migrate");
    conn
}

/// An index that answers with a fixed set of files and counts how often it was asked, so a
/// test can pin that a selector resolves exactly once.
struct FakeIndex {
    files: Vec<IndexedFile>,
    refusal: Option<SelectorRefusal>,
    calls: std::cell::Cell<usize>,
}

impl FakeIndex {
    fn with(files: Vec<IndexedFile>) -> Self {
        FakeIndex {
            files,
            refusal: None,
            calls: std::cell::Cell::new(0),
        }
    }

    fn refusing(refusal: SelectorRefusal) -> Self {
        FakeIndex {
            files: Vec::new(),
            refusal: Some(refusal),
            calls: std::cell::Cell::new(0),
        }
    }
}

impl SelectorIndex for FakeIndex {
    fn resolve(&self, _selector: &OpSelector) -> Result<Vec<IndexedFile>, SelectorRefusal> {
        self.calls.set(self.calls.get() + 1);
        match &self.refusal {
            Some(refusal) => Err(refusal.clone()),
            None => Ok(self.files.clone()),
        }
    }
}

fn indexed(path: &str) -> IndexedFile {
    IndexedFile {
        path: path.to_string(),
        size: Some(4_096),
        modified_at: Some(NOW - 90 * 86_400),
        inode: Some(12),
    }
}

/// A trash group over two named paths: the shortest valid call.
fn trash_call() -> Value {
    json!({
        "groups": [{
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "Two old installers",
            "paths": ["/Users/someone/Downloads/a.dmg", "/Users/someone/Downloads/b.dmg"],
        }]
    })
}

fn selector_call() -> Value {
    json!({
        "groups": [{
            "verb": "trash",
            "selector": { "root": { "volumeId": "root", "path": "~/Downloads" }, "nameGlob": "*.dmg", "olderThanDays": 30 },
            "rationale": "They're months old.",
        }]
    })
}

fn apply(
    conn: &Connection,
    index: &dyn SelectorIndex,
    call: &Value,
) -> Result<super::propose::ProposeReport, ApplyRefusal> {
    apply_over(conn, index, call, &local_facts())
}

/// [`apply`] over the volumes `facts` describes.
fn apply_over(
    conn: &Connection,
    index: &dyn SelectorIndex,
    call: &Value,
    facts: &VolumeFacts,
) -> Result<super::propose::ProposeReport, ApplyRefusal> {
    let planned = plan_sweep(call, NOW, facts).expect("the call is valid");
    apply_planned_sweep(conn, index, planned, None, NOW)
}

/// The boot disk, live: where every path the shape tests use resolves.
fn local_facts() -> VolumeFacts {
    VolumeFacts {
        volumes: vec![volume("root", VolumeKind::Local, "/", true)],
        mount_areas: vec!["/Volumes".to_string()],
    }
}

/// One volume with no index and no share identity, named after its id.
fn volume(id: &str, kind: VolumeKind, root: &str, live: bool) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: id.to_string(),
        kind,
        root: root.to_string(),
        live,
        connection_state: None,
        earlier_roots: Vec::new(),
        index: IndexSources::none(id),
        share: None,
    }
}
