use super::{IndexedRepo, ResticError, ResticRepo};
use crate::snapshots::snapshot_list_of;
use crate::test_fixtures::FixtureRepo;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PASSWORD: &str = "correct horse";
const ROOT: &str = "/Volumes/testshare/backups/test-restic";

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

/// A remote-style parent (no local file access) holding `fixture` at [`ROOT`].
async fn parent_with(fixture: &FixtureRepo, volume: InMemoryVolume) -> Arc<InMemoryVolume> {
    let volume = Arc::new(volume);
    fixture.write_into(volume.as_ref(), Path::new(ROOT)).await;
    volume
}

async fn open(parent: &Arc<InMemoryVolume>, password: &str) -> Result<ResticRepo, ResticError> {
    let parent: Arc<dyn Volume> = Arc::clone(parent) as Arc<dyn Volume>;
    ResticRepo::open(parent, PathBuf::from(ROOT), password, None, &VolumeHost::detached()).await
}

#[tokio::test(flavor = "multi_thread")]
async fn opens_with_the_password_and_lists_snapshots_oldest_first() {
    let fixture = FixtureRepo::init(PASSWORD);
    fixture.add_snapshot(at(1_791_000_000), "test-host");
    fixture.add_snapshot(at(1_790_000_000), "test-host");
    fixture.add_snapshot(at(1_792_000_000), "mini");
    let parent = parent_with(&fixture, InMemoryVolume::new("nas")).await;

    let repo = open(&parent, PASSWORD).await.expect("the right password opens it");
    let files = repo.refresh_snapshots(Vec::new()).await.expect("lists snapshots");
    let list = snapshot_list_of(&files);

    let times: Vec<_> = list.snapshots().iter().map(|s| s.time).collect();
    assert_eq!(times, vec![at(1_790_000_000), at(1_791_000_000), at(1_792_000_000)]);
    let latest = list.latest().expect("latest");
    let latest_file = files.iter().find(|f| *f.id.to_hex() == latest.id).expect("listed");
    assert_eq!(latest_file.hostname, "mini");
    assert!(list.snapshots().iter().all(|s| s.folder_name.ends_with(')')));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_password_is_typed_and_the_right_one_then_opens() {
    let fixture = FixtureRepo::init(PASSWORD);
    fixture.add_snapshot(at(1_791_000_000), "test-host");
    let parent = parent_with(&fixture, InMemoryVolume::new("nas")).await;

    let wrong = open(&parent, "incorrect horse").await;
    assert!(matches!(wrong, Err(ResticError::WrongPassword)), "got {wrong:?}");

    let repo = open(&parent, PASSWORD)
        .await
        .expect("the right password still opens it");
    assert_eq!(repo.refresh_snapshots(Vec::new()).await.expect("snapshots").len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_v1_repo_opens_too() {
    let fixture = FixtureRepo::init_v1(PASSWORD);
    fixture.add_snapshot(at(1_791_000_000), "old-mac");
    let parent = parent_with(&fixture, InMemoryVolume::new("nas")).await;

    let repo = open(&parent, PASSWORD).await.expect("v1 opens");
    let files = repo.refresh_snapshots(Vec::new()).await.expect("snapshots");
    assert_eq!(files[0].hostname, "old-mac");
}

#[tokio::test(flavor = "multi_thread")]
async fn opening_and_listing_never_write_to_the_repo() {
    let fixture = FixtureRepo::init(PASSWORD);
    fixture.add_snapshot(at(1_791_000_000), "test-host");
    let parent = parent_with(&fixture, InMemoryVolume::new("nas")).await;

    let _ = open(&parent, "incorrect horse").await;
    let repo = open(&parent, PASSWORD).await.expect("opens");
    repo.refresh_snapshots(Vec::new()).await.expect("snapshots");

    assert_eq!(repo.backend.write_attempts(), 0);
    // No lock file either: rustic's read path doesn't take restic locks.
    assert!(
        parent
            .list_directory(&Path::new(ROOT).join("locks"), None)
            .await
            .expect("locks/")
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_without_a_config_is_not_a_repository() {
    let fixture = FixtureRepo::init(PASSWORD);
    let parent = parent_with(&fixture, InMemoryVolume::new("nas")).await;
    parent.delete(&Path::new(ROOT).join("config")).await.expect("rm config");

    let result = open(&parent, PASSWORD).await;
    assert!(matches!(result, Err(ResticError::NotARepository)), "got {result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parent_that_cant_answer_surfaces_its_typed_error() {
    let fixture = FixtureRepo::init(PASSWORD);
    let parent = parent_with(&fixture, InMemoryVolume::new("nas")).await;
    // The config's stat fails the way a dropped session does.
    parent.set_stat_failing(&Path::new(ROOT).join("config"));

    let result = open(&parent, PASSWORD).await;
    assert!(
        matches!(result, Err(ResticError::Volume(VolumeError::IoError { .. }))),
        "got {result:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reading_a_tree_uses_ranged_reads_of_the_pack_never_the_whole_pack() {
    let source = TestDir::new("restic_backup_source");
    std::fs::create_dir_all(source.join("nested")).expect("source dir");
    std::fs::write(source.join("a.txt"), b"hello").expect("source file");
    std::fs::write(source.join("nested/b.bin"), vec![42u8; 300_000]).expect("source file");

    let fixture = FixtureRepo::init(PASSWORD);
    fixture.backup(&source, at(1_791_000_000));
    let pack_sizes: Vec<usize> = fixture
        .files()
        .iter()
        .filter(|(path, _)| path.starts_with("data"))
        .map(|(_, bytes)| bytes.len())
        .collect();
    let parent = parent_with(&fixture, InMemoryVolume::new("nas")).await;

    let repo = open(&parent, PASSWORD).await.expect("opens");
    let snapshot = repo.refresh_snapshots(Vec::new()).await.expect("snapshots").remove(0);
    assert!(parent.read_range_log().is_empty(), "opening and listing read no pack");
    let parent_dyn: Arc<dyn Volume> = Arc::clone(&parent) as Arc<dyn Volume>;
    let backend = Arc::new(crate::backend::VolumeBackend::new(
        parent_dyn,
        PathBuf::from(ROOT),
        tokio::runtime::Handle::current(),
    ));
    let indexed = tokio::task::spawn_blocking(move || {
        let open = super::open_blocking(&backend, PASSWORD, None)?;
        IndexedRepo::index(open, backend)
    })
    .await
    .expect("blocking task")
    .expect("index loads");
    let indexed = Arc::new(indexed);
    let reader = Arc::clone(&indexed);
    tokio::task::spawn_blocking(move || {
        reader.repo.get_tree(&snapshot.tree).expect("root tree reads");
    })
    .await
    .expect("blocking task");

    let reads = parent.read_range_log();
    assert!(!reads.is_empty(), "a tree read goes through `read_range`");
    let largest_pack = pack_sizes.into_iter().max().expect("a backup writes packs");
    assert!(
        reads.iter().all(|(_, len)| *len < largest_pack),
        "every read is a window, never a whole pack: {reads:?}"
    );
    assert_eq!(repo.backend.write_attempts() + indexed.backend.write_attempts(), 0);
}

/// ❗ rustic reads snapshot files in parallel (`stream_list`: `rayon::spawn` plus
/// `into_par_iter`) on whatever pool it runs in. A refresh runs on a small pool
/// of its own, so a slow NAS never parks rayon's global pool (search and icons)
/// for a root listing.
#[tokio::test(flavor = "multi_thread")]
async fn a_snapshot_refresh_reads_on_its_own_pool() {
    let fixture = FixtureRepo::init(PASSWORD);
    for hour in 0..6 {
        fixture.add_snapshot(at(1_791_000_000 + hour * 3_600), "test-host");
    }
    let parent = parent_with(&fixture, InMemoryVolume::new("nas")).await;
    let repo = open(&parent, PASSWORD).await.expect("opens");

    repo.refresh_snapshots(Vec::new()).await.expect("lists snapshots");

    let threads = repo.backend.read_threads(rustic_core::FileType::Snapshot);
    assert_eq!(threads.len(), 6);
    assert!(
        threads.iter().all(|name| name.starts_with(super::RUSTIC_POOL_PREFIX)),
        "every snapshot file was read on the refresh's own pool: {threads:?}"
    );
}
