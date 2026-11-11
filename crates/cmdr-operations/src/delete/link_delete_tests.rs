//! A volume delete treats a symbolic link as a leaf: it removes the link and
//! never walks into what it points at (#505).
//!
//! Over a real `LocalPosixVolume` (an attached drive, a cloud drive), whose
//! listing reports a link to a folder with `is_directory` set, so a pane shows
//! it as a folder. Each cell covers one place a delete learns what an entry is:
//! a link inside the selected folder, and a selected link answered by a fresh
//! probe, by a cold scan preview, and by the parent's cached pane listing.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::super::event_sinks::CollectorEventSink;
use super::super::host::testing::PaneListings;
use super::super::state::{CachedScanResult, WriteOperationState};
use super::super::testing::TestOperations;
use super::super::types::WriteOperationConfig;
use super::volume_walker::delete_volume_files_with_progress_inner;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::{CopyScanResult, EntryKind, Volume, VolumeError, WatchCoverage};
use cmdr_listing::testing::TestListing;
use cmdr_local::LocalPosixVolume;

/// A drive holding `project/notes.txt` and `project/data -> ../photos`, with
/// the link's target `photos/keep.jpg` outside the selection.
struct Drive {
    dir: TestDir,
    volume: Arc<dyn Volume>,
}

impl Drive {
    fn new(name: &str) -> Self {
        let dir = TestDir::new(name);
        std::fs::create_dir_all(dir.join("project")).unwrap();
        std::fs::write(dir.join("project/notes.txt"), b"notes").unwrap();
        std::fs::create_dir_all(dir.join("photos")).unwrap();
        std::fs::write(dir.join("photos/keep.jpg"), b"the only copy").unwrap();
        cmdr_fs::testing::symlink(dir.join("photos"), dir.join("project/data")).unwrap();
        let volume = Arc::new(LocalPosixVolume::local_folder(
            "Drive",
            &*dir,
            super::super::host::testing::pane_order_host(),
        )) as Arc<dyn Volume>;
        Self { dir, volume }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }

    fn photos_intact(&self) -> bool {
        std::fs::read(self.path("photos/keep.jpg")).is_ok_and(|bytes| bytes == b"the only copy")
    }

    fn link_gone(&self) -> bool {
        std::fs::symlink_metadata(self.path("project/data")).is_err()
    }
}

async fn delete(
    engine: &TestOperations,
    volume: Arc<dyn Volume>,
    volume_id: &str,
    sources: &[PathBuf],
    config: WriteOperationConfig,
) -> Result<(), super::super::types::WriteOperationError> {
    let state: Arc<WriteOperationState> = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
    let sink = CollectorEventSink::new();
    delete_volume_files_with_progress_inner(volume, volume_id, &sink, volume_id, &state, sources, &config).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_inside_the_selected_folder_is_removed_and_its_target_kept() {
    let drive = Drive::new("link_delete_nested");
    let engine = TestOperations::new();

    let result = delete(
        &engine,
        Arc::clone(&drive.volume),
        "link-nested",
        &[drive.path("project")],
        WriteOperationConfig::default(),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert!(
        drive.photos_intact(),
        "the link's target is outside the selection and stays"
    );
    assert!(
        !drive.path("project").exists(),
        "the selected folder goes, link included"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_link_with_no_preview_is_removed_and_its_target_kept() {
    let drive = Drive::new("link_delete_top_level");
    let engine = TestOperations::new();

    let result = delete(
        &engine,
        Arc::clone(&drive.volume),
        "link-top-level",
        &[drive.path("project/data")],
        WriteOperationConfig::default(),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert!(drive.photos_intact());
    assert!(drive.link_gone(), "the link itself is what was selected");
    assert!(drive.path("project/notes.txt").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_link_a_cold_preview_called_a_folder_is_removed_and_its_target_kept() {
    let drive = Drive::new("link_delete_cold_preview");
    let engine = TestOperations::new();
    let link = drive.path("project/data");
    // The preview's top stat follows a link, so it saw a folder of one file.
    engine.stores().previews().insert_scan_result(
        "preview-link".to_string(),
        CachedScanResult::from_volume_batch(
            crate::types::ScanPreviewPurpose::Delete,
            vec![link.clone()],
            1,
            13,
            13,
            vec![(
                link.clone(),
                CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: 13,
                    dedup_bytes: 13,
                    top_level_is_directory: true,
                    top_level_modified_at: None,
                    top_level_created_at: None,
                },
            )],
        ),
    );

    let result = delete(
        &engine,
        Arc::clone(&drive.volume),
        "link-cold-preview",
        &[link],
        WriteOperationConfig {
            preview_id: Some("preview-link".to_string()),
            ..WriteOperationConfig::default()
        },
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert!(drive.photos_intact());
    assert!(drive.link_gone());
}

/// The drive above, its listings watched, so the parent's pane listing
/// answers what a selected entry is.
struct Watched {
    inner: Arc<dyn Volume>,
}

impl Volume for Watched {
    crate::forward_volume_methods!(inner => name, root, list_directory, get_metadata, exists, is_directory, delete);
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        self.inner.entry_kind(path)
    }
    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        WatchCoverage::EveryWriter
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_link_its_pane_listing_shows_as_a_folder_is_removed_and_its_target_kept() {
    let drive = Drive::new("link_delete_parent_hint");
    let panes = PaneListings::new();
    let engine = TestOperations::seeing(&panes);
    let watched: Arc<dyn Volume> = Arc::new(Watched {
        inner: Arc::clone(&drive.volume),
    });
    let volume_id = "link-parent-hint";
    engine.volumes().register(volume_id, Arc::clone(&watched));
    panes.show_volume(volume_id, Arc::clone(&watched));
    // What a pane shows for the link: a folder, with the link bit set.
    let project = drive.path("project");
    let link_entry = FileEntry::new(
        "data".to_string(),
        drive.path("project/data").display().to_string(),
        true,
        true,
    );
    let _listing = TestListing::new()
        .volume(volume_id)
        .path(project.display().to_string())
        .entries(vec![link_entry])
        .sequence(1)
        .insert(panes.panes(), "link-parent");

    let result = delete(
        &engine,
        watched,
        volume_id,
        &[drive.path("project/data")],
        WriteOperationConfig::default(),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert!(drive.photos_intact());
    assert!(drive.link_gone());
}
