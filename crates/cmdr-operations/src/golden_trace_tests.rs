//! The golden traces' native cells: every scenario in `crate::golden` on both
//! engines, against the fixtures under `crates/cmdr-operations/tests/golden/`.
//! What a trace holds and why: `golden/mod.rs`. The volume engine's traces also
//! run on the browser demo's host (`cmdr-listing-wasm`'s `golden_test.rs`).
//!
//! These exist so a refactor that should change nothing (the clock seam, the
//! local-disk facade, the browser build) can PROVE it changed nothing: run them
//! before and after, and a diff is a regression until it's explained. Rewrite
//! the fixtures with `CMDR_BLESS_GOLDEN=1 cargo nextest run -p cmdr-operations
//! golden` only when a change is MEANT to move what an operation says.

use std::path::{Path, PathBuf};

use cmdr_operation_log::OperationLog;
use cmdr_operation_log::store::operation_log_db_path;
use serde_json::Value;

use crate::Operations;
use crate::golden::{Scenario, Sides, VOLUME_GOLDENS, VolumeWorld, World, render};
use crate::testing::TestOperations;

/// One engine journaling into a temp DB, and the scratch dir its local
/// operations use (on the volume engine, an unused one).
struct Rig {
    engine: TestOperations,
    db: PathBuf,
    dir: tempfile::TempDir,
    _db_dir: tempfile::TempDir,
}

impl Rig {
    /// The engine journals into a log OPENED on its own database, as the app's
    /// is, so an undo finds the writer it reverses through.
    fn new() -> Self {
        let db_dir = tempfile::Builder::new().prefix("golden").tempdir().expect("a db dir");
        let db = operation_log_db_path(db_dir.path());
        let log = OperationLog::detached();
        log.open(&db, &|_| None).expect("the log opens");
        let engine = TestOperations::with(|builder| builder.operation_log(log));
        engine.register_local_root();
        Self {
            engine,
            db,
            dir: tempfile::tempdir().expect("a scratch dir"),
            _db_dir: db_dir,
        }
    }

    /// A path in the scratch dir.
    fn local(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn volume_world(&self) -> VolumeWorld {
        VolumeWorld::new(Operations::clone(&self.engine), self.db.clone(), |id, volume| {
            self.engine.volumes().register(id, volume)
        })
    }
}

/// The local engine's world: the same tree on the disk, under the scratch dir.
struct LocalWorld<'a>(&'a Rig);

impl World for LocalWorld<'_> {
    fn engine(&self) -> &Operations {
        &self.0.engine
    }
    fn journal_db(&self) -> &Path {
        &self.0.db
    }
    fn scratch(&self) -> Vec<String> {
        let plain = self.0.dir.path().display().to_string();
        let canonical = std::fs::canonicalize(self.0.dir.path())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| plain.clone());
        // Longest first, so `/private/var/…` doesn't leave a `/private` behind.
        let mut spellings = vec![canonical, plain];
        spellings.sort_by_key(|s| std::cmp::Reverse(s.len()));
        spellings
    }
    async fn seed(&self, _dest_out: bool) {
        let rig = self.0;
        std::fs::write(rig.local("a.txt"), b"aaa").expect("a");
        std::fs::write(rig.local("b.txt"), b"bbbb").expect("b");
        std::fs::write(rig.local("c.txt"), b"ccccc").expect("c");
        std::fs::create_dir(rig.local("docs")).expect("docs");
        std::fs::write(rig.local("docs/note.md"), b"# note").expect("note");
        std::fs::create_dir(rig.local("out")).expect("out");
    }
    async fn occupy(&self, name: &str, bytes: &'static [u8]) {
        std::fs::write(self.0.local("out").join(name), bytes).expect("taken");
    }
    fn source(&self, name: &str) -> PathBuf {
        self.0.local(name)
    }
    fn across(&self) -> Sides {
        self.within()
    }
    fn within(&self) -> Sides {
        Sides {
            from_volume: "root".into(),
            to_volume: "root".into(),
            dest: self.0.local("out").display().to_string(),
        }
    }
    fn folder(&self) -> String {
        self.0.dir.path().display().to_string()
    }
}

/// Compares `actual` with the fixture `name`, or rewrites it under
/// `CMDR_BLESS_GOLDEN=1`.
fn assert_golden(name: &str, actual: &Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.json"));
    let rendered = render(actual);
    if std::env::var_os("CMDR_BLESS_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the fixture dir");
        std::fs::write(&path, rendered).expect("the fixture writes");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "no golden trace at {}: bless it with CMDR_BLESS_GOLDEN=1",
            path.display()
        )
    });
    assert!(
        expected == rendered,
        "the {name} trace moved (a diff is a regression until explained; bless only a MEANT change):\n--- expected\n{expected}\n--- actual\n{rendered}"
    );
}

async fn volume(scenario: Scenario) -> Value {
    let rig = Rig::new();
    scenario.run(&rig.volume_world()).await
}

async fn local(scenario: Scenario) -> Value {
    let rig = Rig::new();
    scenario.run(&LocalWorld(&rig)).await
}

macro_rules! golden {
    ($name:ident, $fixture:literal, $body:expr) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() {
            assert_golden($fixture, &$body.await);
        }
    };
}

golden!(golden_volume_copy, "volume-copy", volume(Scenario::Copy));
golden!(golden_local_copy, "local-copy", local(Scenario::Copy));
golden!(
    golden_volume_conflict_skip,
    "volume-conflict-skip",
    volume(Scenario::ConflictSkip)
);
golden!(
    golden_local_conflict_skip,
    "local-conflict-skip",
    local(Scenario::ConflictSkip)
);
golden!(golden_volume_undo_copy, "volume-undo-copy", volume(Scenario::UndoCopy));
golden!(golden_local_undo_copy, "local-undo-copy", local(Scenario::UndoCopy));
golden!(golden_volume_move, "volume-move", volume(Scenario::Move));
golden!(golden_local_move, "local-move", local(Scenario::Move));
golden!(
    golden_volume_move_across,
    "volume-move-across",
    volume(Scenario::MoveAcross)
);
golden!(golden_volume_delete, "volume-delete", volume(Scenario::Delete));
golden!(golden_local_delete, "local-delete", local(Scenario::Delete));
golden!(
    golden_volume_instant,
    "volume-rename-and-mkdir",
    volume(Scenario::RenameAndMkdir)
);
golden!(
    golden_local_instant,
    "local-rename-and-mkdir",
    local(Scenario::RenameAndMkdir)
);

/// Every volume-engine fixture is one of the cells above, so the embedded list
/// the browser host replays can't drift from what's blessed here.
#[test]
fn the_embedded_volume_goldens_are_the_blessed_fixtures() {
    for (_, name, embedded) in VOLUME_GOLDENS {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(format!("{name}.json"));
        let on_disk = std::fs::read_to_string(&path).expect("the fixture reads");
        assert_eq!(embedded, on_disk, "{name}");
    }
    let on_disk = std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden"))
        .expect("the fixture dir")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|name| name.starts_with("volume-"))
        .count();
    assert_eq!(
        on_disk,
        VOLUME_GOLDENS.len(),
        "a volume fixture missing from VOLUME_GOLDENS"
    );
}
