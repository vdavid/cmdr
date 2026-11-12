use super::VolumeBackend;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError, VolumeRandomReader};
use rustic_core::{FileType, Id, ReadBackend, WriteBackend};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const ROOT: &str = "/backups/repo";
const PACK_A: &str = "ab00000000000000000000000000000000000000000000000000000000000001";
const PACK_B: &str = "cd00000000000000000000000000000000000000000000000000000000000002";
const SNAP: &str = "5a00000000000000000000000000000000000000000000000000000000000003";
const KEY: &str = "4b00000000000000000000000000000000000000000000000000000000000004";

fn id(hex: &str) -> Id {
    hex.parse().expect("test ids are valid hex")
}

/// restic's own scrypt cost (`N = 2^15`, `r = 8`, `p = 1`).
const DEFAULT_N: u32 = 1 << 15;

/// A key file as restic writes one: plaintext JSON naming its scrypt cost.
fn key_json(n: u32, r: u32, p: u32) -> Vec<u8> {
    format!(
        r#"{{"created":"2026-10-12T21:00:03+02:00","username":"test_user_3","hostname":"test-host","kdf":"scrypt","N":{n},"r":{r},"p":{p},"data":"AAAA","salt":"AAAA"}}"#
    )
    .into_bytes()
}

/// A parent holding a repo's on-disk layout, with arbitrary bytes for content:
/// the adapter only moves bytes, so no real repo is needed at this layer.
async fn layout_volume(volume: InMemoryVolume) -> Arc<InMemoryVolume> {
    let files: [(String, Vec<u8>); 6] = [
        (format!("{ROOT}/config"), b"config bytes".to_vec()),
        (format!("{ROOT}/keys/{KEY}"), key_json(DEFAULT_N, 8, 1)),
        (format!("{ROOT}/snapshots/{SNAP}"), b"snapshot".to_vec()),
        (format!("{ROOT}/data/ab/{PACK_A}"), (0u8..=99).collect()),
        (format!("{ROOT}/data/cd/{PACK_B}"), vec![7; 1_000]),
        // Not a restic id: a Finder droppings file the listing must skip.
        (format!("{ROOT}/snapshots/.DS_Store"), b"junk".to_vec()),
    ];
    for dir in ["keys", "snapshots", "index", "data", "data/ab", "data/cd"] {
        volume
            .create_directory(&PathBuf::from(format!("{ROOT}/{dir}")))
            .await
            .expect("mkdir");
    }
    for (path, content) in &files {
        volume
            .create_file(Path::new(path), content)
            .await
            .expect("write fixture file");
    }
    Arc::new(volume)
}

fn backend(parent: &Arc<InMemoryVolume>) -> Arc<VolumeBackend> {
    let parent: Arc<dyn Volume> = Arc::clone(parent) as Arc<dyn Volume>;
    Arc::new(VolumeBackend::new(
        parent,
        PathBuf::from(ROOT),
        tokio::runtime::Handle::current(),
    ))
}

/// Runs `f` the way production does: on a blocking thread, never an async worker.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.expect("blocking task")
}

fn sorted(mut list: Vec<(Id, u32)>) -> Vec<(Id, u32)> {
    list.sort_unstable_by_key(|(id, _)| *id);
    list
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_each_type_from_its_folder_with_sizes() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let (keys, snaps, index, packs) = blocking(move || {
        (
            be.list_with_size(FileType::Key).expect("keys"),
            be.list_with_size(FileType::Snapshot).expect("snapshots"),
            be.list_with_size(FileType::Index).expect("index"),
            be.list_with_size(FileType::Pack).expect("packs"),
        )
    })
    .await;
    assert_eq!(keys, vec![(id(KEY), key_json(DEFAULT_N, 8, 1).len() as u32)]);
    assert_eq!(snaps, vec![(id(SNAP), 8)], "`.DS_Store` isn't an id, so it's skipped");
    assert_eq!(index, vec![]);
    assert_eq!(sorted(packs), vec![(id(PACK_A), 100), (id(PACK_B), 1_000)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_config_lists_as_one_default_id() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let config = blocking(move || be.list_with_size(FileType::Config).expect("config")).await;
    assert_eq!(config, vec![(Id::default(), 12)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_config_lists_as_none() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    parent
        .delete(Path::new(&format!("{ROOT}/config")))
        .await
        .expect("rm config");
    let be = backend(&parent);
    let config = blocking(move || be.list_with_size(FileType::Config).expect("config")).await;
    assert_eq!(config, vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_a_whole_file_from_its_layout_path() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let (config, pack) = blocking(move || {
        (
            be.read_full(FileType::Config, &Id::default()).expect("config"),
            be.read_full(FileType::Pack, &id(PACK_B)).expect("pack"),
        )
    })
    .await;
    assert_eq!(&config[..], b"config bytes");
    assert_eq!(pack.len(), 1_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_partial_read_is_one_ranged_read_of_exactly_that_window() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let bytes = blocking(move || {
        be.read_partial(FileType::Pack, &id(PACK_A), false, 10, 5)
            .expect("read")
    })
    .await;
    assert_eq!(&bytes[..], &[10, 11, 12, 13, 14]);
    assert_eq!(parent.read_range_log(), vec![(10, 5)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_partial_read_falls_back_to_a_whole_file_read_without_ranged_reads() {
    let parent = layout_volume(InMemoryVolume::new("parent").with_read_range_unsupported()).await;
    let be = backend(&parent);
    let bytes = blocking(move || {
        be.read_partial(FileType::Pack, &id(PACK_A), false, 95, 5)
            .expect("read")
    })
    .await;
    assert_eq!(&bytes[..], &[95, 96, 97, 98, 99]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_short_partial_read_is_an_error() {
    // A pack truncated under us must fail loudly, never hand rustic fewer bytes
    // than it asked for.
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let result = blocking(move || be.read_partial(FileType::Pack, &id(PACK_A), false, 98, 5)).await;
    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parent_failure_carries_the_typed_volume_error() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let err = blocking(move || {
        be.read_full(FileType::Snapshot, &id(PACK_A))
            .expect_err("no such snapshot")
    })
    .await;
    assert!(matches!(super::volume_error_in(&*err), Some(VolumeError::NotFound(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn every_write_is_refused_and_counted() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let be_for_writes = Arc::clone(&be);
    let results = blocking(move || {
        let be = be_for_writes;
        [
            be.create(),
            be.write_bytes(FileType::Snapshot, &id(SNAP), false, b"new".to_vec().into()),
            be.remove(FileType::Snapshot, &id(SNAP), false),
        ]
    })
    .await;
    assert!(results.iter().all(Result::is_err));
    assert_eq!(be.write_attempts(), 3);
    // And the parent is untouched.
    let snapshot = parent
        .read_range(Path::new(&format!("{ROOT}/snapshots/{SNAP}")), 0, 100)
        .await
        .expect("still there");
    assert_eq!(snapshot, b"snapshot");
}

/// Writes `count` more key files, ids from `first` on, as `json` makes them.
async fn add_keys(parent: &InMemoryVolume, first: usize, count: usize, json: impl Fn(usize) -> Vec<u8>) -> Vec<Id> {
    let mut ids = Vec::new();
    for i in first..first + count {
        let hex = format!("{:064x}", 0xe000 + i);
        parent
            .create_file(&PathBuf::from(format!("{ROOT}/keys/{hex}")), &json(i - first))
            .await
            .expect("key file");
        ids.push(id(&hex));
    }
    ids
}

/// ❗ A key file names its own scrypt cost in plaintext, and rustic hands it to
/// scrypt with no ceiling. A crafted "repo" on a shared drive could ask for
/// terabytes of memory the moment someone types a password, so such a key is
/// never offered to rustic.
#[tokio::test(flavor = "multi_thread")]
async fn a_key_asking_scrypt_for_too_much_is_never_offered() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    add_keys(&parent, 0, 3, |i| match i {
        0 => key_json(1 << 30, 8, 1),
        1 => key_json(DEFAULT_N, 8, 1 << 20),
        _ => key_json(1 << 20, 1 << 10, 1),
    })
    .await;
    let be = backend(&parent);

    let keys = blocking(move || be.list_with_size(FileType::Key).expect("keys")).await;

    assert_eq!(keys.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![id(KEY)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_that_isnt_a_key_file_is_never_offered() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    add_keys(&parent, 0, 1, |_| b"not json".to_vec()).await;
    add_keys(&parent, 1, 1, |_| vec![b' '; 100_000]).await;
    let be = backend(&parent);

    let keys = blocking(move || be.list_with_size(FileType::Key).expect("keys")).await;
    assert_eq!(keys.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![id(KEY)]);
}

/// rustic tries every key it's offered, a second of scrypt each. A folder with
/// thousands of keys would pin a core for hours, so only a few are offered.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_bounded_number_of_keys_is_offered() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    add_keys(&parent, 0, 40, |_| key_json(DEFAULT_N, 8, 1)).await;
    let be = backend(&parent);

    let keys = blocking(move || be.list_with_size(FileType::Key).expect("keys")).await;
    assert_eq!(keys.len(), super::MAX_KEYS);
}

#[test]
fn rustics_own_default_cost_is_well_inside_the_ceiling() {
    // `scrypt::Params::recommended()`, what rustic's `init` writes.
    assert!(super::scrypt_cost_is_tolerable(1 << 17, 8, 1));
    assert!(!super::scrypt_cost_is_tolerable(u32::MAX, u32::MAX, u32::MAX));
}

// ── Held pack readers ──────────────────────────────────────────────────────

/// A parent that hands out positioned readers (as `SmbVolume` does), counting
/// opens, closes, and reads. Everything else is the in-memory volume's.
struct ReaderParent {
    inner: Arc<InMemoryVolume>,
    opens: Arc<AtomicUsize>,
    closes: Arc<AtomicUsize>,
}

impl ReaderParent {
    fn over(inner: Arc<InMemoryVolume>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            opens: Arc::new(AtomicUsize::new(0)),
            closes: Arc::new(AtomicUsize::new(0)),
        })
    }
}

struct CountingReader {
    inner: Arc<InMemoryVolume>,
    path: PathBuf,
    size: u64,
    closes: Arc<AtomicUsize>,
}

impl VolumeRandomReader for CountingReader {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(
        &self,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + '_>> {
        self.inner.read_range(&self.path, offset, len)
    }

    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send>> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

impl Volume for ReaderParent {
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
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        self.inner.read_range(path, offset, len)
    }

    fn open_random_reader<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeRandomReader>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let size = self.inner.get_metadata(path).await?.size.unwrap_or(0);
            self.opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(CountingReader {
                inner: Arc::clone(&self.inner),
                path: path.to_path_buf(),
                size,
                closes: Arc::clone(&self.closes),
            }) as Box<dyn VolumeRandomReader>)
        })
    }
}

fn backend_over(parent: &Arc<ReaderParent>) -> Arc<VolumeBackend> {
    let parent: Arc<dyn Volume> = Arc::clone(parent) as Arc<dyn Volume>;
    Arc::new(VolumeBackend::new(
        parent,
        PathBuf::from(ROOT),
        tokio::runtime::Handle::current(),
    ))
}

/// Writes `count` more packs, each 100 bytes, and returns their ids.
async fn add_packs(parent: &InMemoryVolume, count: usize) -> Vec<Id> {
    parent
        .create_directory(&PathBuf::from(format!("{ROOT}/data/ee")))
        .await
        .expect("mkdir");
    let mut ids = Vec::new();
    for i in 0..count {
        let hex = format!("ee{:062x}", i);
        parent
            .create_file(&PathBuf::from(format!("{ROOT}/data/ee/{hex}")), &[i as u8; 100])
            .await
            .expect("pack");
        ids.push(id(&hex));
    }
    ids
}

/// ❗ A pack is read at many offsets (one tree, one blob each). Over SMB a
/// `read_range` is open + read + close, ~14 ms a call, so the adapter opens a
/// pack once and reads it through the held handle from then on.
/// ❗ A held handle keeps a deleted file delete-pending on Samba and Windows, so
/// removing the repo's `data/` right after browsing would answer "folder not
/// empty". The handles close after a short idle, whatever the index does.
#[tokio::test(flavor = "multi_thread")]
async fn held_readers_close_after_a_short_idle() {
    let memory = layout_volume(InMemoryVolume::new("parent")).await;
    let parent = ReaderParent::over(Arc::clone(&memory));
    let be = Arc::new(
        VolumeBackend::new(
            Arc::clone(&parent) as Arc<dyn Volume>,
            PathBuf::from(ROOT),
            tokio::runtime::Handle::current(),
        )
        .with_reader_idle(Duration::from_millis(100)),
    );
    let reading = Arc::clone(&be);

    blocking(move || {
        reading
            .read_partial(FileType::Pack, &id(PACK_A), false, 0, 1)
            .expect("read")
    })
    .await;

    let closes = Arc::clone(&parent.closes);
    wait_until_async(Duration::from_secs(5), "the idle reader is closed", || {
        closes.load(Ordering::SeqCst) == 1
    })
    .await;
    drop(be);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pack_is_opened_once_for_many_reads() {
    let memory = layout_volume(InMemoryVolume::new("parent")).await;
    let parent = ReaderParent::over(Arc::clone(&memory));
    let be = backend_over(&parent);

    let (first, second) = blocking(move || {
        (
            be.read_partial(FileType::Pack, &id(PACK_A), false, 10, 5)
                .expect("read"),
            be.read_partial(FileType::Pack, &id(PACK_A), false, 50, 3)
                .expect("read"),
        )
    })
    .await;

    assert_eq!(&first[..], &[10, 11, 12, 13, 14]);
    assert_eq!(&second[..], &[50, 51, 52]);
    assert_eq!(parent.opens.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn held_readers_are_bounded_and_the_oldest_is_closed() {
    let memory = layout_volume(InMemoryVolume::new("parent")).await;
    let packs = add_packs(&memory, super::MAX_HELD_READERS + 1).await;
    let parent = ReaderParent::over(Arc::clone(&memory));
    let be = backend_over(&parent);

    blocking(move || {
        for pack in packs {
            be.read_partial(FileType::Pack, &pack, false, 0, 1).expect("read");
        }
    })
    .await;

    let closes = Arc::clone(&parent.closes);
    wait_until_async(Duration::from_secs(5), "the oldest reader is closed", || {
        closes.load(Ordering::SeqCst) >= 1
    })
    .await;
    assert_eq!(parent.opens.load(Ordering::SeqCst), super::MAX_HELD_READERS + 1);
}

/// The handles go when the repo does: a dropped adapter closes every reader it
/// held, so no server keeps a handle open until its session ends.
#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_adapter_closes_every_held_reader() {
    let memory = layout_volume(InMemoryVolume::new("parent")).await;
    let parent = ReaderParent::over(Arc::clone(&memory));
    let be = backend_over(&parent);

    blocking(move || {
        be.read_partial(FileType::Pack, &id(PACK_A), false, 0, 1).expect("read");
        be.read_partial(FileType::Pack, &id(PACK_B), false, 0, 1).expect("read");
        drop(be);
    })
    .await;

    let closes = Arc::clone(&parent.closes);
    wait_until_async(Duration::from_secs(5), "both readers are closed", || {
        closes.load(Ordering::SeqCst) == 2
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parent_without_held_readers_keeps_using_ranged_reads() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    blocking(move || {
        be.read_partial(FileType::Pack, &id(PACK_A), false, 10, 5)
            .expect("read");
        be.read_partial(FileType::Pack, &id(PACK_A), false, 20, 5)
            .expect("read");
    })
    .await;
    assert_eq!(parent.read_range_log(), vec![(10, 5), (20, 5)]);
}

/// ❗ Each key passes the per-key ceiling, but rustic tries them one after the
/// other: ten keys at the ceiling would be ten 1 GiB, multi-second derivations
/// per typed password. Their SUMMED cost is capped too.
#[tokio::test(flavor = "multi_thread")]
async fn the_keys_offered_stay_inside_a_summed_scrypt_cost() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    add_keys(&parent, 0, 10, |_| key_json(1 << 20, 8, 1)).await;
    let be = backend(&parent);

    let keys = blocking(move || be.list_with_size(FileType::Key).expect("keys")).await;
    let default_key = id(KEY);
    let ceiling_keys = keys.iter().filter(|(key, _)| *key != default_key).count();
    assert!(
        ceiling_keys as u64 * (1 << 23) <= super::MAX_TOTAL_SCRYPT_COST,
        // allowed-pluralize-noun: a test's failure message, read by a developer only
        "{ceiling_keys} keys at the per-key ceiling offered"
    );
    assert!(ceiling_keys >= 1, "a tolerable key is still offered");
}

/// A parent whose listings report no sizes (some backends can't), over the
/// in-memory volume.
struct SizelessParent {
    inner: Arc<InMemoryVolume>,
}

impl Volume for SizelessParent {
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
        Box::pin(async move {
            let mut entries = self.inner.list_directory(path, on_progress).await?;
            for entry in &mut entries {
                entry.size = None;
            }
            Ok(entries)
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn cmdr_fs::volume::VolumeReadStream>, VolumeError>> + Send + 'a>>
    {
        self.inner.open_read_stream(path)
    }
}

/// ❗ A listing that reports no size (`None` reads as 0) must not let a huge
/// "key" file be read whole before any password is checked: the read itself
/// stops past the key-file limit, and the key is skipped.
#[tokio::test(flavor = "multi_thread")]
async fn a_key_file_past_the_limit_is_skipped_even_when_the_listing_hides_its_size() {
    let inner = layout_volume(InMemoryVolume::new("parent")).await;
    // Valid JSON (trailing whitespace is allowed), so only its size can refuse it.
    let mut padded = key_json(DEFAULT_N, 8, 1);
    padded.extend(std::iter::repeat_n(b' ', 200_000));
    let big = add_keys(&inner, 0, 1, move |_| padded.clone()).await;
    let parent: Arc<dyn Volume> = Arc::new(SizelessParent { inner });
    let be = Arc::new(VolumeBackend::new(
        parent,
        PathBuf::from(ROOT),
        tokio::runtime::Handle::current(),
    ));

    let keys = blocking(move || be.list_with_size(FileType::Key).expect("keys")).await;
    assert!(!keys.iter().any(|(id, _)| *id == big[0]), "{keys:?}");
}
