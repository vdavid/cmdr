//! A prune elsewhere under a loaded index (#653): `restic forget --prune` on
//! another machine repacks the surviving snapshots' blobs into new packs and
//! deletes the old ones, while the snapshot ids stay. The index Cmdr holds still
//! covers those snapshots but names packs that are gone, so a read through it
//! reloads the index once and reads on; a pack missing from a fresh index is a
//! real `NotFound`.

use super::ResticVolume;
use crate::hub::ResticRepos;
use crate::path::VIRTUAL_ENTRY_NAME;
use crate::test_fixtures::{self, FixtureRepo};
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError, VolumeReadStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

const PASSWORD: &str = "correct horse";
const ROOT: &str = "/Volumes/testshare/backups/test-restic";

fn big_bytes() -> Vec<u8> {
    (0..300_000u32).map(|i| (i % 251) as u8).collect()
}

/// A backed-up folder on a remote-style parent, the view over it unlocked, and
/// the index loaded by a listing of the backed-up folder.
struct Pruned {
    _source: TestDir,
    fixture: FixtureRepo,
    parent: Arc<InMemoryVolume>,
    volume: ResticVolume,
    /// The backed-up folder inside the snapshot.
    folder: PathBuf,
}

impl Pruned {
    async fn new() -> Self {
        let source = TestDir::new("restic_stale_index_source");
        std::fs::create_dir_all(source.join("nested")).expect("source dir");
        std::fs::write(source.join("a.txt"), b"hello").expect("source file");
        std::fs::write(source.join("nested/b.bin"), big_bytes()).expect("source file");
        let fixture = FixtureRepo::init(PASSWORD);
        fixture.backup(&source, UNIX_EPOCH + Duration::from_secs(1_791_000_000));
        let parent = Arc::new(InMemoryVolume::new("testshare"));
        fixture.write_into(parent.as_ref(), Path::new(ROOT)).await;
        let hub = Arc::new(ResticRepos::new(VolumeHost::detached()));
        let volume = ResticVolume::new(
            hub,
            "smb-testshare",
            Arc::clone(&parent) as Arc<dyn Volume>,
            PathBuf::from(ROOT),
        );
        assert!(test_fixtures::unlock(&volume, PASSWORD).await);
        let folder = Self::find_source(&volume).await;
        Self {
            _source: source,
            fixture,
            parent,
            volume,
            folder,
        }
    }

    /// Walks down `latest` to the folder holding `a.txt` (a snapshot of a Mac
    /// starts at `/`), which loads the index.
    async fn find_source(volume: &ResticVolume) -> PathBuf {
        let mut dir = Path::new(ROOT).join(VIRTUAL_ENTRY_NAME).join("latest");
        loop {
            let entries = volume.list_directory(&dir, None).await.expect("lists");
            if entries.iter().any(|e| e.name == "a.txt") {
                return dir;
            }
            let [only] = entries.as_slice() else {
                panic!("{} should hold one folder: {entries:?}", dir.display());
            };
            dir = dir.join(&only.name);
        }
    }

    /// Another machine runs `restic prune --repack-all`.
    async fn prune_elsewhere(&self) {
        self.fixture.repack_all();
        self.fixture.sync_into(self.parent.as_ref(), Path::new(ROOT)).await;
    }

    fn index_loads(&self) -> u64 {
        self.volume
            .slot_for_tests()
            .index
            .lock()
            .expect("index lock")
            .generation
    }
}

async fn read_to_end(stream: &mut Box<dyn VolumeReadStream>) -> Result<Vec<u8>, VolumeError> {
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk?);
    }
    Ok(out)
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_prune_elsewhere_a_listing_and_a_read_reload_the_index_once() {
    let rig = Pruned::new().await;
    assert_eq!(rig.index_loads(), 1);

    rig.prune_elsewhere().await;

    let nested = rig
        .volume
        .list_directory(&rig.folder.join("nested"), None)
        .await
        .expect("a tree the stale index points into a deleted pack lists after a reload");
    assert_eq!(nested.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["b.bin"]);
    let mut stream = rig
        .volume
        .open_read_stream(&rig.folder.join("a.txt"))
        .await
        .expect("opens");
    assert_eq!(read_to_end(&mut stream).await.expect("reads"), b"hello");
    assert_eq!(rig.index_loads(), 2, "one reload serves every read after it");
}

/// A copy that opened a file before the prune reads it to the end after it.
#[tokio::test(flavor = "multi_thread")]
async fn a_stream_opened_before_a_prune_elsewhere_reads_on_through_a_reload() {
    let rig = Pruned::new().await;
    let mut stream = rig
        .volume
        .open_read_stream(&rig.folder.join("nested/b.bin"))
        .await
        .expect("opens");

    rig.prune_elsewhere().await;

    assert_eq!(read_to_end(&mut stream).await.expect("reads"), big_bytes());
    assert_eq!(rig.index_loads(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_range_read_after_a_prune_elsewhere_reloads_the_index() {
    let rig = Pruned::new().await;
    rig.prune_elsewhere().await;

    let bytes = rig
        .volume
        .read_range(&rig.folder.join("nested/b.bin"), 1_000, 10)
        .await
        .expect("reads after a reload");
    assert_eq!(bytes, big_bytes()[1_000..1_010]);
}

/// A pack the freshly loaded index still names is a damaged repo, not a stale
/// index: one reload, then the parent's typed `NotFound`, ❌ never a loop.
#[tokio::test(flavor = "multi_thread")]
async fn a_pack_missing_after_a_fresh_load_is_a_real_not_found() {
    let rig = Pruned::new().await;
    let data = Path::new(ROOT).join("data");
    for subdir in rig.parent.list_directory(&data, None).await.expect("lists") {
        for pack in rig
            .parent
            .list_directory(&data.join(&subdir.name), None)
            .await
            .expect("lists")
        {
            rig.parent
                .delete(&data.join(&subdir.name).join(&pack.name))
                .await
                .expect("deletes");
        }
    }

    // The open reads the folder's tree (gone too), or the read the file's blob.
    let read = match rig.volume.open_read_stream(&rig.folder.join("a.txt")).await {
        Ok(mut stream) => read_to_end(&mut stream).await,
        Err(err) => Err(err),
    };
    assert!(matches!(read, Err(VolumeError::NotFound(_))), "got {read:?}");
    assert_eq!(rig.index_loads(), 2, "one reload, then the miss stands");
}
