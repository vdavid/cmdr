//! What this backend tells the host's listing seam, and HOW OFTEN.
//!
//! The seam rule the whole `VolumeHost` design rests on is a pace, not a shape:
//! **a seam may be called per mutation, never per directory entry**
//! (`crates/cmdr-fs/src/volume/host/DETAILS.md`). Every seam is a `dyn` trait
//! object, which is free at human cadence and is not free inside a loop over a
//! quarter of a million entries. Nothing about that rule is visible in a type, so
//! the instrument is [`RecordingListings::change_count`]: a walk that reports a
//! handful of changes is right, and one that reports one per entry fails loudly.
//!
//! SMB is the backend where the drift would cost the most, because every entry it
//! walks came off a network. The Docker cells below seed a directory, walk it two
//! ways, and assert the counter doesn't move: a listing and a scan are READS, and
//! a read tells the panes nothing. `cmdr-archive`'s `watch/host_seam_test.rs` is
//! the same idea against the other extracted backend.

use super::test_support::*;
use super::*;
use cmdr_fs::volume::host::listings::{ListingHost, RecordingListings};
use cmdr_fs::volume::{DirectoryChange, MutationEvent};

/// A recorder wired as the only seam a volume answers to.
fn recording_host() -> (Arc<RecordingListings>, VolumeHost) {
    let listings = Arc::new(RecordingListings::new());
    let host = VolumeHost::builder()
        .listings(Arc::clone(&listings) as Arc<dyn ListingHost>)
        .build();
    (listings, host)
}

/// Three mutations in one directory are three seam calls, each naming the volume
/// id the listing cache keys on and the parent that has to be patched.
///
/// `Deleted` needs no `stat`, so this runs with no session: the pace and the
/// addressing are what's under test, not the round trip.
#[tokio::test]
async fn a_mutation_reports_exactly_one_change_naming_its_parent() {
    let (listings, host) = recording_host();
    let vol = make_test_volume_with("volumestestshare", host);
    let parent = PathBuf::from("/Volumes/TestShare/Documents");

    for name in ["a.txt", "b.txt", "c.txt"] {
        vol.notify_mutation("volumestestshare", &parent, MutationEvent::Deleted(name.to_string()))
            .await;
    }

    assert_eq!(
        listings.change_count(),
        3,
        "one change per mutation: three deletes are three calls, not three per entry in the directory"
    );
    let changes = listings.changes();
    let addressing: Vec<(&str, &PathBuf)> = changes.iter().map(|(id, path, _)| (id.as_str(), path)).collect();
    assert!(
        addressing
            .iter()
            .all(|(volume_id, path)| *volume_id == "volumestestshare" && *path == &parent),
        "every change names the volume id and parent the listing cache keys on: {addressing:?}"
    );
    assert!(
        matches!(&changes[0].2, DirectoryChange::Removed(name) if name == "a.txt"),
        "the first change is the first delete, in the order the mutations arrived"
    );
}

/// A mutation the backend can't describe patches NOTHING.
///
/// `Created` has to `stat` the new entry to build it, and with no session that
/// `stat` can't answer. Reporting an `Added` anyway would put an invented entry
/// in the pane; the honest answer is a log line and no change.
#[tokio::test]
async fn a_creation_the_backend_cannot_stat_patches_nothing() {
    let (listings, host) = recording_host();
    let vol = make_test_volume_with("volumestestshare", host);

    vol.notify_mutation(
        "volumestestshare",
        Path::new("/Volumes/TestShare/Documents"),
        MutationEvent::Created("ghost.txt".to_string()),
    )
    .await;

    assert_eq!(
        listings.change_count(),
        0,
        "a stat that couldn't answer must not become an entry the pane shows"
    );
}

/// A connected share with NO watcher on it.
///
/// `connect_smb_volume` spawns the share watcher, which is a SECOND, independent
/// producer of `directory_changed` calls: it long-polls CHANGE_NOTIFY on the share
/// root and reports every write there, this test's own and every concurrent cell's
/// on the shared fixture share. Counting both producers together measures nothing,
/// and it goes non-deterministic the moment the lane runs under load. This suite is
/// about what the MUTATION path dispatches, so the session is wired by hand and the
/// watcher stays out of it. What the watcher reports has its own cells
/// (`watcher/archive_refresh_test.rs` and the app-side watch suites).
async fn unwatched_docker_volume(host: VolumeHost) -> SmbVolume {
    let params = docker_guest_params();
    let port = params.port;
    let (client, tree) = build_session(&params)
        .await
        .unwrap_or_else(|e| panic!("no fixture container at 127.0.0.1:{port} ({e:?})"));
    let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", port, "public");
    SmbVolume::new("public", TEST_MOUNT_ROOT, &volume_id, params, client, tree, host)
}

/// How many files the walked fixture holds. Big enough that one call per entry is
/// unmistakable next to the handful a correct walk makes, small enough that
/// seeding it stays inside the integration lane's budget.
const WALKED_FILES: usize = 40;

/// Seeding, then walking, a real directory: the writes report one change each and
/// the reads report none.
///
/// This is the cell that would catch the drift. A `notify_mutation` moved inside
/// `list_directory_impl`'s entry mapping, or inside the scan's recursion, costs
/// nothing a reviewer would notice and turns every listing of a big share folder
/// into a per-entry sweep of every cached listing on the volume.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_walk_over_a_directory_reports_nothing_however_many_entries_it_holds() {
    let (listings, host) = recording_host();
    let vol = unwatched_docker_volume(host).await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    for i in 0..WALKED_FILES {
        vol.create_file(Path::new(&format!("{dir}/file-{i:03}.txt")), b"x")
            .await
            .unwrap();
    }

    // The directory itself plus one file each: every WRITE reported exactly once.
    let after_seeding = listings.change_count();
    assert_eq!(
        after_seeding,
        WALKED_FILES + 1,
        "one call per mutation: seeding the fixture is {} mutations and must be that many changes",
        WALKED_FILES + 1
    );

    let entries = vol.list_directory_impl(Path::new(&dir)).await.unwrap();
    assert_eq!(entries.len(), WALKED_FILES, "the fixture is there to be walked");
    assert_eq!(
        listings.change_count(),
        after_seeding,
        "a listing is a READ: it crossed the whole fixture and must tell the panes nothing"
    );

    let scan = vol.scan_for_copy(Path::new(&dir)).await.unwrap();
    assert_eq!(scan.file_count, WALKED_FILES, "the scan walked the same tree");
    assert_eq!(
        listings.change_count(),
        after_seeding,
        "a copy scan is a READ too, however deep it recurses"
    );

    // And one more mutation still costs exactly one call, so the counter is
    // measuring dispatch rather than having stopped moving.
    vol.delete(Path::new(&format!("{dir}/file-000.txt"))).await.unwrap();
    assert_eq!(
        listings.change_count(),
        after_seeding + 1,
        "a delete after the walk is one change, not one per entry the walk saw"
    );

    ensure_clean(&vol, &dir).await;
}
