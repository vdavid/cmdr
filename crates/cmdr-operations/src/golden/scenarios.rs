//! The scenarios, written once against a [`World`]: where the files live and
//! which engine runs them. [`VolumeWorld`] is the volume engine's (two in-memory
//! volumes, `src` and `dst`), shared by every host; the local engine's world is
//! the native test's own (`golden_trace_tests.rs`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::{InMemoryVolume, Volume};
use cmdr_operation_log::types::Initiator;
use serde_json::{Value, json};

use super::{TraceSink, answer, trace};
use crate::Operations;
use crate::host::OperationEventSink;
use crate::types::{ConflictResolution, VolumeCopyConfig, WriteOperationConfig};

/// Where a scenario's ids and paths point.
pub struct Sides {
    /// The volume the sources are on.
    pub from_volume: String,
    /// The volume they go to.
    pub to_volume: String,
    /// The destination folder, on `to_volume`.
    pub dest: String,
}

/// One engine and the files its scenarios act on.
pub trait World {
    /// The engine, journaling into [`Self::journal_db`].
    fn engine(&self) -> &Operations;
    /// The operation log's database.
    fn journal_db(&self) -> &Path;
    /// Spellings of a scratch dir to normalize away, longest first.
    fn scratch(&self) -> Vec<String>;
    /// Three files and a folder holding one, plus an `out` folder; with
    /// `dest_out`, also an empty `out` at a second volume's destination.
    fn seed(&self, dest_out: bool) -> impl Future<Output = ()>;
    /// Puts `bytes` at `out/<name>` on the destination.
    fn occupy(&self, name: &str, bytes: &'static [u8]) -> impl Future<Output = ()>;
    /// The seeded source `name`.
    fn source(&self, name: &str) -> PathBuf;
    /// A transfer to the destination volume's `out`.
    fn across(&self) -> Sides;
    /// A transfer into `out` on the source's own volume.
    fn within(&self) -> Sides;
    /// The folder a new folder is made in.
    fn folder(&self) -> String;
}

/// The volume engine's world: two in-memory volumes, `src` and `dst`.
pub struct VolumeWorld {
    engine: Operations,
    db: PathBuf,
    src: Arc<InMemoryVolume>,
    dst: Arc<InMemoryVolume>,
}

impl VolumeWorld {
    /// `engine` (journaling into `db`) with `src` and `dst` handed to `register`,
    /// the host's volume registry.
    pub fn new(engine: Operations, db: PathBuf, register: impl Fn(&str, Arc<dyn Volume>)) -> Self {
        let src = Arc::new(InMemoryVolume::new("Src").with_space_info(100_000_000, 90_000_000));
        let dst = Arc::new(InMemoryVolume::new("Dst").with_space_info(100_000_000, 90_000_000));
        register("src", Arc::clone(&src) as Arc<dyn Volume>);
        register("dst", Arc::clone(&dst) as Arc<dyn Volume>);
        Self { engine, db, src, dst }
    }
}

impl World for VolumeWorld {
    fn engine(&self) -> &Operations {
        &self.engine
    }
    fn journal_db(&self) -> &Path {
        &self.db
    }
    fn scratch(&self) -> Vec<String> {
        Vec::new()
    }
    async fn seed(&self, dest_out: bool) {
        let v = &self.src;
        v.create_file(Path::new("/a.txt"), b"aaa").await.expect("a");
        v.create_file(Path::new("/b.txt"), b"bbbb").await.expect("b");
        v.create_file(Path::new("/c.txt"), b"ccccc").await.expect("c");
        v.create_directory(Path::new("/docs")).await.expect("docs");
        v.create_file(Path::new("/docs/note.md"), b"# note")
            .await
            .expect("note");
        v.create_directory(Path::new("/out")).await.expect("out");
        if dest_out {
            self.dst.create_directory(Path::new("/out")).await.expect("out");
        }
    }
    async fn occupy(&self, name: &str, bytes: &'static [u8]) {
        let path = Path::new("/out").join(name);
        self.dst.create_file(&path, bytes).await.expect("taken");
    }
    fn source(&self, name: &str) -> PathBuf {
        Path::new("/").join(name)
    }
    fn across(&self) -> Sides {
        Sides {
            from_volume: "src".into(),
            to_volume: "dst".into(),
            dest: "/out".into(),
        }
    }
    fn within(&self) -> Sides {
        Sides {
            from_volume: "src".into(),
            to_volume: "src".into(),
            dest: "/out".into(),
        }
    }
    fn folder(&self) -> String {
        "/".into()
    }
}

/// One golden scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Three files and a folder copied across.
    Copy,
    /// The same, onto a taken name, answered Skip by config.
    ConflictSkip,
    /// A copy, then its undo.
    UndoCopy,
    /// A move within one volume.
    Move,
    /// A move to another volume.
    MoveAcross,
    /// A delete.
    Delete,
    /// Rename and new folder, each once fine and once onto a taken name.
    RenameAndMkdir,
}

/// The volume engine's traces: each scenario, its fixture's name, and the
/// fixture, embedded so a wasm test (no filesystem) compares against it.
pub const VOLUME_GOLDENS: [(Scenario, &str, &str); 7] = [
    (
        Scenario::Copy,
        "volume-copy",
        include_str!("../../tests/golden/volume-copy.json"),
    ),
    (
        Scenario::ConflictSkip,
        "volume-conflict-skip",
        include_str!("../../tests/golden/volume-conflict-skip.json"),
    ),
    (
        Scenario::UndoCopy,
        "volume-undo-copy",
        include_str!("../../tests/golden/volume-undo-copy.json"),
    ),
    (
        Scenario::Move,
        "volume-move",
        include_str!("../../tests/golden/volume-move.json"),
    ),
    (
        Scenario::MoveAcross,
        "volume-move-across",
        include_str!("../../tests/golden/volume-move-across.json"),
    ),
    (
        Scenario::Delete,
        "volume-delete",
        include_str!("../../tests/golden/volume-delete.json"),
    ),
    (
        Scenario::RenameAndMkdir,
        "volume-rename-and-mkdir",
        include_str!("../../tests/golden/volume-rename-and-mkdir.json"),
    ),
];

impl Scenario {
    /// Runs it in `world` and answers its normalized trace.
    pub async fn run(self, world: &impl World) -> Value {
        match self {
            Self::Copy => copy_scenario(world).await,
            Self::ConflictSkip => conflict_skip(world).await,
            Self::UndoCopy => undo_copy(world).await,
            Self::Move => move_within(world).await,
            Self::MoveAcross => move_across(world).await,
            Self::Delete => delete(world).await,
            Self::RenameAndMkdir => rename_and_mkdir(world).await,
        }
    }
}

fn sources(world: &impl World) -> Vec<PathBuf> {
    ["a.txt", "b.txt", "c.txt", "docs"]
        .iter()
        .map(|name| world.source(name))
        .collect()
}

async fn copy(world: &impl World, config: VolumeCopyConfig) -> (Value, Arc<TraceSink>, Option<String>) {
    let s = world.across();
    let sink = Arc::new(TraceSink::default());
    let started = world
        .engine()
        .start_volume_copy(
            Arc::clone(&sink) as Arc<dyn OperationEventSink>,
            s.from_volume,
            sources(world),
            s.to_volume,
            s.dest,
            config,
            Initiator::User,
            None,
        )
        .await;
    let op = started.as_ref().ok().map(|s| s.operation_id.to_string());
    if started.is_ok() {
        sink.wait_settled().await;
    }
    (answer(&started), sink, op)
}

async fn copy_scenario(world: &impl World) -> Value {
    world.seed(true).await;
    let (started, sink, _) = copy(world, VolumeCopyConfig::default()).await;
    trace(world, vec![started], &[&sink])
}

async fn conflict_skip(world: &impl World) -> Value {
    world.seed(true).await;
    world.occupy("b.txt", b"older").await;
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };
    let (started, sink, _) = copy(world, config).await;
    trace(world, vec![started], &[&sink])
}

async fn undo_copy(world: &impl World) -> Value {
    world.seed(true).await;
    let (started, copy_sink, op) = copy(world, VolumeCopyConfig::default()).await;
    let op = op.expect("the copy started");
    let undo_sink = Arc::new(TraceSink::default());
    let report = world
        .engine()
        .undo_operations(
            &[op.as_str().into()],
            Initiator::User,
            Arc::clone(&undo_sink) as Arc<dyn OperationEventSink>,
        )
        .await;
    let outcomes: Vec<Value> = report
        .operations
        .iter()
        .map(|op| {
            json!({
                "restored": op.restored,
                "skipped": op.skipped,
                "skips": op.skips.len(),
                "finalState": op.final_state.as_ref().map(|s| format!("{s:?}")),
                "refusal": op.refusal.as_ref().map(|r| format!("{r:?}")),
            })
        })
        .collect();
    let report = json!({ "restored": report.restored, "skipped": report.skipped, "operations": outcomes });
    trace(world, vec![started, report], &[&copy_sink, &undo_sink])
}

async fn start_move(world: &impl World, s: Sides) -> Value {
    let sink = Arc::new(TraceSink::default());
    let started = world
        .engine()
        .start_volume_move(
            Arc::clone(&sink) as Arc<dyn OperationEventSink>,
            s.from_volume,
            sources(world),
            s.to_volume,
            s.dest,
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    if started.is_ok() {
        sink.wait_settled().await;
    }
    trace(world, vec![answer(&started)], &[&sink])
}

/// Within one volume: a rename-shaped move, the common case in the demo.
async fn move_within(world: &impl World) -> Value {
    world.seed(false).await;
    start_move(world, world.within()).await
}

async fn move_across(world: &impl World) -> Value {
    world.seed(true).await;
    start_move(world, world.across()).await
}

async fn delete(world: &impl World) -> Value {
    world.seed(false).await;
    let sink = Arc::new(TraceSink::default());
    let started = world
        .engine()
        .delete_files_start(
            Arc::clone(&sink) as Arc<dyn OperationEventSink>,
            sources(world),
            WriteOperationConfig::default(),
            Some(world.within().from_volume),
            Initiator::User,
            None,
        )
        .await;
    if started.is_ok() {
        sink.wait_settled().await;
    }
    trace(world, vec![answer(&started)], &[&sink])
}

async fn rename_and_mkdir(world: &impl World) -> Value {
    world.seed(false).await;
    let volume_id = world.within().from_volume;
    let engine = world.engine();
    let renamed = engine
        .rename_managed(
            world.source("a.txt"),
            world.source("renamed.txt"),
            false,
            volume_id.clone(),
            Initiator::User,
        )
        .await;
    // A taken name refuses, typed.
    let refused = engine
        .rename_managed(
            world.source("b.txt"),
            world.source("c.txt"),
            false,
            volume_id.clone(),
            Initiator::User,
        )
        .await;
    let made = engine
        .create_directory_managed(
            Some(volume_id.clone()),
            world.folder(),
            "New folder".into(),
            Initiator::User,
        )
        .await;
    let taken = engine
        .create_directory_managed(Some(volume_id), world.folder(), "New folder".into(), Initiator::User)
        .await;
    trace(
        world,
        vec![answer(&renamed), answer(&refused), answer(&made), answer(&taken)],
        &[],
    )
}
