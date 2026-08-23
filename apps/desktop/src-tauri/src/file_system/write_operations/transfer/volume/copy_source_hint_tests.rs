//! What a volume copy does when the preflight produced NO per-source hint.
//!
//! Every other suite here hand-seeds a fully populated `per_path`, so none of
//! them exercise the shape production actually reaches for a LOCAL source: the
//! local `std::fs` scan preview completes with an EMPTY `per_path`, so
//! `preflight::scan_volume_sources` hands the drivers an empty hint map.
//!
//! A missing hint means UNKNOWN, never "file". Both drivers must resolve the
//! source's real type before they stream it AND before they decide whether the
//! destination path is a sweepable partial, because a directory source's dest
//! root can be a merged directory holding the user's own files.
//!
//! Shared fixtures `make_state` / `make_volumes` live in `volume/copy_tests.rs`
//! (`super::tests`).

use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{CopyScanResult, InMemoryVolume, ListingProgress, SpaceInfo, VolumeReadStream};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::scan_cache::seed_incoherent_scan_result_for_test;
use crate::file_system::write_operations::types::{ConflictResolution, WriteConflictEvent};
use crate::ignore_poison::IgnorePoison;

/// Seeds the scan-preview cache with a COMPLETED preview that carries no
/// per-path data. `insert_scan_result`'s canary rejects this shape, but the
/// canary is a `debug_assert!`: in a release build the entry lands anyway and
/// the drivers still have to handle it without lying about what a source is.
/// These fixtures are that defense's proof, so they seed past the canary.
fn seed_preview_without_per_path(preview_id: &str, sources: &[&str], file_count: usize, total_bytes: u64) {
    seed_incoherent_scan_result_for_test(
        preview_id.to_string(),
        sources.iter().map(PathBuf::from).collect(),
        file_count,
        total_bytes,
    );
}

/// An `InMemoryVolume` that refuses to open one named path for reading. Lets a
/// DIRECTORY source fail partway through its subtree stream, which is the only
/// way to reach the post-loop partial sweep with a directory in play.
struct PoisonedChildVolume {
    inner: Arc<InMemoryVolume>,
    poisoned_file: String,
}

impl Volume for PoisonedChildVolume {
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
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }
    fn supports_export(&self) -> bool {
        true
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_concurrent_ops(&self) -> usize {
        32
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let name = self.poisoned_file.clone();
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            if path.to_string_lossy() == name {
                return Err(VolumeError::IoError {
                    message: "Injected read failure".into(),
                    raw_os_error: Some(5), // EIO
                });
            }
            inner.open_read_stream(path).await
        })
    }
}

/// Builds a source volume holding `/album` with one unreadable child, plus a
/// destination that already has an `/album` of its own carrying a sentinel the
/// operation must never touch.
///
/// `extra_source_files` land next to `/album` at the source root: the concurrent
/// driver only engages from three top-level sources up, and the double doesn't
/// forward writes, so they have to be seeded here.
async fn poisoned_dir_source_and_merged_dest(extra_source_files: &[&str]) -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    let inner_source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    inner_source.create_directory(Path::new("/album")).await.unwrap();
    inner_source
        .create_file(Path::new("/album/poison.bin"), &vec![0xAB; 4096])
        .await
        .unwrap();
    for name in extra_source_files {
        inner_source.create_file(Path::new(name), b"aaaa").await.unwrap();
    }
    let source: Arc<dyn Volume> = Arc::new(PoisonedChildVolume {
        inner: inner_source,
        poisoned_file: "/album/poison.bin".to_string(),
    });

    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/sentinel.txt"), b"precious user data")
        .await
        .unwrap();

    (source, dest)
}

/// Reads a destination file back, so the assertion checks CONTENT survival, not
/// just a name.
async fn read_dest_file(dest: &Arc<dyn Volume>, path: &str) -> Option<Vec<u8>> {
    let mut stream = dest.open_read_stream(Path::new(path)).await.ok()?;
    let mut collected = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        collected.extend_from_slice(&chunk);
    }
    Some(collected)
}

/// SERIAL driver: a completed preview with an empty `per_path` must still copy a
/// DIRECTORY source's whole subtree.
///
/// Pre-fix this streamed the directory as a file and died on the destination's
/// "can't read a directory" error, which is exactly what a local folder → NAS
/// copy hit in production.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn directory_source_copies_when_preview_carries_no_per_path_serial() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_file(Path::new("/album/one.bin"), b"first").await.unwrap();
    source.create_directory(Path::new("/album/inner")).await.unwrap();
    source
        .create_file(Path::new("/album/inner/two.bin"), b"second")
        .await
        .unwrap();

    let preview_id = "test-preview-no-per-path-serial";
    seed_preview_without_per_path(preview_id, &["/album"], 2, 11);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        preview_id: Some(preview_id.to_string()),
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-no-per-path-serial",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "directory copy should succeed, got {:?}", result);
    assert_eq!(
        read_dest_file(&dest, "/album/one.bin").await.as_deref(),
        Some(&b"first"[..])
    );
    assert_eq!(
        read_dest_file(&dest, "/album/inner/two.bin").await.as_deref(),
        Some(&b"second"[..])
    );
}

/// CONCURRENT driver (3+ sources, `max_concurrent_ops() > 1`): same contract.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn directory_source_copies_when_preview_carries_no_per_path_concurrent() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_file(Path::new("/album/one.bin"), b"first").await.unwrap();
    source.create_directory(Path::new("/album/inner")).await.unwrap();
    source
        .create_file(Path::new("/album/inner/two.bin"), b"second")
        .await
        .unwrap();
    source.create_file(Path::new("/loose_a.bin"), b"aaaa").await.unwrap();
    source.create_file(Path::new("/loose_b.bin"), b"bbbb").await.unwrap();

    let preview_id = "test-preview-no-per-path-concurrent";
    seed_preview_without_per_path(preview_id, &["/album", "/loose_a.bin", "/loose_b.bin"], 4, 19);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        preview_id: Some(preview_id.to_string()),
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-no-per-path-concurrent",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/album"),
            PathBuf::from("/loose_a.bin"),
            PathBuf::from("/loose_b.bin"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "directory copy should succeed, got {:?}", result);
    assert_eq!(
        read_dest_file(&dest, "/album/one.bin").await.as_deref(),
        Some(&b"first"[..])
    );
    assert_eq!(
        read_dest_file(&dest, "/album/inner/two.bin").await.as_deref(),
        Some(&b"second"[..])
    );
    assert_eq!(
        read_dest_file(&dest, "/loose_a.bin").await.as_deref(),
        Some(&b"aaaa"[..])
    );
    assert_eq!(
        read_dest_file(&dest, "/loose_b.bin").await.as_deref(),
        Some(&b"bbbb"[..])
    );
}

/// SERIAL driver, the data-safety one: a directory copy that FAILS must leave
/// the pre-existing destination directory (and everything in it the operation
/// never wrote) alone.
///
/// Dir-vs-dir merges silently, so the destination `/album` here is the user's
/// own folder. The post-loop partial sweep runs an unconditional recursive
/// delete on whatever the driver reported as the in-flight partial; only a
/// correctly-resolved "this source is a directory" keeps the merged root out of
/// that list.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_directory_copy_spares_preexisting_dest_dir_serial() {
    let (source, dest) = poisoned_dir_source_and_merged_dest(&[]).await;

    let preview_id = "test-preview-no-per-path-fail-serial";
    seed_preview_without_per_path(preview_id, &["/album"], 1, 4096);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        // Dir-vs-dir merges under any policy; Overwrite keeps the run
        // prompt-free.
        conflict_resolution: ConflictResolution::Overwrite,
        preview_id: Some(preview_id.to_string()),
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-no-per-path-fail-serial",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "the poisoned child must fail the copy");
    assert_eq!(
        read_dest_file(&dest, "/album/sentinel.txt").await.as_deref(),
        Some(&b"precious user data"[..]),
        "a failed directory copy must never delete the destination directory it merged into"
    );
}

/// CONCURRENT driver: same data-safety contract, via `in_flight_partials`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_directory_copy_spares_preexisting_dest_dir_concurrent() {
    let (source, dest) = poisoned_dir_source_and_merged_dest(&["/loose_a.bin", "/loose_b.bin"]).await;

    let preview_id = "test-preview-no-per-path-fail-concurrent";
    seed_preview_without_per_path(preview_id, &["/album", "/loose_a.bin", "/loose_b.bin"], 3, 4104);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        preview_id: Some(preview_id.to_string()),
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-no-per-path-fail-concurrent",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/album"),
            PathBuf::from("/loose_a.bin"),
            PathBuf::from("/loose_b.bin"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "the poisoned child must fail the copy");
    assert_eq!(
        read_dest_file(&dest, "/album/sentinel.txt").await.as_deref(),
        Some(&b"precious user data"[..]),
        "a failed directory copy must never delete the destination directory it merged into"
    );
    // No prompt: dir-vs-dir is never a conflict, so nothing here waited on a
    // human.
    let conflicts: Vec<WriteConflictEvent> = events.conflicts.lock_ignore_poison().clone();
    assert!(conflicts.is_empty(), "dir-vs-dir must not prompt: {:?}", conflicts);
}
