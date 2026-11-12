//! The hub's memo of which folders are repos: what a confirmation costs, what
//! it remembers, and for how long.

use super::{ResticRepos, Tuning};
use crate::path::VIRTUAL_ENTRY_NAME;
use crate::test_fixtures::FixtureRepo;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::{WedgedVolume, wait_until_async};
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const REPO: &str = "/Volumes/testshare/backups/test-restic";
const PLAIN: &str = "/Volumes/testshare/photos";

/// A parent that counts its listings (started and finished), over an in-memory
/// volume holding a repo at [`REPO`] and a plain folder at [`PLAIN`]. Each
/// listing takes `delay`: a NAS waking its disks.
struct CountingParent {
    inner: InMemoryVolume,
    delay: Duration,
    listings: AtomicUsize,
    finished: AtomicUsize,
}

impl CountingParent {
    async fn new() -> Arc<Self> {
        Self::slow(Duration::ZERO).await
    }

    async fn slow(delay: Duration) -> Arc<Self> {
        let inner = InMemoryVolume::new("testshare");
        FixtureRepo::init("pw").write_into(&inner, Path::new(REPO)).await;
        inner.create_directory(Path::new(PLAIN)).await.expect("plain folder");
        Arc::new(Self {
            inner,
            delay,
            listings: AtomicUsize::new(0),
            finished: AtomicUsize::new(0),
        })
    }

    fn listings(&self) -> usize {
        self.listings.load(Ordering::SeqCst)
    }

    fn finished(&self) -> usize {
        self.finished.load(Ordering::SeqCst)
    }

    fn as_volume(self: &Arc<Self>) -> Arc<dyn Volume> {
        Arc::clone(self) as Arc<dyn Volume>
    }
}

impl Volume for CountingParent {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.listings.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            // allowed-test-sleep: this fake parent IS a slow NAS listing
            tokio::time::sleep(self.delay).await;
            let listed = self.inner.list_directory(path, on_progress).await;
            self.finished.fetch_add(1, Ordering::SeqCst);
            listed
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
}

fn hub() -> ResticRepos {
    ResticRepos::new(VolumeHost::detached())
}

#[tokio::test]
async fn a_repo_is_confirmed_by_one_listing_and_remembered() {
    let parent = CountingParent::new().await;
    let hub = hub();

    assert_eq!(hub.serves_confirmed("nas", Path::new(REPO)), None, "never asked");
    assert_eq!(
        hub.confirm_repo("nas", &parent.as_volume(), Path::new(REPO)).await,
        Some(true)
    );
    assert_eq!(
        hub.confirm_repo("nas", &parent.as_volume(), Path::new(REPO)).await,
        Some(true)
    );

    assert_eq!(parent.listings(), 1);
    assert_eq!(hub.serves_confirmed("nas", Path::new(REPO)), Some(true));
}

/// ❗ Without a remembered "no", every path below a real folder named
/// [`VIRTUAL_ENTRY_NAME`] outside a repo would pay a listing of its parent.
#[tokio::test]
async fn a_folder_that_isnt_a_repo_costs_one_listing_across_many_calls() {
    let parent = CountingParent::new().await;
    let hub = hub();

    for _ in 0..5 {
        assert_eq!(
            hub.confirm_repo("nas", &parent.as_volume(), Path::new(PLAIN)).await,
            Some(false)
        );
    }
    assert_eq!(parent.listings(), 1);
    assert_eq!(hub.serves_confirmed("nas", Path::new(PLAIN)), Some(false));
}

#[tokio::test]
async fn a_no_goes_stale_and_is_asked_again() {
    let parent = CountingParent::new().await;
    let hub = ResticRepos::with_tuning(
        VolumeHost::detached(),
        Tuning {
            not_a_repo_for: Duration::ZERO,
            ..Tuning::default()
        },
    );

    assert_eq!(
        hub.confirm_repo("nas", &parent.as_volume(), Path::new(PLAIN)).await,
        Some(false)
    );
    assert_eq!(
        hub.serves_confirmed("nas", Path::new(PLAIN)),
        None,
        "a stale no is no answer"
    );
    assert_eq!(
        hub.confirm_repo("nas", &parent.as_volume(), Path::new(PLAIN)).await,
        Some(false)
    );
    assert_eq!(parent.listings(), 2);
}

#[tokio::test]
async fn a_missing_folder_is_not_a_repo() {
    let parent = CountingParent::new().await;
    let hub = hub();
    assert_eq!(
        hub.confirm_repo("nas", &parent.as_volume(), Path::new("/Volumes/testshare/gone"))
            .await,
        Some(false)
    );
    assert_eq!(
        hub.serves_confirmed("nas", Path::new("/Volumes/testshare/gone")),
        Some(false)
    );
}

/// The overlay already saw the repo's entries, so its row confirms the repo
/// for free: entering it costs no listing.
#[tokio::test]
async fn the_overlay_row_confirms_the_repo_without_a_listing() {
    let parent = CountingParent::new().await;
    let hub = hub();

    let row = hub.overlay_row("nas", Path::new(REPO));

    assert_eq!(row.name, VIRTUAL_ENTRY_NAME);
    assert_eq!(PathBuf::from(&row.path), Path::new(REPO).join(VIRTUAL_ENTRY_NAME));
    assert!(row.is_directory && !row.is_symlink);
    // The frontend draws its own glyph for this reserved id, never an OS folder icon.
    assert_eq!(row.icon_id, "restic:snapshots");
    assert_eq!(
        hub.confirm_repo("nas", &parent.as_volume(), Path::new(REPO)).await,
        Some(true)
    );
    assert_eq!(parent.listings(), 0);
}

#[tokio::test]
async fn the_memo_is_per_volume() {
    let hub = hub();
    hub.overlay_row("nas", Path::new(REPO));
    assert_eq!(hub.serves_confirmed("other-volume", Path::new(REPO)), None);
}

/// A wedged share must not hang every path below it: the confirmation gives up
/// at its deadline and answers UNDECIDED (`None`), which a write refuses on and a
/// read passes through; it's no "yes" for the sync seams either.
#[tokio::test]
async fn a_wedged_parent_times_out_undecided() {
    let parent: Arc<dyn Volume> = Arc::new(WedgedVolume::new("testshare"));
    let hub = ResticRepos::with_tuning(
        VolumeHost::detached(),
        Tuning {
            confirm_deadline: Duration::from_millis(50),
            ..Tuning::default()
        },
    );

    assert_eq!(hub.confirm_repo("nas", &parent, Path::new(REPO)).await, None);
    assert_eq!(hub.serves_confirmed("nas", Path::new(REPO)), None);
}

/// ❗ The confirm listing past its deadline is DETACHED, never dropped: a parent
/// on a phone (MTP) wedges when a transaction is abandoned mid-flight. And an
/// undecided answer is remembered briefly, so every path below a slow folder
/// doesn't pay the deadline again.
#[tokio::test(flavor = "multi_thread")]
async fn a_slow_confirm_detaches_its_listing_and_remembers_undecided_briefly() {
    let parent = CountingParent::slow(Duration::from_millis(300)).await;
    let hub = ResticRepos::with_tuning(
        VolumeHost::detached(),
        Tuning {
            confirm_deadline: Duration::from_millis(50),
            ..Tuning::default()
        },
    );

    assert_eq!(
        hub.confirm_repo("nas", &parent.as_volume(), Path::new(REPO)).await,
        None
    );
    assert_eq!(
        hub.confirm_repo("nas", &parent.as_volume(), Path::new(REPO)).await,
        None
    );
    assert_eq!(parent.listings(), 1, "the undecided answer is remembered");

    let finished = Arc::clone(&parent);
    wait_until_async(Duration::from_secs(5), "the detached listing runs to its end", || {
        finished.finished() == 1
    })
    .await;
}
