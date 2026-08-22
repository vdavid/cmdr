//! What this backend tells the host's listing seam, and HOW OFTEN.
//!
//! The rule the whole `VolumeHost` design rests on is a pace rather than a shape:
//! **a seam may be called per mutation, never per directory entry**
//! (`crates/cmdr-fs/src/volume/host/DETAILS.md`). Every seam is a `dyn` trait
//! object, which costs nothing at human cadence and is not free inside a loop
//! over a quarter of a million entries. Nothing about that rule is visible in a
//! type, so the instrument is `RecordingListings::change_count`: a walk that
//! reports a handful of changes is right, and one that reports one per entry
//! fails loudly.
//!
//! It matters more here than anywhere. This backend has no watcher, so
//! `notify_mutation` is the ONLY producer of listing changes — which makes the
//! counter an exact measure of dispatch rather than a mix of two sources, and
//! makes a stray call inside a listing loop the kind of thing that silently
//! doubles the cost of every big directory a user opens.

use std::path::Path;

use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::{DirectoryChange, MutationEvent, Volume};

use super::testing::*;

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

/// How many files the walked fixture holds.
///
/// Big enough that one call per entry is unmistakable next to the handful a
/// correct walk makes, small enough that seeding it stays inside the integration
/// lane's budget.
const WALKED_FILES: usize = 40;

/// Three mutations in one directory are three seam calls, each naming the volume
/// id the listing cache keys on and the parent that has to be patched.
///
/// `Deleted` needs no `stat`, so this runs with no session behind it: the pace
/// and the addressing are what's under test, not the round trip.
#[tokio::test]
async fn a_mutation_reports_exactly_one_change_naming_its_parent() {
    let listings = std::sync::Arc::new(cmdr_fs::volume::host::listings::RecordingListings::new());
    let host = cmdr_fs::volume::host::VolumeHost::builder()
        .listings(std::sync::Arc::clone(&listings) as std::sync::Arc<dyn ListingHost>)
        .build();
    let volume = super::test_support::make_test_volume_with(
        super::test_support::TEST_ROOT,
        crate::auth::AuthRungUsed::Agent,
        host,
    );
    let parent = Path::new("/srv/data/photos");

    for name in ["a.txt", "b.txt", "c.txt"] {
        volume
            .notify_mutation(volume.volume_id(), parent, MutationEvent::Deleted(name.to_string()))
            .await;
    }

    assert_eq!(
        listings.change_count(),
        3,
        "one change per mutation: three deletes are three calls, not three per entry in the directory"
    );
    let changes = listings.changes();
    let addressing: Vec<(&str, &std::path::PathBuf)> =
        changes.iter().map(|(id, path, _)| (id.as_str(), path)).collect();
    assert!(
        addressing
            .iter()
            .all(|(id, path)| *id == volume.volume_id() && path.as_path() == parent),
        "every change names the volume id and parent the listing cache keys on: {addressing:?}"
    );
    assert!(
        matches!(&changes[0].2, DirectoryChange::Removed(name) if name == "a.txt"),
        "the first change is the first delete, in the order the mutations arrived"
    );
}

/// Seeding, then walking, a real directory: the writes report one change each and
/// the reads report none.
///
/// ❗ This is the cell that would catch the drift. A `notify_mutation` moved
/// inside `list_directory_impl`'s entry mapping, or inside the scan's recursion,
/// costs nothing a reviewer would notice and turns every listing of a big remote
/// folder into a per-entry sweep of every cached listing on the volume.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_walk_over_a_directory_reports_nothing_however_many_entries_it_holds() {
    let params = fixture_params("OPENSSH", 12480);
    let (host, listings) = fixture_host_recording(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir("host-seam");
    clean_scratch(&volume, &dir).await;

    volume.create_directory(Path::new(&dir)).await.expect(FIXTURE);
    for i in 0..WALKED_FILES {
        volume
            .create_file(Path::new(&format!("{dir}/file-{i:03}.txt")), b"x")
            .await
            .expect(FIXTURE);
    }

    // The directory itself plus one file each: every WRITE reported exactly once.
    let after_seeding = listings.change_count();
    assert_eq!(
        after_seeding,
        WALKED_FILES + 1,
        "one call per mutation: seeding the fixture is {} mutations and must be that many changes",
        WALKED_FILES + 1
    );

    let entries = volume.list_directory(Path::new(&dir), None).await.expect(FIXTURE);
    assert_eq!(entries.len(), WALKED_FILES, "the fixture is there to be walked");
    assert_eq!(
        listings.change_count(),
        after_seeding,
        "a listing is a READ: it crossed the whole fixture and must tell the panes nothing"
    );

    let scan = volume.scan_for_copy(Path::new(&dir)).await.expect(FIXTURE);
    assert_eq!(scan.file_count, WALKED_FILES, "the scan walked the same tree");
    assert_eq!(
        listings.change_count(),
        after_seeding,
        "a copy scan is a READ too, however deep it recurses"
    );

    let conflicts = volume
        .scan_for_conflicts(
            &[cmdr_fs::volume::SourceItemInfo {
                name: "file-000.txt".to_string(),
                size: 1,
                modified: None,
                is_directory: false,
            }],
            Path::new(&dir),
        )
        .await
        .expect(FIXTURE);
    assert_eq!(conflicts.len(), 1, "the conflict scan saw the destination");
    assert_eq!(
        listings.change_count(),
        after_seeding,
        "and reading a destination for taken names is a read as well"
    );

    // One more mutation still costs exactly one call, so the counter is measuring
    // dispatch rather than having stopped moving.
    volume
        .delete(Path::new(&format!("{dir}/file-000.txt")))
        .await
        .expect(FIXTURE);
    assert_eq!(
        listings.change_count(),
        after_seeding + 1,
        "a delete after the walk is one change, not one per entry the walk saw"
    );

    clean_scratch(&volume, &dir).await;
}
