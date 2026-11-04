//! Which engine each entry point runs, per host shape: the local-disk engine
//! (`std::fs` under the scratch dir) or the volume engine (the `Volume` the id
//! names). The facade that decides this must keep every native answer exactly
//! as it is; these cells are the referee.
//!
//! Each cell asserts an OBSERVABLE that tells the two engines apart, ❌ never
//! just "it succeeded" (which passes on both): the effect lands on disk or in
//! the in-memory volume, and the other side is checked untouched. The in-memory
//! volume is rooted AT a real (empty) scratch dir, so a misrouted local engine
//! would find nothing there or write into it, and both show.
//!
//! Shapes (the probes take one by name):
//! - `"root"`: the desktop, where `root` is the local disk: local engine.
//! - `"mem"`: a non-local id: volume engine.
//! - `"demo-root"`: a host with NO local disk (the browser demo, whose one volume
//!   is also called `root`): volume engine for every entry point, and the OS
//!   trash refused as a volume without one refuses.
//!
//! Not pinned here, each for a stated reason: the scan preview (its walk is the
//! same code either way once a volume is in hand; `start_scan_preview` picks it
//! for a host with no local disk), bulk rename (its
//! rows carry reviewed fingerprints; `rename/bulk/tests.rs` pins its local and
//! volume halves), and the archive edits' `settle_volume` choice (it picks which
//! volume to notify after an in-zip edit, not an engine).

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{InMemoryVolume, Volume};
use cmdr_operation_log::types::Initiator;

use crate::event_sinks::CollectorEventSink;
use crate::host::OperationEventSink;
use crate::testing::TestOperations;
use crate::types::{VolumeCopyConfig, WriteOperationConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ran {
    Local,
    Volume,
}

/// The id a shape's operations name.
fn id(shape: &str) -> &str {
    if shape == "demo-root" { "root" } else { shape }
}

/// One engine, a scratch dir on disk, and an in-memory volume rooted at a second,
/// EMPTY scratch dir, registered under `mem` (or, for `demo-root`, under `root` on a
/// host with no local disk).
struct Rig {
    engine: TestOperations,
    disk: tempfile::TempDir,
    mem_root: tempfile::TempDir,
    mem: Arc<InMemoryVolume>,
}

impl Rig {
    async fn new(shape: &str) -> Self {
        let engine = TestOperations::new();
        if shape == "demo-root" {
            engine.volumes().without_local_disk();
        } else {
            engine.register_local_root();
        }
        let disk = tempfile::tempdir().expect("a scratch dir");
        let mem_root = tempfile::tempdir().expect("an empty scratch dir");
        let mem = Arc::new(
            InMemoryVolume::new("Mem")
                .with_root(mem_root.path())
                .with_space_info(10_000_000, 9_000_000),
        );
        let mem_id = if shape == "demo-root" { "root" } else { "mem" };
        engine.volumes().register(mem_id, Arc::clone(&mem) as Arc<dyn Volume>);
        for (name, bytes) in [("a.txt", b"aaa".as_slice()), ("b.txt", b"bbbb".as_slice())] {
            std::fs::write(disk.path().join(name), bytes).expect("a disk file");
            mem.create_file(&mem_root.path().join(name), bytes)
                .await
                .expect("a mem file");
        }
        std::fs::create_dir(disk.path().join("out")).expect("disk out");
        mem.create_directory(&mem_root.path().join("out"))
            .await
            .expect("mem out");
        Self {
            engine,
            disk,
            mem_root,
            mem,
        }
    }

    /// The base path an id's paths live under.
    fn base(&self, volume_id: &str) -> PathBuf {
        if volume_id != "root" {
            self.mem_root.path().to_path_buf()
        } else {
            self.disk.path().to_path_buf()
        }
    }

    /// Where `rel` exists after the op: on disk under the id's base, in memory, or both.
    async fn where_is(&self, volume_id: &str, rel: &str) -> (bool, bool) {
        let path = self.base(volume_id).join(rel);
        (path.exists(), self.mem.exists(&path).await)
    }

    /// Which engine left `rel` behind (`gone = false`) or took it away (`gone = true`).
    async fn ran(&self, volume_id: &str, rel: &str, gone: bool) -> Ran {
        let (on_disk, in_memory) = self.where_is(volume_id, rel).await;
        let mem_disk_untouched = std::fs::read_dir(self.mem_root.path())
            .expect("the mem root reads")
            .next()
            .is_none();
        match (volume_id, gone) {
            // The desktop shape: the effect is on disk.
            ("root", false) if on_disk && !in_memory => Ran::Local,
            ("root", true) if !on_disk => Ran::Local,
            // A volume id: the effect is in memory, and the empty dir under it stays empty.
            (_, false) if in_memory && !on_disk && mem_disk_untouched => Ran::Volume,
            (_, true) if !in_memory && mem_disk_untouched => Ran::Volume,
            _ => panic!(
                "{volume_id}: `{rel}` on disk {on_disk}, in memory {in_memory}, mem dir untouched {mem_disk_untouched}"
            ),
        }
    }
}

fn sink() -> Arc<CollectorEventSink> {
    Arc::new(CollectorEventSink::new())
}

/// Waits for the op that reports to `events` to settle.
async fn settled(events: &CollectorEventSink) {
    cmdr_fs::testing::wait_until_async(std::time::Duration::from_secs(10), "the operation settles", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
}

async fn copy(volume_id: &str) -> Ran {
    let rig = Rig::new(volume_id).await;
    let base = rig.base(volume_id);
    let events = sink();
    rig.engine
        .start_volume_copy(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            id(volume_id).into(),
            vec![base.join("a.txt")],
            id(volume_id).into(),
            base.join("out").display().to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await
        .expect("the copy starts");
    settled(&events).await;
    rig.ran(volume_id, "out/a.txt", false).await
}

async fn moved(volume_id: &str) -> Ran {
    let rig = Rig::new(volume_id).await;
    let base = rig.base(volume_id);
    let events = sink();
    rig.engine
        .start_volume_move(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            id(volume_id).into(),
            vec![base.join("a.txt")],
            id(volume_id).into(),
            base.join("out").display().to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await
        .expect("the move starts");
    settled(&events).await;
    let landed = rig.ran(volume_id, "out/a.txt", false).await;
    assert_eq!(
        rig.ran(volume_id, "a.txt", true).await,
        landed,
        "the source left on the same side"
    );
    landed
}

async fn delete(volume_id: &str) -> Ran {
    let rig = Rig::new(volume_id).await;
    let base = rig.base(volume_id);
    let events = sink();
    rig.engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![base.join("a.txt")],
            WriteOperationConfig::default(),
            Some(id(volume_id).into()),
            Initiator::User,
            None,
        )
        .await
        .expect("the delete starts");
    settled(&events).await;
    // The other side's copy of the file is still there.
    let (on_disk, in_memory) = rig.where_is(volume_id, "a.txt").await;
    match volume_id {
        "root" => assert!(!on_disk && rig.mem.exists(&rig.mem_root.path().join("a.txt")).await),
        _ => assert!(!in_memory && rig.disk.path().join("a.txt").exists()),
    }
    rig.ran(volume_id, "a.txt", true).await
}

async fn rename(volume_id: &str) -> Ran {
    let rig = Rig::new(volume_id).await;
    let base = rig.base(volume_id);
    rig.engine
        .rename_managed(
            base.join("a.txt"),
            base.join("renamed.txt"),
            false,
            id(volume_id).into(),
            Initiator::User,
        )
        .await
        .expect("the rename lands");
    rig.ran(volume_id, "renamed.txt", false).await
}

async fn mkdir(volume_id: &str) -> Ran {
    let rig = Rig::new(volume_id).await;
    let base = rig.base(volume_id);
    rig.engine
        .create_directory_managed(
            Some(id(volume_id).into()),
            base.display().to_string(),
            "New folder".into(),
            Initiator::User,
        )
        .await
        .expect("the folder lands");
    rig.ran(volume_id, "New folder", false).await
}

async fn mkfile(volume_id: &str) -> Ran {
    let rig = Rig::new(volume_id).await;
    let base = rig.base(volume_id);
    rig.engine
        .create_file_managed(
            Some(id(volume_id).into()),
            base.display().to_string(),
            "New file.txt".into(),
            Initiator::User,
        )
        .await
        .expect("the file lands");
    rig.ran(volume_id, "New file.txt", false).await
}

/// The rename pre-flight `lstat`s on the local disk, or leaves the check to the
/// volume. A path that exists ONLY in memory tells them apart: the local check
/// can't find it.
async fn rename_preflight(volume_id: &str) -> Ran {
    let rig = Rig::new(volume_id).await;
    let only_in_memory = rig.mem_root.path().join("a.txt");
    match rig
        .engine
        .check_rename_permission_for_volume(only_in_memory, id(volume_id).into())
        .await
    {
        Ok(()) => Ran::Volume,
        Err(_) => Ran::Local,
    }
}

macro_rules! route {
    ($name:ident, $probe:ident, $id:literal, $expected:expr) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() {
            assert_eq!($probe($id).await, $expected);
        }
    };
}

route!(copy_on_root_runs_the_local_engine, copy, "root", Ran::Local);
route!(copy_on_a_volume_runs_the_volume_engine, copy, "mem", Ran::Volume);
route!(move_on_root_runs_the_local_engine, moved, "root", Ran::Local);
route!(move_on_a_volume_runs_the_volume_engine, moved, "mem", Ran::Volume);
route!(delete_on_root_runs_the_local_engine, delete, "root", Ran::Local);
route!(delete_on_a_volume_runs_the_volume_engine, delete, "mem", Ran::Volume);
route!(rename_on_root_runs_the_local_engine, rename, "root", Ran::Local);
route!(rename_on_a_volume_runs_the_volume_engine, rename, "mem", Ran::Volume);
route!(mkdir_on_root_runs_the_local_engine, mkdir, "root", Ran::Local);
route!(mkdir_on_a_volume_runs_the_volume_engine, mkdir, "mem", Ran::Volume);
route!(mkfile_on_root_runs_the_local_engine, mkfile, "root", Ran::Local);
route!(mkfile_on_a_volume_runs_the_volume_engine, mkfile, "mem", Ran::Volume);
route!(
    rename_preflight_on_root_checks_the_local_disk,
    rename_preflight,
    "root",
    Ran::Local
);
route!(
    rename_preflight_on_a_volume_leaves_it_to_the_volume,
    rename_preflight,
    "mem",
    Ran::Volume
);

route!(
    copy_with_no_local_disk_runs_the_volume_engine,
    copy,
    "demo-root",
    Ran::Volume
);
route!(
    move_with_no_local_disk_runs_the_volume_engine,
    moved,
    "demo-root",
    Ran::Volume
);
route!(
    delete_with_no_local_disk_runs_the_volume_engine,
    delete,
    "demo-root",
    Ran::Volume
);
route!(
    rename_with_no_local_disk_runs_the_volume_engine,
    rename,
    "demo-root",
    Ran::Volume
);
route!(
    mkdir_with_no_local_disk_runs_the_volume_engine,
    mkdir,
    "demo-root",
    Ran::Volume
);
route!(
    mkfile_with_no_local_disk_runs_the_volume_engine,
    mkfile,
    "demo-root",
    Ran::Volume
);
route!(
    rename_preflight_with_no_local_disk_leaves_it_to_the_volume,
    rename_preflight,
    "demo-root",
    Ran::Volume
);
route!(
    local_move_with_no_local_disk_runs_the_volume_engine,
    local_move,
    "demo-root",
    Ran::Volume
);

/// The local-disk-only move command (`move_files_start`, no volume id): on a host
/// with no local disk it moves through the volume `root` names.
async fn local_move(volume_id: &str) -> Ran {
    let rig = Rig::new(volume_id).await;
    let base = rig.base(volume_id);
    let events = sink();
    rig.engine
        .move_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![base.join("a.txt")],
            base.join("out"),
            WriteOperationConfig::default(),
            Vec::new(),
            None,
            Initiator::User,
            None,
            None,
        )
        .await
        .expect("the move starts");
    settled(&events).await;
    rig.ran(volume_id, "out/a.txt", false).await
}

route!(local_move_on_root_runs_the_local_engine, local_move, "root", Ran::Local);

/// The OS trash needs the local disk: a host with none answers the way a volume
/// without a Trash does, typed, and touches nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trash_with_no_local_disk_refuses_like_a_volume_without_one() {
    let rig = Rig::new("demo-root").await;
    let file = rig.base("demo-root").join("a.txt");
    let refused = rig
        .engine
        .trash_files_start(
            sink() as Arc<dyn OperationEventSink>,
            vec![file.clone()],
            None,
            WriteOperationConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(
        matches!(
            refused,
            Err(crate::types::WriteOperationError::TrashNotSupported { .. })
        ),
        "{refused:?}"
    );
    assert!(rig.mem.exists(&file).await, "the file stays");
}
