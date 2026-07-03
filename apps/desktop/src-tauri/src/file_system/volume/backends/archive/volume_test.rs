//! Tests for the `ArchiveVolume` `Volume` impl, driven against real zip files
//! written to a temp path (the volume reads through `LocalFileSource`, which
//! needs a real file). Clean zips are built with the reading core's fixture
//! builders; hostile variants byte-patch a clean one.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::super::test_fixtures::{
    FixtureFile, build_zip, deflated, dir, overstate_record_count, set_first_entry_encrypted, stored,
};
use super::*;
use crate::file_system::volume::backends::InMemoryVolume;
use crate::file_system::volume::{ListingProgress, Volume, VolumeError, VolumeReadStream};
use crate::ignore_poison::IgnorePoison;

/// A zip written to a unique temp file, cleaned up on drop. Hands out
/// `ArchiveVolume`s backed by a configurable parent (default: a plain
/// in-memory volume).
struct TestArchive {
    path: PathBuf,
}

impl TestArchive {
    fn from_entries(entries: &[FixtureFile]) -> Self {
        Self::from_bytes(build_zip(entries))
    }

    fn from_bytes(bytes: Vec<u8>) -> Self {
        let path = std::env::temp_dir().join(format!("cmdr-archive-vol-{}.zip", uuid::Uuid::new_v4()));
        std::fs::write(&path, bytes).expect("write fixture zip");
        Self { path }
    }

    fn volume(&self) -> ArchiveVolume {
        self.volume_with_parent(Arc::new(InMemoryVolume::new("parent")))
    }

    fn volume_with_parent(&self, parent: Arc<dyn Volume>) -> ArchiveVolume {
        ArchiveVolume::new(parent, self.path.clone())
    }
}

impl Drop for TestArchive {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn names(entries: &[FileEntry]) -> Vec<String> {
    entries.iter().map(|e| e.name.clone()).collect()
}

async fn read_all(volume: &ArchiveVolume, path: &Path) -> Result<Vec<u8>, VolumeError> {
    let mut stream = volume.open_read_stream(path).await?;
    drain(stream.as_mut()).await
}

async fn drain(stream: &mut dyn VolumeReadStream) -> Result<Vec<u8>, VolumeError> {
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk?);
    }
    Ok(out)
}

// ---- Tier 1: browse -------------------------------------------------------

#[tokio::test]
async fn lists_root_nested_and_synthetic_directories() {
    // No explicit `dir/` entry: `dir/` and `dir/sub/` must be synthesized.
    let archive = TestArchive::from_entries(&[
        stored("a.txt", "hello"),
        deflated("dir/b.txt", "world"),
        deflated("dir/sub/c.txt", "deep"),
    ]);
    let volume = archive.volume();

    // Root: directories first, then files.
    let root = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(names(&root), vec!["dir", "a.txt"]);

    let nested = volume.list_directory(Path::new("dir"), None).await.unwrap();
    assert_eq!(names(&nested), vec!["sub", "b.txt"]);
    assert_eq!(
        names(&volume.list_directory(Path::new("dir/sub"), None).await.unwrap()),
        vec!["c.txt"]
    );

    // The synthetic `dir/` node is a directory with no timestamp.
    let synthetic = &nested.iter().find(|e| e.name == "sub").unwrap();
    assert!(synthetic.is_directory);
    assert_eq!(synthetic.modified_at, None);
}

#[tokio::test]
async fn metadata_maps_size_name_and_transparent_path() {
    let archive = TestArchive::from_entries(&[deflated("dir/file.txt", "abcde")]);
    let volume = archive.volume();

    let meta = volume.get_metadata(Path::new("dir/file.txt")).await.unwrap();
    assert_eq!(meta.name, "file.txt");
    assert_eq!(meta.size, Some(5));
    assert!(!meta.is_directory);
    // The full path renders as `<archive>/inner`, so the path bar reads
    // `/…/foo.zip/dir/file.txt` for free.
    assert_eq!(meta.path, archive.path.join("dir/file.txt").to_string_lossy());
}

#[tokio::test]
async fn metadata_on_root_reports_the_archive_itself() {
    let archive = TestArchive::from_entries(&[stored("a.txt", "x")]);
    let volume = archive.volume();

    let root = volume.get_metadata(Path::new("")).await.unwrap();
    assert!(root.is_directory);
    assert_eq!(root.name, volume.name());
    assert_eq!(root.path, archive.path.to_string_lossy());
}

#[tokio::test]
async fn exists_and_is_directory_round_trip() {
    let archive = TestArchive::from_entries(&[deflated("dir/b.txt", "world")]);
    let volume = archive.volume();

    assert!(volume.exists(Path::new("dir")).await);
    assert!(volume.exists(Path::new("dir/b.txt")).await);
    assert!(!volume.exists(Path::new("dir/missing")).await);

    assert!(volume.is_directory(Path::new("dir")).await.unwrap());
    assert!(!volume.is_directory(Path::new("dir/b.txt")).await.unwrap());
    assert!(matches!(
        volume.is_directory(Path::new("nope")).await,
        Err(VolumeError::NotFound(_))
    ));
}

#[tokio::test]
async fn accepts_absolute_paths_carrying_the_archive_prefix() {
    // The frontend sends full absolute paths (`/…/foo.zip/dir`); the volume
    // must strip the archive-path prefix to reach the inner node.
    let archive = TestArchive::from_entries(&[deflated("dir/b.txt", "world")]);
    let volume = archive.volume();

    let absolute = archive.path.join("dir");
    assert_eq!(
        names(&volume.list_directory(&absolute, None).await.unwrap()),
        vec!["b.txt"]
    );
}

#[tokio::test]
async fn list_directory_reports_one_cumulative_progress_tick() {
    let archive = TestArchive::from_entries(&[stored("a.txt", "hi"), stored("b.txt", "there"), dir("sub/")]);
    let volume = archive.volume();

    let seen = std::sync::Mutex::new(Vec::<ListingProgress>::new());
    let callback = |p: ListingProgress| seen.lock_ignore_poison().push(p);
    volume.list_directory(Path::new(""), Some(&callback)).await.unwrap();

    let seen = seen
        .into_inner()
        .expect("progress mutex is never poisoned in this test");
    assert_eq!(seen.len(), 1, "the atomic archive listing reports once");
    assert_eq!(
        seen[0],
        ListingProgress {
            files: 2,
            dirs: 1,
            bytes: 7
        }
    );
}

#[tokio::test]
async fn cancelable_listing_returns_the_full_listing() {
    // The archive listing is atomic (parse once from the cached index), so the
    // volume inherits the trait default: the cancel flag is accepted and the
    // full listing is returned, exactly like the local and in-memory backends.
    let archive = TestArchive::from_entries(&[stored("a.txt", "x"), deflated("dir/b.txt", "y")]);
    let volume = archive.volume();

    let cancel = Arc::new(AtomicBool::new(false));
    let entries = volume
        .list_directory_with_cancel(Path::new(""), None, Some(&cancel))
        .await
        .unwrap();
    assert_eq!(names(&entries), vec!["dir", "a.txt"]);
}

// ---- Tier 2: extract-out (streaming reads) --------------------------------

#[tokio::test]
async fn open_read_stream_decompresses_an_entry_end_to_end() {
    // ~300 KiB of compressible data exercises multi-chunk decompression.
    let content: Vec<u8> = (0..300_000).map(|i| (i % 251) as u8).collect();
    let archive = TestArchive::from_entries(&[deflated("big.bin", content.clone())]);
    let volume = archive.volume();

    let mut stream = volume.open_read_stream(Path::new("big.bin")).await.unwrap();
    assert_eq!(stream.total_size(), content.len() as u64);
    let data = drain(stream.as_mut()).await.unwrap();
    assert_eq!(data, content);
    assert_eq!(stream.bytes_read(), content.len() as u64);
}

#[tokio::test]
async fn open_read_stream_at_offset_yields_the_tail() {
    let content: Vec<u8> = (0..100_000).map(|i| (i % 97) as u8).collect();
    let archive = TestArchive::from_entries(&[deflated("data.bin", content.clone())]);
    let volume = archive.volume();

    let offset = 40_000u64;
    let mut stream = volume
        .open_read_stream_at_offset(Path::new("data.bin"), offset)
        .await
        .unwrap();
    // `total_size` stays the full file; `bytes_read` counts only this segment.
    assert_eq!(stream.total_size(), content.len() as u64);
    let data = drain(stream.as_mut()).await.unwrap();
    assert_eq!(data, content[offset as usize..]);
    assert_eq!(stream.bytes_read(), content.len() as u64 - offset);
}

#[tokio::test]
async fn concurrent_reads_on_two_entries_are_independent() {
    let a: Vec<u8> = (0..200_000).map(|i| (i % 7) as u8).collect();
    let b: Vec<u8> = (0..200_000).map(|i| (i % 13) as u8).collect();
    let archive = TestArchive::from_entries(&[deflated("a.bin", a.clone()), deflated("b.bin", b.clone())]);
    let volume = archive.volume();

    let (ra, rb) = tokio::join!(
        read_all(&volume, Path::new("a.bin")),
        read_all(&volume, Path::new("b.bin"))
    );
    assert_eq!(ra.unwrap(), a);
    assert_eq!(rb.unwrap(), b);
}

// ---- scan_for_copy --------------------------------------------------------

#[tokio::test]
async fn scan_for_copy_counts_a_directory_subtree() {
    let archive = TestArchive::from_entries(&[
        stored("top/a.txt", "aa"),           // 2 bytes
        deflated("top/sub/b.txt", "bbbb"),   // 4 bytes
        deflated("top/sub/deep/c.txt", "c"), // 1 byte
        stored("elsewhere.txt", "ignored"),
    ]);
    let volume = archive.volume();

    let scan = volume.scan_for_copy(Path::new("top")).await.unwrap();
    assert_eq!(scan.file_count, 3);
    assert_eq!(scan.dir_count, 2, "sub/ and sub/deep/");
    assert_eq!(scan.total_bytes, 7);
    assert_eq!(scan.dedup_bytes, 7);
    assert!(scan.top_level_is_directory);
}

#[tokio::test]
async fn scan_for_copy_sizes_a_single_file() {
    let archive = TestArchive::from_entries(&[deflated("only.txt", "twelve bytes")]);
    let volume = archive.volume();

    let scan = volume.scan_for_copy(Path::new("only.txt")).await.unwrap();
    assert_eq!(scan.file_count, 1);
    assert_eq!(scan.dir_count, 0);
    assert_eq!(scan.total_bytes, 12);
    assert!(!scan.top_level_is_directory);
}

#[tokio::test]
async fn scan_for_copy_on_a_missing_path_is_not_found() {
    let archive = TestArchive::from_entries(&[stored("a.txt", "x")]);
    let volume = archive.volume();

    assert!(matches!(
        volume.scan_for_copy(Path::new("nope")).await,
        Err(VolumeError::NotFound(_))
    ));
}

// ---- Read-only: every mutation is unsupported -----------------------------

#[tokio::test]
async fn every_mutation_is_unsupported() {
    let archive = TestArchive::from_entries(&[deflated("dir/b.txt", "world")]);
    let volume = archive.volume();

    assert!(matches!(
        volume.create_file(Path::new("new.txt"), b"x").await,
        Err(VolumeError::NotSupported)
    ));
    assert!(matches!(
        volume.create_directory(Path::new("new")).await,
        Err(VolumeError::NotSupported)
    ));
    // Even for an ALREADY-EXISTING dir, `create_directory_all` must refuse
    // rather than no-op to `Ok(())` (the trait default would).
    assert!(matches!(
        volume.create_directory_all(Path::new("dir")).await,
        Err(VolumeError::NotSupported)
    ));
    assert!(matches!(
        volume.delete(Path::new("dir/b.txt")).await,
        Err(VolumeError::NotSupported)
    ));
    assert!(matches!(
        volume
            .rename(Path::new("dir/b.txt"), Path::new("dir/c.txt"), false)
            .await,
        Err(VolumeError::NotSupported)
    ));

    // `write_from_stream` needs a source stream; any one does.
    let mem = InMemoryVolume::new("m");
    mem.create_file(Path::new("/src"), b"hi").await.unwrap();
    let source = mem.open_read_stream(Path::new("/src")).await.unwrap();
    let result = volume
        .write_from_stream(Path::new("dest"), 2, source, &|_, _| ControlFlow::Continue(()))
        .await;
    assert!(matches!(result, Err(VolumeError::NotSupported)));
}

// ---- Typed errors surfaced through the Volume API -------------------------

#[tokio::test]
async fn browsing_works_but_extracting_an_encrypted_entry_is_refused() {
    let mut bytes = build_zip(&[deflated("secret.txt", "classified")]);
    set_first_entry_encrypted(&mut bytes);
    let archive = TestArchive::from_bytes(bytes);
    let volume = archive.volume();

    // Names live in the central directory, so browsing still lists it.
    assert_eq!(
        names(&volume.list_directory(Path::new(""), None).await.unwrap()),
        vec!["secret.txt"]
    );
    // Extraction of the encrypted entry maps to a typed `NotSupported`.
    assert!(matches!(
        volume.open_read_stream(Path::new("secret.txt")).await,
        Err(VolumeError::NotSupported)
    ));
}

#[tokio::test]
async fn a_corrupt_archive_lists_as_a_typed_io_error() {
    let mut bytes = build_zip(&[stored("a.txt", "hi")]);
    overstate_record_count(&mut bytes);
    let archive = TestArchive::from_bytes(bytes);
    let volume = archive.volume();

    assert!(matches!(
        volume.list_directory(Path::new(""), None).await,
        Err(VolumeError::IoError { .. })
    ));
}

#[tokio::test]
async fn a_non_zip_file_is_unsupported() {
    let archive = TestArchive::from_bytes(b"this is not a zip file at all".to_vec());
    let volume = archive.volume();

    assert!(matches!(
        volume.list_directory(Path::new(""), None).await,
        Err(VolumeError::NotSupported)
    ));
}

// ---- Capability flags and the parent seam ---------------------------------

#[tokio::test]
async fn lane_key_is_the_parents_lane_key() {
    let archive = TestArchive::from_entries(&[stored("a.txt", "x")]);
    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("device").with_lane_key("shared-device"));
    let volume = archive.volume_with_parent(Arc::clone(&parent));

    // Never the archive path: archive work shares the device's serialization lane.
    assert_eq!(volume.lane_key(), parent.lane_key());
    assert_ne!(volume.lane_key().as_str(), archive.path.to_string_lossy().as_ref());
}

#[tokio::test]
async fn get_space_info_delegates_to_the_parent() {
    let archive = TestArchive::from_entries(&[stored("a.txt", "x")]);
    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("drive").with_space_info(1_000, 400));
    let volume = archive.volume_with_parent(parent);

    let space = volume.get_space_info().await.unwrap();
    // The parent's numbers verbatim, and crucially available > 0 so the
    // pre-copy space check never reads the archive as "disk full".
    assert_eq!(space.total_bytes, 1_000);
    assert_eq!(space.available_bytes, 400);
    assert_eq!(space.used_bytes, 600);
    assert!(space.available_bytes > 0);
}

#[test]
fn capability_flags_are_read_only_and_virtual() {
    let archive = TestArchive::from_entries(&[stored("a.txt", "x")]);
    let volume = archive.volume();

    assert_eq!(volume.name(), archive.path.file_name().unwrap().to_string_lossy());
    assert_eq!(volume.root(), archive.path);
    assert!(volume.supports_export());
    assert!(volume.supports_streaming());
    assert_eq!(volume.max_concurrent_ops(), 1);
    assert_eq!(volume.local_path(), None);
    assert!(!volume.supports_local_fs_access());
    assert_eq!(volume.space_poll_interval(), None);
    // No live watcher yet (M3): never claim listing freshness.
    assert!(!volume.listing_is_watched(Path::new("")));
}
