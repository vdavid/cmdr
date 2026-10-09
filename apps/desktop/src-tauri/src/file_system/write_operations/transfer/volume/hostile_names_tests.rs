//! A listing from a hostile server or device names an entry `../x`, `/x`, or
//! `a/b`. Every engine that joins a listed name onto a destination path has to
//! refuse it, typed, rather than write outside the folder the user picked.
//!
//! The source lies through `InMemoryVolume::set_reported_name` (listed under
//! the hostile name, still readable at its real path, as an MTP object is), and
//! the destination is a real `LocalPosixVolume` over a tempdir, so an escape is
//! a file on disk the cell can see. The guard sits in the engine, under every
//! backend, so a backend-blind source stands in for all of them.

#![cfg(unix)]

use super::copy::copy_volumes_with_progress;
use super::faulty_volume::forward_volume_methods;
use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{make_state, write_file};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{InMemoryVolume, ListingProgress, LocalPosixVolume, Volume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{VolumeCopyConfig, WriteOperationError};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tempfile::TempDir;

/// Every name class that isn't one plain path component.
const HOSTILE_NAMES: &[&str] = &[
    "../escape.txt",
    "/escape.txt",
    "sub/../../escape.txt",
    "nested/escape.txt",
    "..",
    ".",
    "",
    "nul\0escape.txt",
];

fn config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    }
}

/// Every regular file under `root`, relative to it.
fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(root).expect("under root").to_path_buf());
            }
        }
    }
    out
}

fn assert_refused_as_invalid_name(error: &WriteOperationError, name: &str) {
    assert!(
        matches!(error, WriteOperationError::InvalidName { .. }),
        "a listed name {name:?} must be refused as InvalidName, got {error:?}"
    );
}

/// A folder copied off a hostile source: the bad child is refused and nothing
/// lands anywhere but inside the copied folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_refuses_a_listed_name_that_isnt_one_plain_component() {
    for &name in HOSTILE_NAMES {
        let source = InMemoryVolume::new("Phone").with_space_info(10_000_000, 10_000_000);
        source.create_directory(Path::new("/album")).await.unwrap();
        source.create_file(Path::new("/album/evil.txt"), b"EVIL").await.unwrap();
        source.set_reported_name(Path::new("/album/evil.txt"), name);
        let source: Arc<dyn Volume> = Arc::new(source);

        let dest_dir = TempDir::new().unwrap();
        std::fs::create_dir(dest_dir.path().join("inbox")).unwrap();
        let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Dest", dest_dir.path().to_path_buf()));

        let result = copy_volumes_with_progress(
            Arc::new(CollectorEventSink::new()),
            "hostile-name-copy",
            &make_state(),
            source,
            &[PathBuf::from("/album")],
            dest,
            Path::new("/inbox"),
            &config(),
        )
        .await;

        let failure = result.expect_err(&format!("a listed name {name:?} must fail the copy"));
        assert_refused_as_invalid_name(&failure.error, name);
        for file in files_under(dest_dir.path()) {
            assert!(
                file.parent() == Some(Path::new("inbox/album")),
                "{name:?} put {} outside the copied folder",
                file.display()
            );
        }
    }
}

/// A folder listed under a hostile name is refused before the walk recurses
/// into it: `..` would otherwise merge its children into the destination's
/// parent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_refuses_a_folder_listed_under_a_parent_name() {
    let source = InMemoryVolume::new("Phone").with_space_info(10_000_000, 10_000_000);
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/payload.txt"), b"EVIL")
        .await
        .unwrap();
    source.set_reported_name(Path::new("/album/sub"), "..");
    let source: Arc<dyn Volume> = Arc::new(source);

    let dest_dir = TempDir::new().unwrap();
    std::fs::create_dir(dest_dir.path().join("inbox")).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Dest", dest_dir.path().to_path_buf()));

    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "hostile-dir-copy",
        &make_state(),
        source,
        &[PathBuf::from("/album")],
        dest,
        Path::new("/inbox"),
        &config(),
    )
    .await;

    let failure = result.expect_err("a folder listed as `..` must fail the copy");
    assert_refused_as_invalid_name(&failure.error, "..");
    assert!(
        !dest_dir.path().join("inbox/payload.txt").exists(),
        "the folder's children landed in the destination's parent"
    );
}

/// The symlink-then-folder trick, the half that needs a link already at the
/// destination: `inbox/album/a` is a link to a folder outside, and the source
/// brings a FOLDER `a` holding `x`. The walk must treat the link as a leaf (a
/// type clash for the policy), ❌ never merge through it, under every policy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_never_merges_through_a_destination_link() {
    use crate::file_system::write_operations::types::ConflictResolution;
    for policy in [
        ConflictResolution::Skip,
        ConflictResolution::Overwrite,
        ConflictResolution::Rename,
        ConflictResolution::OverwriteSmaller,
    ] {
        let source = InMemoryVolume::new("Phone").with_space_info(10_000_000, 10_000_000);
        source.create_directory(Path::new("/album")).await.unwrap();
        source.create_directory(Path::new("/album/a")).await.unwrap();
        source.create_file(Path::new("/album/a/x"), b"EVIL").await.unwrap();
        let source: Arc<dyn Volume> = Arc::new(source);

        let dest_dir = TempDir::new().unwrap();
        let outside = dest_dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(dest_dir.path().join("inbox/album")).unwrap();
        std::os::unix::fs::symlink(&outside, dest_dir.path().join("inbox/album/a")).unwrap();
        let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Dest", dest_dir.path().to_path_buf()));

        let _ = copy_volumes_with_progress(
            Arc::new(CollectorEventSink::new()),
            "link-then-dir-copy",
            &make_state(),
            source,
            &[PathBuf::from("/album")],
            dest,
            Path::new("/inbox"),
            &VolumeCopyConfig {
                conflict_resolution: policy,
                ..config()
            },
        )
        .await;

        assert!(
            !outside.join("x").exists(),
            "{policy:?}: the folder merged through the destination link"
        );
    }
}

/// The other half: one listing names a FILE (standing in for a source link,
/// which no backend recreates as a link) and a FOLDER the same. The folder
/// lands on the name the file took in a level this copy made, so nothing can
/// lead it outside.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_with_a_file_and_folder_listed_under_one_name_stays_inside() {
    let source = InMemoryVolume::new("Phone").with_space_info(10_000_000, 10_000_000);
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_file(Path::new("/album/link"), b"LINK").await.unwrap();
    source.set_reported_name(Path::new("/album/link"), "a");
    source.create_directory(Path::new("/album/a")).await.unwrap();
    source.create_file(Path::new("/album/a/x"), b"EVIL").await.unwrap();
    let source: Arc<dyn Volume> = Arc::new(source);

    let dest_dir = TempDir::new().unwrap();
    std::fs::create_dir(dest_dir.path().join("inbox")).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Dest", dest_dir.path().to_path_buf()));

    let _ = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "file-and-dir-one-name",
        &make_state(),
        source,
        &[PathBuf::from("/album")],
        dest,
        Path::new("/inbox"),
        &config(),
    )
    .await;

    for file in files_under(dest_dir.path()) {
        assert!(
            file.starts_with("inbox/album"),
            "{} landed outside the copied folder",
            file.display()
        );
    }
}

/// A `LocalPosixVolume` whose listings report `reported` for the entry really
/// named `real`, the same-volume twin of `set_reported_name`.
struct RenamingListings {
    inner: LocalPosixVolume,
    real: String,
    reported: String,
}

impl Volume for RenamingListings {
    forward_volume_methods!(inner => name, root, get_metadata, exists, is_directory, create_directory, delete, rename,
        get_space_info, supports_streaming, scan_for_copy, open_read_stream, write_from_stream);
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
                if entry.name == self.real {
                    entry.name = self.reported.clone();
                }
            }
            Ok(entries)
        })
    }
}

/// The same-volume rename-merge joins listed names onto the destination too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_merge_refuses_a_listed_name_that_isnt_one_plain_component() {
    for &name in ["../escape.txt", "/escape.txt", "nested/escape.txt"].iter() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write_file(root, "src/album/evil.txt", b"EVIL");
        write_file(root, "dst/album/keep.txt", b"KEEP");
        let volume: Arc<dyn Volume> = Arc::new(RenamingListings {
            inner: LocalPosixVolume::local_folder("V", root.to_path_buf()),
            real: "evil.txt".to_string(),
            reported: name.to_string(),
        });

        let result = move_within_same_volume_with_progress(
            Arc::new(CollectorEventSink::new()),
            "hostile-name-rename-merge",
            &make_state(),
            volume,
            &[PathBuf::from("src/album")],
            Path::new("dst"),
            &config(),
        )
        .await;

        let error = result.expect_err(&format!("a listed name {name:?} must fail the move"));
        assert_refused_as_invalid_name(&error, name);
        assert!(!root.join("dst/escape.txt").exists(), "{name:?} escaped to dst/");
        assert!(!root.join("escape.txt").exists(), "{name:?} escaped to the volume root");
        assert!(
            root.join("src/album/evil.txt").exists(),
            "{name:?}: the refused source stays put"
        );
    }
}
