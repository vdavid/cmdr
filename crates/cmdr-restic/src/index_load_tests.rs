//! The index load's memory and threads: the index files stream through a
//! bounded window whatever the index's size, every one reaches rustic through
//! the feed (❌ never rustic reading them itself), and rustic decodes them on the
//! load's own pool (❌ never rayon's global one, which search and icons share).

use super::{INDEX_DECODERS, Load, LoadSpec};
use crate::repo::IndexedRepo;
use crate::test_fixtures::FixtureRepo;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::{InMemoryVolume, Volume};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use zeroize::Zeroizing;

const PASSWORD: &str = "correct horse";
const ROOT: &str = "/backups/test-restic";
/// Backups in the fixture: each changes a file, so each writes an index file.
/// Several times the files the window holds plus the ones rustic asks for.
const BACKUPS: u64 = 24;

/// A repo of [`BACKUPS`] index files on an in-memory parent, with the index
/// files' sizes.
struct IndexedFixture {
    parent: Arc<dyn Volume>,
    index_sizes: Vec<u64>,
    _source: TestDir,
}

async fn fixture() -> IndexedFixture {
    let source = TestDir::new("restic_index_load_source");
    let repo = FixtureRepo::init(PASSWORD);
    for i in 0..BACKUPS {
        std::fs::write(source.join("a.txt"), format!("version {i}")).expect("source file");
        repo.backup(&source, UNIX_EPOCH + Duration::from_secs(1_791_000_000 + i * 3_600));
    }
    let parent = Arc::new(InMemoryVolume::new("testshare"));
    repo.write_into(parent.as_ref(), Path::new(ROOT)).await;
    let index_sizes = repo
        .files()
        .into_iter()
        .filter(|(path, _)| path.starts_with("index"))
        .map(|(_, bytes)| bytes.len() as u64)
        .collect();
    IndexedFixture {
        parent,
        index_sizes,
        _source: source,
    }
}

async fn load(fixture: &IndexedFixture, window: u64) -> Arc<IndexedRepo> {
    let spec = LoadSpec {
        parent: Arc::clone(&fixture.parent),
        root: PathBuf::from(ROOT),
        password: Zeroizing::new(PASSWORD.to_string()),
        runtime: tokio::runtime::Handle::current(),
        window,
    };
    let load = Load::start(spec, |_| {});
    let waiter = load.join().expect("a fresh load");
    waiter.outcome(None).await.expect("the index loads")
}

/// ❗ The bytes held for rustic stay bounded however big the index is: the
/// window's worth read ahead, plus the file each of rustic's decoders waits on.
/// Holding the whole index (112 MB on David's NAS repo, ~300 MB on a 6 TB one)
/// on top of rustic's own copy is what this replaced.
#[tokio::test(flavor = "multi_thread")]
async fn an_index_bigger_than_the_window_streams_through_bounded_memory() {
    let fixture = fixture().await;
    let total: u64 = fixture.index_sizes.iter().sum();
    let largest = fixture.index_sizes.iter().copied().max().expect("index files");
    let window = largest;
    assert!(
        fixture.index_sizes.len() >= 3 * (INDEX_DECODERS + 1),
        "a fixture worth streaming"
    );

    let indexed = load(&fixture, window).await;

    let feed = indexed.backend.index_feed().expect("the load's feed");
    let peak = feed.peak_held_bytes();
    let bound = window + INDEX_DECODERS as u64 * largest;
    assert!(peak > 0, "the feed held the files it read");
    assert!(
        peak <= bound,
        "peak held: {peak}, past the bound of {bound} (the window, {window}, plus one file per decoder); index size: {total}"
    );
}

/// The load serves every index file to rustic itself, at every size: ❌ no
/// cutoff above which rustic reads them on rayon's global pool.
#[tokio::test(flavor = "multi_thread")]
async fn every_index_file_reaches_rustic_through_the_feed_at_any_size() {
    let fixture = fixture().await;

    let indexed = load(&fixture, 1).await;

    let feed = indexed.backend.index_feed().expect("the load's feed");
    assert_eq!(feed.files_read_directly(), 0);
    assert_eq!(feed.files_served() as usize, fixture.index_sizes.len());
}

/// rustic decodes the index files on the load's own small pool, so a load
/// never parks rayon's global pool (search and icons) for its length.
#[tokio::test(flavor = "multi_thread")]
async fn rustic_decodes_the_index_on_the_loads_own_pool() {
    let fixture = fixture().await;

    let indexed = load(&fixture, 1).await;

    let threads = indexed.backend.index_feed().expect("the load's feed").serving_threads();
    assert!(!threads.is_empty());
    assert!(
        threads
            .iter()
            .all(|name| name.starts_with(crate::repo::RUSTIC_POOL_PREFIX)),
        "every file was served to a thread of the load's pool: {threads:?}"
    );
}

/// ❗ A read the feed let start before a cancel, whose task only runs after it,
/// opens nothing: the reads in flight finish their round trip, but no new one
/// leaves for the parent once nobody wants the index. On a loaded box a
/// spawned read's task can first run long after its load was cancelled.
#[tokio::test(flavor = "multi_thread")]
async fn a_read_whose_task_runs_after_the_cancel_opens_nothing() {
    let fixture = fixture().await;
    let dir = Path::new(ROOT).join("index");
    let files = super::index_files(fixture.parent.as_ref(), &dir).await.expect("listed");
    let cancel = tokio_util::sync::CancellationToken::new();
    let (progress, _) = tokio::sync::watch::channel(None);
    let feed = Arc::new(super::IndexFeed::new(files, u64::MAX, cancel.clone(), progress));
    let start = feed.next_read().expect("a read may start");
    cancel.cancel();

    // An empty parent: any open answers `NotFound`.
    let empty: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("testshare"));
    let path = dir.join(&*start.id.to_hex());
    let read = super::read_one(empty, path, start, feed).await;

    assert!(read.is_ok(), "the read opened its file after the cancel: {read:?}");
}
