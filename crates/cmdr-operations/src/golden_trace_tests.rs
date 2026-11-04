//! Golden traces: what each kind of operation SAYS and RECORDS, end to end through
//! the engine's public entry points, pinned as JSON fixtures under
//! `crates/cmdr-operations/tests/golden/` (one per op kind and engine).
//!
//! A trace is the op's events in order with their payloads (progress and scan
//! ticks left out: how many arrive is timing, not behavior), the per-source
//! events as a set (a concurrent copy finishes its sources in any order), the
//! typed answer the entry point gave, and the journal rows it left. Ids,
//! timestamps, rates, and the scratch directory are normalized away.
//!
//! These exist so a refactor that should change nothing (the clock seam, the
//! local-disk facade, the browser build) can PROVE it changed nothing: run them
//! before and after, and a diff is a regression until it's explained. Rewrite
//! the fixtures with `CMDR_BLESS_GOLDEN=1 cargo nextest run -p cmdr-operations
//! golden` only when a change is MEANT to move what an operation says.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{InMemoryVolume, Volume};
use cmdr_operation_log::store::{open_read_connection, read_operation, read_operation_items, recent_operations};
use cmdr_operation_log::types::Initiator;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::conflict_slot::ArmedConflict;
use crate::host::OperationEventSink;
use crate::testing::TestOperations;
use crate::types::{
    ConflictInfo, ConflictResolution, DryRunResult, ScanProgressEvent, VolumeCopyConfig, WriteCancelledEvent,
    WriteCompleteEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteLockedSourcesEvent, WriteOperationConfig,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent, WriteSourcesGoneEvent,
};
use cmdr_operation_log::OperationLog;
use cmdr_operation_log::store::operation_log_db_path;

// ── The recording sink ───────────────────────────────────────────────

/// Records every event an operation emits, by its wire name, and wakes a waiter
/// once the operation settles.
#[derive(Default)]
struct TraceSink {
    ordered: Mutex<Vec<Value>>,
    items: Mutex<Vec<Value>>,
    settled: Notify,
}

impl TraceSink {
    fn record(&self, name: &str, payload: &impl Serialize) {
        let payload = serde_json::to_value(payload).expect("event payloads serialize");
        self.ordered
            .lock_ignore_poison()
            .push(json!({ "event": name, "payload": payload }));
    }

    async fn wait_settled(&self) {
        tokio::time::timeout(Duration::from_secs(20), self.settled.notified())
            .await
            .expect("the operation settles within 20 s");
    }
}

impl OperationEventSink for TraceSink {
    fn emit_progress(&self, _event: WriteProgressEvent) {}
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.record("write-complete", &event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.record("write-cancelled", &event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.record("write-error", &event);
    }
    fn emit_conflict(&self, event: ArmedConflict) {
        self.record("write-conflict", &event.into_event());
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        self.record("write-conflict-resolved", &event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        let payload = serde_json::to_value(&event).expect("event payloads serialize");
        self.items.lock_ignore_poison().push(payload);
    }
    fn emit_sources_gone(&self, event: WriteSourcesGoneEvent) {
        self.record("write-sources-gone", &event);
    }
    fn emit_locked_sources(&self, event: WriteLockedSourcesEvent) {
        self.record("write-locked-sources", &event);
    }
    fn emit_scan_progress(&self, _event: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.record("scan-conflict", &conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.record("dry-run-complete", &result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.record("write-settled", &event);
        self.settled.notify_one();
    }
}

// ── Normalizing ──────────────────────────────────────────────────────

/// Keys whose value is a moment, a duration, a rate, or an id: timing and
/// identity, never behavior.
fn is_volatile_key(key: &str) -> bool {
    // Ids an operation is minted with (a volume id is behavior, so it stays).
    const IDS: [&str; 7] = [
        "operationId",
        "conflictId",
        "previewId",
        "opId",
        "itemId",
        "rollsBackOpId",
        "inverseOpId",
    ];
    if IDS.contains(&key) || key == "mtime" {
        return true;
    }
    // camelCase suffixes for a moment or a span: `startedAt`, `durationMs`.
    if ["At", "Ms", "Secs", "Seconds"]
        .iter()
        .any(|suffix| key.ends_with(suffix))
    {
        return true;
    }
    let lower = key.to_ascii_lowercase();
    ["duration", "elapsed", "persecond", "throughput", "eta"]
        .iter()
        .any(|word| lower == *word || lower.contains(word) && *word != "eta")
}

/// `value` with volatile keys blanked and `scratch` (and its canonical twin)
/// spelled `<tmp>`, so two runs compare equal.
fn normalize(value: Value, scratch: &[String]) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = if is_volatile_key(&key) && !value.is_null() {
                        Value::String("<volatile>".into())
                    } else {
                        normalize(value, scratch)
                    };
                    (key, value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(|v| normalize(v, scratch)).collect()),
        Value::String(text) => {
            let mut text = text;
            for dir in scratch {
                text = text.replace(dir.as_str(), "<tmp>");
            }
            Value::String(text)
        }
        other => other,
    }
}

// ── A run, and its trace ─────────────────────────────────────────────

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
    fn new(label: &str) -> Self {
        let db_dir = tempfile::Builder::new().prefix(label).tempdir().expect("a db dir");
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

    fn scratch_spellings(&self) -> Vec<String> {
        let plain = self.dir.path().display().to_string();
        let canonical = std::fs::canonicalize(self.dir.path())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| plain.clone());
        // Longest first, so `/private/var/…` doesn't leave a `/private` behind.
        let mut spellings = vec![canonical, plain];
        spellings.sort_by_key(|s| std::cmp::Reverse(s.len()));
        spellings
    }

    /// Every op the journal holds (newest first, as `recent_operations` reads
    /// them), each with its items, projected onto what's behavior.
    fn journal(&self) -> Value {
        let conn = open_read_connection(&self.db).expect("a read connection");
        let ops = recent_operations(&conn, 50).expect("the journal reads");
        let mut rows = Vec::new();
        for op in ops.iter().rev() {
            let row = read_operation(&conn, &op.op_id).expect("reads").expect("the op row");
            let mut items: Vec<Value> = read_operation_items(&conn, &op.op_id, 1_000)
                .expect("items read")
                .into_iter()
                .map(|item| {
                    json!({
                        "entryType": format!("{:?}", item.entry_type),
                        "rowRole": format!("{:?}", item.row_role),
                        "source": item.source_name,
                        "dest": item.dest_name,
                        "size": item.size,
                        "outcome": format!("{:?}", item.outcome),
                    })
                })
                .collect();
            items.sort_by_key(|item| item.to_string());
            rows.push(json!({
                "kind": format!("{:?}", row.kind),
                "initiator": format!("{:?}", row.initiator),
                "status": format!("{:?}", row.execution_status),
                "rollback": format!("{:?}", row.rollback_state),
                "notRollbackable": row.not_rollbackable_reason.map(|r| format!("{r:?}")),
                "reverses": row.rolls_back_op_id.is_some(),
                "sourceVolume": row.source_volume_id,
                "destVolume": row.dest_volume_id,
                "itemCount": row.item_count,
                "itemsDone": row.items_done,
                "bytesTotal": row.bytes_total,
                "items": items,
            }));
        }
        Value::Array(rows)
    }
}

/// What one scenario produced: the entry points' answers, the events, and the
/// journal, normalized.
fn trace(rig: &Rig, answers: Vec<Value>, sinks: &[&TraceSink]) -> Value {
    let scratch = rig.scratch_spellings();
    let events: Vec<Value> = sinks
        .iter()
        .map(|sink| {
            let mut items = sink.items.lock_ignore_poison().clone();
            items.sort_by_key(|item| item.to_string());
            json!({
                "ordered": sink.ordered.lock_ignore_poison().clone(),
                "sourceItems": items,
            })
        })
        .collect();
    normalize(
        json!({ "answers": answers, "events": events, "journal": rig.journal() }),
        &scratch,
    )
}

/// Compares `actual` with the fixture `name`, or rewrites it under
/// `CMDR_BLESS_GOLDEN=1`.
fn assert_golden(name: &str, actual: &Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.json"));
    let rendered = serde_json::to_string_pretty(actual).expect("renders") + "\n";
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

fn answer<T: Serialize, E: Serialize>(result: &Result<T, E>) -> Value {
    match result {
        Ok(value) => json!({ "ok": serde_json::to_value(value).expect("answers serialize") }),
        Err(error) => json!({ "err": serde_json::to_value(error).expect("errors serialize") }),
    }
}

// ── The two engines' fixtures ────────────────────────────────────────

/// The volume engine's world: two in-memory volumes, `src` and `dst`.
fn volumes(rig: &Rig) -> (Arc<InMemoryVolume>, Arc<InMemoryVolume>) {
    let src = Arc::new(InMemoryVolume::new("Src").with_space_info(100_000_000, 90_000_000));
    let dst = Arc::new(InMemoryVolume::new("Dst").with_space_info(100_000_000, 90_000_000));
    rig.engine
        .volumes()
        .register("src", Arc::clone(&src) as Arc<dyn Volume>);
    rig.engine
        .volumes()
        .register("dst", Arc::clone(&dst) as Arc<dyn Volume>);
    (src, dst)
}

/// Three files and a folder holding one, on `volume`.
async fn seed_volume(volume: &InMemoryVolume) {
    volume.create_file(Path::new("/a.txt"), b"aaa").await.expect("a");
    volume.create_file(Path::new("/b.txt"), b"bbbb").await.expect("b");
    volume.create_file(Path::new("/c.txt"), b"ccccc").await.expect("c");
    volume.create_directory(Path::new("/docs")).await.expect("docs");
    volume
        .create_file(Path::new("/docs/note.md"), b"# note")
        .await
        .expect("note");
    volume.create_directory(Path::new("/out")).await.expect("out");
}

/// The same tree on the local disk, under the scratch dir.
fn seed_local(rig: &Rig) {
    std::fs::write(rig.local("a.txt"), b"aaa").expect("a");
    std::fs::write(rig.local("b.txt"), b"bbbb").expect("b");
    std::fs::write(rig.local("c.txt"), b"ccccc").expect("c");
    std::fs::create_dir(rig.local("docs")).expect("docs");
    std::fs::write(rig.local("docs/note.md"), b"# note").expect("note");
    std::fs::create_dir(rig.local("out")).expect("out");
}

fn sources(volume_engine: bool, rig: &Rig) -> Vec<PathBuf> {
    ["a.txt", "b.txt", "c.txt", "docs"]
        .iter()
        .map(|name| {
            if volume_engine {
                Path::new("/").join(name)
            } else {
                rig.local(name)
            }
        })
        .collect()
}

/// Where a scenario's ids and paths point, per engine: the volume engine copies
/// `src` → `dst`, the local one `root` → `root` inside the scratch dir.
struct Sides {
    from_volume: &'static str,
    to_volume: &'static str,
    dest: String,
}

fn sides(volume_engine: bool, rig: &Rig) -> Sides {
    if volume_engine {
        Sides {
            from_volume: "src",
            to_volume: "dst",
            dest: "/out".into(),
        }
    } else {
        Sides {
            from_volume: "root",
            to_volume: "root",
            dest: rig.local("out").display().to_string(),
        }
    }
}

async fn copy(rig: &Rig, volume_engine: bool, config: VolumeCopyConfig) -> (Value, Arc<TraceSink>, Option<String>) {
    let s = sides(volume_engine, rig);
    let sink = Arc::new(TraceSink::default());
    let started = rig
        .engine
        .start_volume_copy(
            Arc::clone(&sink) as Arc<dyn OperationEventSink>,
            s.from_volume.into(),
            sources(volume_engine, rig),
            s.to_volume.into(),
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

// ── The scenarios ────────────────────────────────────────────────────

async fn scenario_copy(volume_engine: bool) -> Value {
    let rig = Rig::new("golden-copy");
    if volume_engine {
        let (src, dst) = volumes(&rig);
        seed_volume(&src).await;
        dst.create_directory(Path::new("/out")).await.expect("out");
    } else {
        seed_local(&rig);
    }
    let (started, sink, _) = copy(&rig, volume_engine, VolumeCopyConfig::default()).await;
    trace(&rig, vec![started], &[&sink])
}

async fn scenario_conflict_skip(volume_engine: bool) -> Value {
    let rig = Rig::new("golden-skip");
    if volume_engine {
        let (src, dst) = volumes(&rig);
        seed_volume(&src).await;
        dst.create_directory(Path::new("/out")).await.expect("out");
        dst.create_file(Path::new("/out/b.txt"), b"older").await.expect("taken");
    } else {
        seed_local(&rig);
        std::fs::write(rig.local("out/b.txt"), b"older").expect("taken");
    }
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };
    let (started, sink, _) = copy(&rig, volume_engine, config).await;
    trace(&rig, vec![started], &[&sink])
}

async fn scenario_undo_copy(volume_engine: bool) -> Value {
    let rig = Rig::new("golden-undo");
    if volume_engine {
        let (src, dst) = volumes(&rig);
        seed_volume(&src).await;
        dst.create_directory(Path::new("/out")).await.expect("out");
    } else {
        seed_local(&rig);
    }
    let (started, copy_sink, op) = copy(&rig, volume_engine, VolumeCopyConfig::default()).await;
    let op = op.expect("the copy started");
    let undo_sink = Arc::new(TraceSink::default());
    let report = rig
        .engine
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
    trace(&rig, vec![started, report], &[&copy_sink, &undo_sink])
}

async fn scenario_move(volume_engine: bool) -> Value {
    let rig = Rig::new("golden-move");
    let s = sides(volume_engine, &rig);
    // Within one volume: a rename-shaped move, the common case in the demo.
    let (volume, dest) = if volume_engine {
        let (src, _) = volumes(&rig);
        seed_volume(&src).await;
        ("src", "/out".to_string())
    } else {
        seed_local(&rig);
        (s.from_volume, s.dest.clone())
    };
    let sink = Arc::new(TraceSink::default());
    let started = rig
        .engine
        .start_volume_move(
            Arc::clone(&sink) as Arc<dyn OperationEventSink>,
            volume.into(),
            sources(volume_engine, &rig),
            volume.into(),
            dest,
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    if started.is_ok() {
        sink.wait_settled().await;
    }
    trace(&rig, vec![answer(&started)], &[&sink])
}

async fn scenario_move_across_volumes() -> Value {
    let rig = Rig::new("golden-move-across");
    let (src, dst) = volumes(&rig);
    seed_volume(&src).await;
    dst.create_directory(Path::new("/out")).await.expect("out");
    let sink = Arc::new(TraceSink::default());
    let started = rig
        .engine
        .start_volume_move(
            Arc::clone(&sink) as Arc<dyn OperationEventSink>,
            "src".into(),
            sources(true, &rig),
            "dst".into(),
            "/out".into(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    if started.is_ok() {
        sink.wait_settled().await;
    }
    trace(&rig, vec![answer(&started)], &[&sink])
}

async fn scenario_delete(volume_engine: bool) -> Value {
    let rig = Rig::new("golden-delete");
    let volume_id = if volume_engine {
        let (src, _) = volumes(&rig);
        seed_volume(&src).await;
        "src"
    } else {
        seed_local(&rig);
        "root"
    };
    let sink = Arc::new(TraceSink::default());
    let started = rig
        .engine
        .delete_files_start(
            Arc::clone(&sink) as Arc<dyn OperationEventSink>,
            sources(volume_engine, &rig),
            WriteOperationConfig::default(),
            Some(volume_id.into()),
            Initiator::User,
            None,
        )
        .await;
    if started.is_ok() {
        sink.wait_settled().await;
    }
    trace(&rig, vec![answer(&started)], &[&sink])
}

async fn scenario_rename_and_mkdir(volume_engine: bool) -> Value {
    let rig = Rig::new("golden-instant");
    let (volume_id, from, to, parent) = if volume_engine {
        let (src, _) = volumes(&rig);
        seed_volume(&src).await;
        (
            "src",
            PathBuf::from("/a.txt"),
            PathBuf::from("/renamed.txt"),
            "/".to_string(),
        )
    } else {
        seed_local(&rig);
        (
            "root",
            rig.local("a.txt"),
            rig.local("renamed.txt"),
            rig.dir.path().display().to_string(),
        )
    };
    let renamed = rig
        .engine
        .rename_managed(from, to, false, volume_id.into(), Initiator::User)
        .await;
    // A taken name refuses, typed.
    let refused = rig
        .engine
        .rename_managed(
            if volume_engine {
                PathBuf::from("/b.txt")
            } else {
                rig.local("b.txt")
            },
            if volume_engine {
                PathBuf::from("/c.txt")
            } else {
                rig.local("c.txt")
            },
            false,
            volume_id.into(),
            Initiator::User,
        )
        .await;
    let made = rig
        .engine
        .create_directory_managed(
            Some(volume_id.into()),
            parent.clone(),
            "New folder".into(),
            Initiator::User,
        )
        .await;
    let taken = rig
        .engine
        .create_directory_managed(Some(volume_id.into()), parent, "New folder".into(), Initiator::User)
        .await;
    trace(
        &rig,
        vec![answer(&renamed), answer(&refused), answer(&made), answer(&taken)],
        &[],
    )
}

// ── The cells ────────────────────────────────────────────────────────

macro_rules! golden {
    ($name:ident, $fixture:literal, $body:expr) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() {
            assert_golden($fixture, &$body.await);
        }
    };
}

golden!(golden_volume_copy, "volume-copy", scenario_copy(true));
golden!(golden_local_copy, "local-copy", scenario_copy(false));
golden!(
    golden_volume_conflict_skip,
    "volume-conflict-skip",
    scenario_conflict_skip(true)
);
golden!(
    golden_local_conflict_skip,
    "local-conflict-skip",
    scenario_conflict_skip(false)
);
golden!(golden_volume_undo_copy, "volume-undo-copy", scenario_undo_copy(true));
golden!(golden_local_undo_copy, "local-undo-copy", scenario_undo_copy(false));
golden!(golden_volume_move, "volume-move", scenario_move(true));
golden!(golden_local_move, "local-move", scenario_move(false));
golden!(
    golden_volume_move_across,
    "volume-move-across",
    scenario_move_across_volumes()
);
golden!(golden_volume_delete, "volume-delete", scenario_delete(true));
golden!(golden_local_delete, "local-delete", scenario_delete(false));
golden!(
    golden_volume_instant,
    "volume-rename-and-mkdir",
    scenario_rename_and_mkdir(true)
);
golden!(
    golden_local_instant,
    "local-rename-and-mkdir",
    scenario_rename_and_mkdir(false)
);
