//! HOW OFTEN this backend tells the host's listing seam something.
//!
//! The rule the whole `VolumeHost` design rests on is a pace rather than a shape:
//! **a seam may be called per mutation, never per directory entry**
//! (`crates/cmdr-fs/src/volume/host/DETAILS.md`). Every seam is a `dyn` trait
//! object, which costs nothing at human cadence and is not free inside a loop over
//! a phone's whole camera roll. Nothing about that rule is visible in a type, so
//! the instrument is `RecordingListings::change_count`: a walk that reports a
//! handful of changes is right, and one that reports one per entry fails loudly.
//!
//! It matters more here than anywhere. `notify_mutation` is the ONLY producer of
//! listing changes on this backend — there's no local watcher behind it — which
//! makes the counter an exact measure of dispatch rather than a mix of two
//! sources, and makes a stray call inside a listing loop the kind of thing that
//! silently doubles the cost of every big folder a user opens on their phone.
//!
//! What the SESSION layer tells the analytics and registrar seams is the other
//! half: `connection::host_seam_test`.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::{ListingHost, RecordingListings};
use cmdr_fs::volume::{DirectoryChange, SourceItemInfo, Volume, VolumeError, VolumeReadStream};

use crate::connection::events::no_device_events;
use crate::connection::MtpConnectionManager;
use crate::testing::{ConnectedDevice, connect_virtual_device, device_lock, recording_registrar, volume_for};
use crate::volume::MtpVolume;

/// How many files the walked directory holds.
///
/// Big enough that one call per entry is unmistakable next to the handful a
/// correct walk makes, small enough that seeding it over the virtual transport
/// stays inside the suite's budget.
const WALKED_FILES: usize = 40;

/// A device connected over a host whose listing seam records, plus the volume to
/// walk it with.
async fn recording_walk_fixture() -> (Arc<RecordingListings>, Arc<MtpConnectionManager>, ConnectedDevice, MtpVolume) {
    let listings = Arc::new(RecordingListings::new());
    let host = VolumeHost::builder()
        .listings(Arc::clone(&listings) as Arc<dyn ListingHost>)
        .build();
    let manager = MtpConnectionManager::new(host, no_device_events(), recording_registrar());
    let device = connect_virtual_device(&manager).await;
    let volume = volume_for(&manager, &device, None).await;
    (listings, manager, device, volume)
}

/// Seeding, then walking, a directory on the device: the writes report one change
/// each and the reads report none, however many entries they cross.
///
/// ❗ This is the cell that would catch the drift. `notify_mutation` is the only
/// producer of listing changes on this backend (there's no local watcher behind
/// it), so the counter measures dispatch exactly rather than mixing two sources.
/// Every read below crosses the whole directory: a plain listing, a metadata
/// probe, a windowed stream, a ranged read, a copy scan, and a conflict scan.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_walk_over_a_directory_reports_nothing_however_many_entries_it_holds() {
    let _guard = device_lock().await;
    let (listings, manager, device, volume) = recording_walk_fixture().await;
    let dir = Path::new("/host-seam");

    volume.create_directory(dir).await.expect("creating the walked directory");
    for i in 0..WALKED_FILES {
        volume
            .write_from_stream(
                &dir.join(format!("file-{i:03}.txt")),
                1,
                Box::new(BytesStream::new(b"x".to_vec())),
                &|_bytes, _total| ControlFlow::Continue(()),
            )
            .await
            .expect("seeding a file on the device");
    }

    // The directory itself plus one file each: every WRITE reported exactly once.
    let after_seeding = listings.change_count();
    assert_eq!(
        after_seeding,
        WALKED_FILES + 1,
        "one call per mutation: seeding is {} mutations and must be that many changes",
        WALKED_FILES + 1
    );

    let entries = volume.list_directory(dir, None).await.expect("listing the directory");
    assert_eq!(entries.len(), WALKED_FILES, "the directory is there to be walked");
    assert_eq!(
        listings.change_count(),
        after_seeding,
        "a listing is a READ: it crossed every entry and must tell the panes nothing"
    );

    let one = dir.join("file-000.txt");
    volume.get_metadata(&one).await.expect("stat one entry");
    assert_eq!(
        listings.change_count(),
        after_seeding,
        "a stat lists the parent to find its entry, and still reports nothing"
    );

    let mut stream = volume.open_read_stream(&one).await.expect("open a read stream");
    while stream.next_chunk().await.is_some() {}
    volume.read_range(&one, 0, 1).await.expect("a ranged read");
    assert_eq!(
        listings.change_count(),
        after_seeding,
        "reading bytes off the device is a read too, windowed or ranged"
    );

    let scan = volume.scan_for_copy(dir).await.expect("a copy scan");
    assert_eq!(scan.file_count, WALKED_FILES, "the scan walked the same tree");
    let conflicts = volume
        .scan_for_conflicts(
            &[SourceItemInfo {
                name: "file-000.txt".to_string(),
                size: 1,
                modified: None,
                is_directory: false,
            }],
            dir,
        )
        .await
        .expect("a conflict scan");
    assert_eq!(conflicts.len(), 1, "the conflict scan saw the destination");
    assert_eq!(
        listings.change_count(),
        after_seeding,
        "both scans are reads, however deep they recurse"
    );

    // One more mutation still costs exactly one call, so the counter is measuring
    // dispatch rather than having stopped moving.
    volume.delete(&one).await.expect("deleting one entry");
    assert_eq!(
        listings.change_count(),
        after_seeding + 1,
        "a delete after the walk is one change, not one per entry the walk saw"
    );

    // And every change names the volume id and the parent the listing cache keys
    // on, which is what makes a patch land rather than get silently dropped.
    let changes = listings.changes();
    let parent_url = PathBuf::from(format!("mtp://{}/{}/host-seam", device.id, device.storage_id));
    assert!(
        changes
            .iter()
            .skip(1)
            .all(|(id, path, _)| *id == volume.volume_id && *path == parent_url),
        "every change inside the walked directory names it by its canonical URL: {:?}",
        changes.iter().map(|(id, path, _)| (id, path)).collect::<Vec<_>>()
    );
    assert!(
        matches!(&changes.last().expect("the delete").2, DirectoryChange::Removed(name) if name == "file-000.txt"),
        "the last change is the delete, carrying the name the cache removes by"
    );

    device.teardown(&manager).await;
}

/// A one-shot `VolumeReadStream` over bytes already in hand: what an upload's
/// source looks like when the test doesn't care where the bytes came from.
struct BytesStream {
    bytes: Option<Vec<u8>>,
    total: u64,
    read: u64,
}

impl BytesStream {
    fn new(bytes: Vec<u8>) -> Self {
        let total = bytes.len() as u64;
        Self {
            bytes: Some(bytes),
            total,
            read: 0,
        }
    }
}

impl VolumeReadStream for BytesStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let chunk = self.bytes.take()?;
            self.read += chunk.len() as u64;
            Some(Ok(chunk))
        })
    }

    fn total_size(&self) -> u64 {
        self.total
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }
}
