//! The ` (N)` placeholder a deep-merge `Rename` reserves, and what happens to it
//! when the child that was going to fill it never lands.
//!
//! `naming.rs::find_unique_volume_name` reserves the chosen name on a local-FS
//! destination with an `O_CREAT|O_EXCL` zero-byte file, which is the TOCTOU guard
//! that stops a concurrent writer taking the name. Nothing else on disk tells
//! that file apart from a real one, so a leaf that gives up after the
//! reservation has to take it back: an empty `clash (1).txt` sitting in the
//! user's folder looks exactly like a file the copy produced.
//!
//! Needs a real local-FS destination: the `O_EXCL` branch only runs when
//! `dest_volume.local_path()` is `Some`.

use super::tests::make_state;
use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::backends::LocalPosixVolume;
use crate::file_system::volume::{InMemoryVolume, VolumeError, VolumeReadStream};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::ConflictResolution;

use super::super::faulty_volume::forward_volume_methods;

/// A source whose read stream refuses for ONE path, so a single merge child
/// fails while its siblings copy. `IoError` with no errno isn't retryable
/// (`retry.rs::is_retryable`), so the leaf gives up on the first attempt.
struct FailReadForPath {
    inner: Arc<InMemoryVolume>,
    fail_for: PathBuf,
}

impl Volume for FailReadForPath {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, exists, is_directory, create_file, create_directory,
        create_directory_all, delete, rename, get_space_info, supports_streaming, supports_export,
        operations_are_local, max_concurrent_ops, scan_for_copy, write_from_stream, local_path,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        if path == self.fail_for {
            return Box::pin(async move {
                Err(VolumeError::IoError {
                    message: "simulated source read failure".to_string(),
                    raw_os_error: None,
                })
            });
        }
        self.inner.open_read_stream(path)
    }
}

/// The names sitting in the destination's `album` folder right now.
fn names_in_album(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root.join("album"))
        .expect("the merged folder exists")
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// A deep-merge child resolved to `Rename` whose copy then fails must leave no
/// empty ` (1)` file behind.
///
/// Pre-fix the reservation was made by `find_unique_volume_name` and recorded
/// nowhere: `copy_leaf` records into `CreatedPaths` only after the stream
/// succeeds, so a failed leaf left a zero-byte `clash (1).txt` that no rollback
/// knew about and no sweep could reach. Top-level sources never had the problem
/// (their write path is tracked as an in-flight partial).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merge_child_that_never_lands_takes_its_rename_placeholder_back() {
    let dest_dir = tempfile::tempdir().expect("tempdir");
    let root = dest_dir.path().to_path_buf();
    std::fs::create_dir(root.join("album")).unwrap();
    std::fs::write(root.join("album").join("clash.txt"), b"the user's file").unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", root.clone()));

    let source_inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source_inner.create_directory(Path::new("/album")).await.unwrap();
    source_inner
        .create_file(Path::new("/album/clash.txt"), b"the incoming file")
        .await
        .unwrap();
    source_inner
        .create_file(Path::new("/album/fine.txt"), b"lands cleanly")
        .await
        .unwrap();
    let source: Arc<dyn Volume> = Arc::new(FailReadForPath {
        inner: source_inner,
        fail_for: PathBuf::from("/album/clash.txt"),
    });

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        // Rename is what reserves the ` (N)` placeholder.
        conflict_resolution: ConflictResolution::Rename,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-merge-rename-placeholder",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_err(), "the unreadable child must fail the copy: {result:?}");

    let names = names_in_album(&root);
    assert!(
        !names.iter().any(|n| n.contains("(1)")),
        "the reservation for a child that never landed must be taken back; found {names:?}"
    );
    assert_eq!(
        std::fs::read(root.join("album").join("clash.txt")).unwrap(),
        b"the user's file",
        "and the file the Rename was avoiding is untouched"
    );
}

/// The guard the fix must not break: a merge child resolved to `Rename` that
/// DOES land keeps its ` (N)` name and its bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merge_child_that_lands_keeps_its_rename_name() {
    let dest_dir = tempfile::tempdir().expect("tempdir");
    let root = dest_dir.path().to_path_buf();
    std::fs::create_dir(root.join("album")).unwrap();
    std::fs::write(root.join("album").join("clash.txt"), b"the user's file").unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", root.clone()));

    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_directory(Path::new("/album")).await.unwrap();
    source
        .create_file(Path::new("/album/clash.txt"), b"the incoming file")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Rename,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    copy_volumes_with_progress(
        events.clone(),
        "op-merge-rename-placeholder-ok",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await
    .expect("the merge should complete");

    assert_eq!(
        std::fs::read(root.join("album").join("clash (1).txt")).unwrap(),
        b"the incoming file",
        "the renamed child keeps its bytes"
    );
    assert_eq!(
        std::fs::read(root.join("album").join("clash.txt")).unwrap(),
        b"the user's file"
    );
}

/// The unused-import guard: the forwarding macro's expansion names this type.
#[allow(dead_code, reason = "the macro expansion names the type")]
type ListingRow = FileEntry;
