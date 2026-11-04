//! A listing's index sizes are enriched with no engine lock held, and written
//! back only when no newer re-read published in between.
//!
//! Each cell builds its own engine over a scripted index, so the index can be
//! held mid-enrich while the test does something to the same listing.

use crate::RowSpan;
use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::{IgnorePoison, RwLockIgnorePoison};
use cmdr_fs::volume::{InMemoryVolume, ResolvedVolume, RoutedKind, Volume};

use super::cached_listing::CachedListing;
use super::engine::Listings;
use super::host::{ListingIndex, ListingVolumes, NoEvents, Unregistered};
use super::listing_id::ListingId;
use super::pipeline::{OverlayRows, Prepared};
use super::testing::{hold_diffs, pending_changes};
use cmdr_fs::testing::{wait_until, wait_until_async};
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

const DIR: &str = "/sized";
const FOLDER: &str = "/sized/photos";
const LISTING: &str = "index-sizes-listing";
const WAIT: Duration = Duration::from_secs(5);

/// An index that answers one size for every folder, and can hold one chosen
/// call open until the test releases it.
struct ScriptedIndex {
    size: AtomicU64,
    /// Answers for the next calls, in order, ahead of `size`.
    answers: Mutex<VecDeque<u64>>,
    calls: AtomicUsize,
    /// The 1-based call to hold, or 0 for none.
    held_call: usize,
    entered: Mutex<Sender<()>>,
    release: Mutex<Receiver<()>>,
}

impl ListingIndex for ScriptedIndex {
    fn enrich(&self, _volume_id: &str, entries: &mut [FileEntry]) {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        // The size this call answers is what the index held when it was asked.
        let scripted = self.answers.lock_ignore_poison().pop_front();
        let size = scripted.unwrap_or_else(|| self.size.load(Ordering::SeqCst));
        if call == self.held_call {
            let _ = self.entered.lock_ignore_poison().send(());
            // A dropped sender (the test failed and unwound) lets it go too.
            let _ = self.release.lock_ignore_poison().recv();
        }
        // The contract: only the seven `recursive_*` fields, only on folders
        // that aren't symlinks.
        for entry in entries.iter_mut().filter(|e| e.is_directory && !e.is_symlink) {
            entry.recursive_size = Some(size);
        }
    }

    fn verify_directory(&self, _volume_id: &str, _path: &Path) {}
}

/// The test's handles on a held call: wait for it, then let it go.
struct Hold {
    entered: Receiver<()>,
    release: Sender<()>,
}

impl Hold {
    fn wait_entered(&self) {
        self.entered.recv_timeout(WAIT).expect("the held index call to start");
    }

    fn release(&self) {
        let _ = self.release.send(());
    }
}

fn scripted_index(size: u64, held_call: usize) -> (Arc<ScriptedIndex>, Hold) {
    let (entered_tx, entered_rx) = channel();
    let (release_tx, release_rx) = channel();
    let index = Arc::new(ScriptedIndex {
        size: AtomicU64::new(size),
        answers: Mutex::new(VecDeque::new()),
        calls: AtomicUsize::new(0),
        held_call,
        entered: Mutex::new(entered_tx),
        release: Mutex::new(release_rx),
    });
    (
        index,
        Hold {
            entered: entered_rx,
            release: release_tx,
        },
    )
}

/// One volume serving every id, routed the way the test says.
struct OneVolume {
    volume: Arc<dyn Volume>,
    routed: Option<RoutedKind>,
}

impl ListingVolumes for OneVolume {
    fn get(&self, _volume_id: &str) -> Option<Arc<dyn Volume>> {
        Some(Arc::clone(&self.volume))
    }

    fn resolve<'a>(
        &'a self,
        volume_id: &'a str,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ResolvedVolume> + Send + 'a>> {
        Box::pin(async move { self.resolve_local_only(volume_id, path) })
    }

    fn resolve_local_only(&self, _volume_id: &str, path: &Path) -> ResolvedVolume {
        ResolvedVolume {
            volume: Some(Arc::clone(&self.volume)),
            path: path.to_path_buf(),
            routed: self.routed,
            unconfirmed: None,
        }
    }

    fn unregistered<'a>(
        &'a self,
        _volume_id: &'a str,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Unregistered> + Send + 'a>> {
        Box::pin(async { Unregistered::Gone })
    }
}

/// An engine over a volume holding `/sized/photos`, with `index` as its drive
/// index, and a listing of `/sized` already open in it.
async fn engine_with(index: Arc<ScriptedIndex>, routed: Option<RoutedKind>) -> Listings {
    let volume = InMemoryVolume::new("sized");
    volume
        .create_directory(Path::new(DIR))
        .await
        .expect("create the listed folder");
    volume
        .create_directory(Path::new(FOLDER))
        .await
        .expect("create the subfolder");
    let engine = Listings::builder(
        Arc::new(OneVolume {
            volume: Arc::new(volume),
            routed,
        }),
        Arc::new(NoEvents),
    )
    .index(index)
    .runtime(tokio::runtime::Handle::current())
    .build();
    let folder = FileEntry::new("photos".to_string(), FOLDER.to_string(), true, false);
    let listing = CachedListing::new(
        &engine.inner,
        "sized".to_string(),
        PathBuf::from(DIR),
        Prepared::for_test(vec![folder], OverlayRows::Recounted(0)),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );
    engine
        .inner
        .cache
        .write_ignore_poison()
        .insert(LISTING.to_string(), listing);
    engine
}

fn folder_size(engine: &Listings) -> Option<u64> {
    let cache = engine.inner.cache.read_ignore_poison();
    let listing = cache.get(LISTING).expect("the listing is open");
    listing
        .entries()
        .iter()
        .find(|e| e.path == FOLDER)
        .expect("the folder row")
        .recursive_size
}

/// Reads a range of `engine`'s listing on its own thread, so a read that a
/// held lock blocks fails the test instead of hanging it.
fn read_range_returns(engine: &Listings) {
    let done = Arc::new(AtomicBool::new(false));
    let reader = {
        let engine = engine.clone();
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let _ = engine
                .inner
                .get_file_range(LISTING, RowSpan { start: 0, count: 10 }, true);
            done.store(true, Ordering::SeqCst);
        })
    };
    wait_until(WAIT, "a range read of the listing while the index is held", || {
        done.load(Ordering::SeqCst)
    });
    reader.join().expect("the reader thread ends");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_index_size_refresh_leaves_the_listing_readable_while_the_index_answers() {
    // An index storm runs this once per event per open pane, and each enrich is two
    // SQLite queries. Under the cache write lock, every pane read and IPC accessor
    // waited behind it.
    let (index, hold) = scripted_index(42, 1);
    let engine = engine_with(index, None).await;

    let refresher = {
        let engine = engine.clone();
        std::thread::spawn(move || engine.inner.refresh_listing_index_sizes(LISTING))
    };
    hold.wait_entered();
    read_range_returns(&engine);
    hold.release();
    refresher
        .join()
        .expect("the refresh thread ends")
        .expect("the refresh answers");

    assert_eq!(folder_size(&engine), Some(42));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn index_sizes_asked_before_a_reread_landed_are_dropped() {
    // The re-read that replaced the rows enriched them itself, later than the held
    // refresh asked, so the held refresh's sizes are the stale ones.
    let (index, hold) = scripted_index(1, 1);
    let engine = engine_with(Arc::clone(&index), None).await;

    let refresher = {
        let engine = engine.clone();
        std::thread::spawn(move || engine.inner.refresh_listing_index_sizes(LISTING))
    };
    hold.wait_entered();

    index.size.store(2, Ordering::SeqCst);
    let reread = {
        let engine = engine.clone();
        tokio::spawn(async move { engine.refresh(&ListingId::new(LISTING)).await })
    };
    wait_until_async(WAIT, "the re-read to land while the index-size refresh is held", || {
        reread.is_finished()
    })
    .await;

    hold.release();
    refresher
        .join()
        .expect("the refresh thread ends")
        .expect("the refresh answers");

    assert_eq!(
        folder_size(&engine),
        Some(2),
        "the held refresh read the index before the re-read landed; its sizes must not overwrite the re-read's"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reread_enriches_its_rows_with_no_engine_lock_held() {
    // Call 1 is the read pipeline's own enrich; call 2 is the one the replace step
    // runs for the rows it just stored. Holding it must not hold the listing.
    let (index, hold) = scripted_index(9, 2);
    let engine = engine_with(index, None).await;

    let reread = {
        let engine = engine.clone();
        tokio::spawn(async move { engine.refresh(&ListingId::new(LISTING)).await })
    };
    hold.wait_entered();
    read_range_returns(&engine);
    hold.release();
    reread.await.expect("the re-read task ends");

    assert_eq!(folder_size(&engine), Some(9));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reread_hands_the_pane_the_sizes_its_rows_landed_with() {
    // Call 1 is the read pipeline's enrich, before the read lands; call 2 is the
    // replace step's, after. The index learned something in between, and the app's
    // index-size worker compares against the cache, so a size written there
    // without telling the pane would never reach it.
    let (index, _hold) = scripted_index(0, 0);
    index.answers.lock_ignore_poison().extend([1, 2]);
    let engine = engine_with(Arc::clone(&index), None).await;
    hold_diffs(&engine, LISTING);

    engine.refresh(&ListingId::new(LISTING)).await;

    assert_eq!(folder_size(&engine), Some(2));
    let shown = pending_changes(&engine, LISTING)
        .into_iter()
        .rev()
        .find(|change| change.entry.path == FOLDER)
        .map(|change| change.entry.recursive_size);
    assert_eq!(shown, Some(Some(2)), "the pane must end on the size the cache holds");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_routed_reread_gets_no_index_sizes() {
    // An archive's or a git snapshot's paths aren't on disk, so no drive index holds
    // them: the read pipeline skips its enrich there, and the replace must too.
    let (index, _hold) = scripted_index(7, 0);
    let engine = engine_with(Arc::clone(&index), Some(RoutedKind::Archive)).await;

    engine.refresh(&ListingId::new(LISTING)).await;

    assert_eq!(folder_size(&engine), None);
    assert_eq!(index.calls.load(Ordering::SeqCst), 0);
}

/// Re-sorts `engine`'s listing by size, the way a pane's sort change does.
fn resort_by_size(engine: &Listings) {
    engine
        .inner
        .resort_listing(
            LISTING,
            SortColumn::Size,
            SortOrder::Descending,
            DirectorySortMode::LikeFiles,
            None,
            true,
            None,
            false,
            None,
        )
        .expect("the resort answers");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_resort_leaves_the_listing_readable_while_the_index_answers() {
    // A resort asks the index for fresh folder sizes before it sorts (sorting by
    // size needs them). Under the cache write lock, every pane read and IPC
    // accessor waited behind those two SQLite queries.
    let (index, hold) = scripted_index(42, 1);
    let engine = engine_with(index, None).await;

    let resorter = {
        let engine = engine.clone();
        std::thread::spawn(move || resort_by_size(&engine))
    };
    hold.wait_entered();
    read_range_returns(&engine);
    hold.release();
    resorter.join().expect("the resort thread ends");

    assert_eq!(folder_size(&engine), Some(42), "the resort still sorts on fresh sizes");
}
