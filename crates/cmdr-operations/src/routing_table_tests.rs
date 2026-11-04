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
//! Shapes:
//! - `"root"` over the local disk (the desktop): local engine.
//! - A non-local id (`"mem"`): volume engine.
//!
//! Not pinned here, each for a stated reason: trash (`trash_files_start` takes
//! no volume id; it is the OS trash, local by construction), the scan preview
//! (its caller passes the source volume, so the HOST decides), bulk rename (its
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

/// One engine, a scratch dir on disk, and an in-memory volume rooted at a second,
/// EMPTY scratch dir, registered under `mem`.
struct Rig {
    engine: TestOperations,
    disk: tempfile::TempDir,
    mem_root: tempfile::TempDir,
    mem: Arc<InMemoryVolume>,
}

impl Rig {
    async fn new() -> Self {
        let engine = TestOperations::new();
        engine.register_local_root();
        let disk = tempfile::tempdir().expect("a scratch dir");
        let mem_root = tempfile::tempdir().expect("an empty scratch dir");
        let mem = Arc::new(
            InMemoryVolume::new("Mem")
                .with_root(mem_root.path())
                .with_space_info(10_000_000, 9_000_000),
        );
        engine.volumes().register("mem", Arc::clone(&mem) as Arc<dyn Volume>);
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
        if volume_id == "mem" {
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
            ("mem", false) if in_memory && !on_disk && mem_disk_untouched => Ran::Volume,
            ("mem", true) if !in_memory && mem_disk_untouched => Ran::Volume,
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
    let rig = Rig::new().await;
    let base = rig.base(volume_id);
    let events = sink();
    rig.engine
        .start_volume_copy(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            volume_id.into(),
            vec![base.join("a.txt")],
            volume_id.into(),
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
    let rig = Rig::new().await;
    let base = rig.base(volume_id);
    let events = sink();
    rig.engine
        .start_volume_move(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            volume_id.into(),
            vec![base.join("a.txt")],
            volume_id.into(),
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
    let rig = Rig::new().await;
    let base = rig.base(volume_id);
    let events = sink();
    rig.engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![base.join("a.txt")],
            WriteOperationConfig::default(),
            Some(volume_id.into()),
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
    let rig = Rig::new().await;
    let base = rig.base(volume_id);
    rig.engine
        .rename_managed(
            base.join("a.txt"),
            base.join("renamed.txt"),
            false,
            volume_id.into(),
            Initiator::User,
        )
        .await
        .expect("the rename lands");
    rig.ran(volume_id, "renamed.txt", false).await
}

async fn mkdir(volume_id: &str) -> Ran {
    let rig = Rig::new().await;
    let base = rig.base(volume_id);
    rig.engine
        .create_directory_managed(
            Some(volume_id.into()),
            base.display().to_string(),
            "New folder".into(),
            Initiator::User,
        )
        .await
        .expect("the folder lands");
    rig.ran(volume_id, "New folder", false).await
}

async fn mkfile(volume_id: &str) -> Ran {
    let rig = Rig::new().await;
    let base = rig.base(volume_id);
    rig.engine
        .create_file_managed(
            Some(volume_id.into()),
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
    let rig = Rig::new().await;
    let only_in_memory = rig.mem_root.path().join("a.txt");
    match rig
        .engine
        .check_rename_permission_for_volume(only_in_memory, volume_id.into())
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
