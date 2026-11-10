//! The engine on the app's volume registry: the routing a test's fake registry
//! (`host::testing::FakeVolumes`) doesn't do. Archives and the git portal route
//! inside the app's registry, and an id nothing knows reads as gone or not
//! connected yet through the app's `unregistered_volumes`, so each cell drives
//! an engine over the app's own answers (`test_engine`, or a test engine whose
//! volume host is the app's) and registers its volumes in the app's registry
//! under ids of its own. The engine's half of each question, on fakes, lives
//! beside the code it pins.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::file_system::volume::manager::get_volume_manager;
use cmdr_archive::mutator::{AddEntry, AddSource, Changeset};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::{TestDir, wait_until_async};
use cmdr_fs::volume::{InMemoryVolume, RoutedKind, Volume};
use cmdr_operation_log::types::Initiator;
use cmdr_operations::testing::{
    ArchiveEditRequest, CollectorEventSink, OperationSummaryText, TestOperations, archive_inner_exists,
};
use cmdr_operations::{MutationError, OperationEventSink};

/// A real zip of `entries`, as bytes.
fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        for (name, content) in entries {
            writer.start_file(*name, SimpleFileOptions::default()).expect("start");
            writer.write_all(content).expect("write");
        }
        writer.finish().expect("finish");
    }
    cursor.into_inner()
}

/// A real zip of `entries`, written at `path`.
fn write_multi_zip(path: &Path, entries: &[(&str, &[u8])]) {
    std::fs::write(path, zip_bytes(entries)).expect("write zip");
}

/// A NON-local `InMemoryVolume` (the remote-parent stand-in) holding a zip at
/// `archive_path`, under a unique volume id that is its lane too.
async fn remote_zip(archive_path: &Path, entries: &[(&str, &[u8])]) -> (String, Arc<InMemoryVolume>) {
    let id = format!("remote-test-{}", uuid::Uuid::new_v4());
    let parent = InMemoryVolume::new("Remote").with_lane_key(id.clone());
    if let Some(dir) = archive_path.parent() {
        parent.create_directory(dir).await.expect("seed parent dir");
    }
    parent
        .create_file(archive_path, &zip_bytes(entries))
        .await
        .expect("seed remote zip");
    (id, Arc::new(parent))
}

fn create_test_dir(name: &str) -> TestDir {
    TestDir::new(&format!("rename_mod_test_{}", name))
}

// ============================================================================
// Which volume a transfer's source resolves to, and an id nothing knows
// ============================================================================

#[tokio::test]
async fn resolve_source_treats_the_zip_file_itself_as_a_plain_file() {
    use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
    use cmdr_local::LocalPosixVolume;

    let engine = super::test_engine();

    let dir = tempfile::tempdir().expect("tempdir");
    let zip = dir.path().join("bundle.zip");
    std::fs::write(&zip, b"PK\x03\x04rest").expect("write zip magic");
    // The parent drive holds the `.zip`, under an id of this cell's own in the
    // app's registry, where the archive routing looks.
    let parent_id = cmdr_fs::testing::unique_test_id("routing-zip-parent");
    let _parent = TestVolumeRegistration::install(
        &parent_id,
        Arc::new(LocalPosixVolume::local_folder(
            "Root",
            dir.path().to_str().unwrap(),
            crate::volume_host::host(),
        )),
    );

    // The `.zip` FILE itself is copied as a plain file: routed to the PARENT
    // volume, `is_inside = false` (NOT the ArchiveVolume, which would scan its
    // contents instead of copying the file).
    let (vol, route) = engine
        .resolve_source_volume(&parent_id, Some(&zip))
        .await
        .expect("source volume");
    assert_eq!(route, None, "the .zip file itself is not archive-inner");
    assert_eq!(vol.name(), "Root", "routed to the parent volume, not the archive");

    // A path INSIDE the archive routes to the ArchiveVolume. This is what makes
    // an EXTRACT reachable as an ordinary copy.
    let (inner_vol, inner_route) = engine
        .resolve_source_volume(&parent_id, Some(&zip.join("entry.txt")))
        .await
        .expect("inner volume");
    assert_eq!(inner_route, Some(RoutedKind::Archive), "an inner path is archive-inner");
    assert_eq!(inner_vol.root(), zip, "the archive volume's root is the .zip");
}

/// A snapshot path is the portal's, so the copy engine reads it with
/// `open_read_stream` and walks it with `scan_for_copy`. Left on the parent
/// volume it would take the local-to-local fast path against a path with no
/// inode: the transfer dialog then sits on "Verifying before copy" forever and
/// ends in "Couldn't finish copying".
#[tokio::test]
async fn resolve_source_routes_a_snapshot_path_to_the_git_portal() {
    use crate::file_system::git;
    use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
    use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};
    use cmdr_local::LocalPosixVolume;

    let engine = super::test_engine();
    let dir = temp_dir("write_routing", "copy_out");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"hello\n", "initial");
    // The repo's drive, under an id of this cell's own in the app's registry,
    // where the portal routing looks.
    let parent_id = cmdr_fs::testing::unique_test_id("routing-repo-parent");
    let _parent = TestVolumeRegistration::install(
        &parent_id,
        Arc::new(LocalPosixVolume::local_folder(
            "Root",
            dir.to_str().unwrap(),
            crate::volume_host::host(),
        )),
    );
    git::wiring::set_virtual_portal_enabled(true);

    let (volume, route) = engine
        .resolve_source_volume(&parent_id, Some(&dir.join(".git/branches/main/README.md")))
        .await
        .expect("source volume");
    assert_eq!(route, Some(RoutedKind::GitPortal));
    assert_eq!(volume.name(), ".git", "the portal serves it, not the parent drive");
    assert_eq!(
        volume.local_path(),
        None,
        "no local path, so the copy takes the cross-volume engine"
    );

    // `.git/` itself and the real files under it are the parent's, so editing
    // and deleting them keeps working.
    let (parent, real_route) = engine
        .resolve_source_volume(&parent_id, Some(&dir.join(".git/config")))
        .await
        .expect("source volume");
    assert_eq!(real_route, None);
    assert_eq!(parent.name(), "Root");

    cleanup(&dir);
}

/// An id no provider lists and no saved server names keeps reading as a volume
/// that's gone (an unmount race), ❌ never "not connected yet": there's nothing to
/// open, so that advice would send the user looking for a row that isn't there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_onto_an_id_nothing_knows_still_reads_as_a_missing_volume() {
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_operation_log::types::Initiator;
    use cmdr_operations::{VolumeCopyConfig, WriteOperationError};

    let engine = super::test_engine();
    let source_id = format!("routing-source-{}", uuid::Uuid::new_v4());
    let source = InMemoryVolume::new("Source");
    source
        .create_file(Path::new("/a.txt"), b"a")
        .await
        .expect("the source file");
    get_volume_manager().register(&source_id, Arc::new(source));

    let refused = engine
        .start_volume_copy(
            Arc::new(CollectorEventSink::new()),
            source_id.clone(),
            vec![Path::new("/a.txt").to_path_buf()],
            "no-provider-or-saved-server-knows-this".to_string(),
            "/photos".to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await
        .expect_err("a copy onto an unknown volume is refused");
    get_volume_manager().unregister(&source_id);

    assert!(
        matches!(refused, WriteOperationError::IoError { .. }),
        "an unknown id is a missing volume; got {refused:?}"
    );
}

/// A copy FROM an id that left the registry and nothing lists any more (a phone
/// unplugged under a search-results pane) says the source isn't connected any
/// more, ❌ never "not connected yet" (there's no row to open) and ❌ never a bare
/// "volume not found" naming an internal id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_from_an_id_nothing_knows_reads_as_a_source_no_longer_connected() {
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_operation_log::types::Initiator;
    use cmdr_operations::{VolumeCopyConfig, WriteOperationError};

    let engine = super::test_engine();
    let dest_id = format!("routing-dest-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(&dest_id, Arc::new(InMemoryVolume::new("Dest")));

    let refused = engine
        .start_volume_copy(
            Arc::new(CollectorEventSink::new()),
            "an-unplugged-phone-nothing-lists".to_string(),
            vec![Path::new("/sdcard/DCIM/a.jpg").to_path_buf()],
            dest_id.clone(),
            "/".to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await
        .expect_err("a copy from an unknown volume is refused");
    get_volume_manager().unregister(&dest_id);

    assert!(
        matches!(&refused, WriteOperationError::SourceNoLongerConnected { path } if path == "/sdcard/DCIM/a.jpg"),
        "an unknown source id is a source no longer connected; got {refused:?}"
    );
}

// ============================================================================
// A rename inside an archive
// ============================================================================

#[tokio::test]
async fn rename_managed_routes_an_in_archive_rename_to_the_edit_driver() {
    // Routing detection is parent-aware (`VolumeManager::path_is_inside_archive`),
    // so it needs a registered local `"root"` volume to confirm the boundary — as
    // production always has. (nextest isolates the global per test process.)
    get_volume_manager().register_if_absent(
        "root",
        Arc::new(cmdr_local::LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::volume_host::host(),
        )),
    );

    // An engine over the app's own answers: the boundary check asks the app's
    // registry, which `FakeVolumes` doesn't route through.
    let engine = super::test_engine();
    let tmp = create_test_dir("archive_rename");
    let zip = tmp.join("bundle.zip");
    std::fs::write(&zip, b"PK\x03\x04not-a-real-body").expect("write zip magic");

    // A rename INSIDE the archive routes to the managed edit driver. With no
    // host installed in the unit test, `host_operation_sink()` is absent, so routing
    // surfaces the "app isn't ready" signal — which proves the ROUTING fork fired
    // rather than the old flat "isn't available yet" refusal or an instant rename.
    let from = zip.join("old.txt");
    let to = zip.join("new.txt");
    let err = engine
        .rename_managed(from, to, false, "root".to_string(), Initiator::User)
        .await
        .expect_err("routing needs an app handle the unit test doesn't wire");
    // The archive-specific variant is how we tell the routing fork fired from a
    // natural rename failure.
    assert!(
        matches!(
            err,
            MutationError::ArchiveNotEditable | MutationError::ArchiveEditNotReady
        ),
        "expected the archive-routing signal, got: {err:?}"
    );

    // A cross-boundary rename (OUT of the archive) is refused as a move, a
    // deterministic routing decision that needs no app handle.
    let outside = tmp.join("out.txt");
    let cross = engine
        .rename_managed(zip.join("old.txt"), outside, false, "root".to_string(), Initiator::User)
        .await
        .expect_err("a cross-boundary rename is refused");
    assert!(
        matches!(cross, MutationError::RenameOutOfArchive),
        "a cross-boundary rename should suggest a move instead, got: {cross:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

// ============================================================================
// The duplicate pre-check inside a remote archive
// ============================================================================

#[tokio::test]
async fn archive_inner_exists_detects_a_duplicate_in_a_remote_archive() {
    // The mkdir/mkfile duplicate pre-check must see entries inside a REMOTE archive
    // (through the parent volume), not fail open by attempting a local file open.
    let archive_path = PathBuf::from("/device/bundle.zip");
    let (parent_id, parent) = remote_zip(&archive_path, &[("existing.txt", b"x")]).await;
    // An engine over the app's own answers: the archive inside the parent routes
    // through the app's registry, which `FakeVolumes` doesn't route through.
    get_volume_manager().register(&parent_id, parent);
    let engine = super::test_engine();

    assert!(
        archive_inner_exists(engine.host(), &parent_id, &archive_path, "existing.txt").await,
        "a duplicate inside a remote archive must be detected"
    );
    assert!(
        !archive_inner_exists(engine.host(), &parent_id, &archive_path, "not_there.txt").await,
        "a non-existent inner path reports absent"
    );

    get_volume_manager().unregister(&parent_id);
}

// ============================================================================
// A pane open inside an archive the edit rewrote
// ============================================================================

/// A unique id giving one archive-edit op its own lane (`archive_edit/test_support.rs`).
fn unique_lane_id() -> String {
    format!("test-lane-{}", uuid::Uuid::new_v4())
}

/// A pane open inside a zip shows the edit as soon as it completes. The FE's
/// post-op `refresh_listing` skips an archive listing (its volume claims a
/// watch), and the FSEvents content watch is too slow and lossy to carry the
/// app's own edit, so the edit refreshes the open listings itself, before the
/// terminal event (getcmdr/cmdr-reports#55: a copy into a zip, then an F8 inside
/// it, each left the pane stale until re-entered). No watch runs in this test.
mod an_open_listing_inside_the_zip {
    use super::*;
    use cmdr_fs::volume::InMemoryVolume;
    use cmdr_listing::testing::TestListing;

    /// Registers a local parent drive on `engine` and the app, writes the zip on
    /// it, and opens a listing at the zip's root showing `entries`.
    struct OpenZip {
        _tmp: tempfile::TempDir,
        path: PathBuf,
        parent: String,
        listing: cmdr_listing::testing::TestListingGuard,
    }

    impl OpenZip {
        async fn new(engine: &TestOperations, tag: &str, entries: &[(&str, &[u8])]) -> Self {
            let tmp = tempfile::tempdir().expect("tempdir");
            let path = tmp.path().join("a.zip");
            write_multi_zip(&path, entries);
            let parent = unique_lane_id();
            let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("parent").with_local_fs_access());
            engine.volumes().register(&parent, Arc::clone(&volume));
            // The listing engine resolves the pane's archive path through the app's registry.
            get_volume_manager().register(&parent, volume);
            assert!(get_volume_manager().resolve(&parent, &path).await.is_routed());
            let names: Vec<_> = entries
                .iter()
                .map(|(name, _)| {
                    cmdr_fs::entry::FileEntry::new(
                        (*name).to_string(),
                        path.join(name).to_string_lossy().into_owned(),
                        false,
                        false,
                    )
                })
                .collect();
            let listing = TestListing::new()
                .volume(&parent)
                .path(&path)
                .entries(names)
                .insert(crate::listing_host::listings(), tag);
            Self {
                _tmp: tmp,
                path,
                parent,
                listing,
            }
        }

        fn names(&self) -> Vec<String> {
            let mut names = self.listing.entry_names();
            names.sort();
            names
        }
    }

    impl Drop for OpenZip {
        fn drop(&mut self) {
            get_volume_manager().unregister(&self.parent);
        }
    }

    async fn wait_for_complete(events: &CollectorEventSink) {
        wait_until_async(Duration::from_secs(5), "the write-complete event", || {
            !events.complete.lock_ignore_poison().is_empty()
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shows_a_file_copied_in() {
        let engine = TestOperations::with(|builder| builder.volume_host(crate::volume_host::host()));
        let zip = OpenZip::new(&engine, "archive-edit-refresh-add", &[("keep.txt", b"keep")]).await;
        let events = Arc::new(CollectorEventSink::new());
        let request = ArchiveEditRequest {
            archive_path: zip.path.clone(),
            parent_volume_id: zip.parent.clone(),
            changeset: Changeset {
                adds: vec![AddEntry {
                    inner_path: "added.txt".to_string(),
                    source: AddSource::Bytes(b"new bytes".to_vec()),
                }],
                ..Default::default()
            },
            summary: OperationSummaryText::default(),
            skipped_count: 0,
            preview_id: None,
        };

        cmdr_operations::testing::archive_edit_start(
            &engine,
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            request,
            0,
        )
        .await
        .expect("start archive edit");
        wait_for_complete(&events).await;

        assert_eq!(zip.names(), vec!["added.txt", "keep.txt"]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn drops_a_file_deleted_inside_it() {
        let engine = TestOperations::with(|builder| builder.volume_host(crate::volume_host::host()));
        let zip = OpenZip::new(
            &engine,
            "archive-edit-refresh-delete",
            &[("keep.txt", b"keep"), ("drop.txt", b"drop")],
        )
        .await;
        let events = Arc::new(CollectorEventSink::new());

        cmdr_operations::testing::route_archive_delete(
            &engine,
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            &[zip.path.join("drop.txt")],
            &zip.parent,
            0,
            None,
        )
        .await
        .expect("start delete");
        wait_for_complete(&events).await;

        assert_eq!(zip.names(), vec!["keep.txt"]);
    }
}
