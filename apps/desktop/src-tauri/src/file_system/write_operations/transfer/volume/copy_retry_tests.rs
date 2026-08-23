//! What a per-file retry (`retry.rs`) must not disturb, driven through the real
//! `copy_volumes_with_progress` pipeline.
//!
//! The retry itself is pinned in `volume/strategy_retry_tests.rs`. These two
//! cover the operation-level invariants a retry sits inside, because they are the
//! ones whose failure mode is silent: a merge that quietly loses a dest-only
//! file, and a transfer that asks the user the same question twice.

use super::tests::make_state;
use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    DirectoryCreation, InMemoryVolume, ListingProgress, ScanConflict, SourceItemInfo, SpaceInfo, Volume,
    VolumeReadStream,
};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::ConflictResolution;
use std::pin::Pin as StdPin;

use super::super::super::conflict_responder_test_support::{ConflictResponderSink, file_conflict_count};

/// A destination that fails the first `fail_writes` writes of ONE named file and
/// otherwise behaves exactly like the `InMemoryVolume` it wraps.
///
/// Wrapping (rather than owning) the inner volume is what lets a test set up a
/// pre-existing dest tree and then read it back through the same handle, which is
/// how the merge invariant is checked. The file is named by the prefix of its
/// final name, because a staged write targets `<name>.cmdr-tmp-<uuid>`.
struct FlakyMergeDest {
    inner: Arc<InMemoryVolume>,
    fail_writes: usize,
    error: VolumeError,
    name: String,
    calls: AtomicUsize,
}

impl FlakyMergeDest {
    fn wrap(inner: Arc<InMemoryVolume>, fail_writes: usize, error: VolumeError, name: &str) -> Arc<Self> {
        Arc::new(Self {
            inner,
            fail_writes,
            error,
            name: name.to_owned(),
            calls: AtomicUsize::new(0),
        })
    }

    /// How many writes of the named file were attempted. A test asserts on this
    /// so it can't pass by never having triggered the blip at all.
    fn write_calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Volume for FlakyMergeDest {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_concurrent_ops(&self) -> usize {
        self.inner.max_concurrent_ops()
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> StdPin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> StdPin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_directory(path)
    }
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        self.inner.create_directory_all(path)
    }
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_file(path, content)
    }
    fn delete<'a>(&'a self, path: &'a Path) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.rename(from, to, force)
    }
    fn get_space_info<'a>(&'a self) -> StdPin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        self.inner.create_directory_errors_on_existing_dir()
    }
    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_conflicts(source_items, dest_path)
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> StdPin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let eligible = dest
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(self.name.as_str()));
        let attempt = if eligible {
            self.calls.fetch_add(1, Ordering::SeqCst)
        } else {
            usize::MAX
        };
        Box::pin(async move {
            if attempt >= self.fail_writes {
                return self.inner.write_from_stream(dest, size, stream, on_progress).await;
            }
            // Leave the partial behind, like a backend that never reached its own
            // cleanup: the staging layer is what has to clear it.
            let mut partial = Vec::new();
            if let Some(Ok(chunk)) = stream.next_chunk().await {
                partial.extend_from_slice(&chunk);
            }
            let _ = self.inner.delete(dest).await;
            let _ = self.inner.create_file(dest, &partial).await;
            Err(self.error.clone())
        })
    }
}

/// A source `/album` and a pre-existing dest `/album` that share one clashing
/// file and each keep one of their own.
async fn merge_fixture() -> (Arc<dyn Volume>, Arc<InMemoryVolume>) {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_directory(Path::new("/album")).await.unwrap();
    source
        .create_file(Path::new("/album/fresh.txt"), b"SRC-fresh")
        .await
        .unwrap();
    source
        .create_file(Path::new("/album/clash.txt"), b"SRC-clash")
        .await
        .unwrap();

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_directory(Path::new("/album")).await.unwrap();
    dest_inner
        .create_file(Path::new("/album/keep.txt"), b"DEST-keep")
        .await
        .unwrap();
    dest_inner
        .create_file(Path::new("/album/clash.txt"), b"DEST-clash")
        .await
        .unwrap();

    (source as Arc<dyn Volume>, dest_inner)
}

async fn read_all(vol: &Arc<InMemoryVolume>, path: &str) -> Option<Vec<u8>> {
    let mut stream = vol.open_read_stream(Path::new(path)).await.ok()?;
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    Some(out)
}

/// The merge invariant across an attempt boundary: a merge never deletes or
/// overwrites a dest file the source doesn't shadow, and a retry must not turn
/// one into a replace. The retried child re-runs its write; nothing about the
/// LEVEL is re-walked, re-created, or re-deleted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retried_merge_child_still_leaves_dest_only_files_alone() {
    let (source, dest_inner) = merge_fixture().await;
    let state = make_state();
    let op = TestOperationGuard::register_state("retry-merge", Arc::clone(&state));
    // Only the source-only file blips, and only once.
    let flaky = FlakyMergeDest::wrap(
        Arc::clone(&dest_inner),
        1,
        VolumeError::ConnectionTimeout("send timed out".into()),
        "fresh.txt",
    );
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    copy_volumes_with_progress(
        events,
        op.id(),
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await
    .expect("a blip on one merge child must not fail the merge");

    assert_eq!(
        read_all(&dest_inner, "/album/keep.txt").await.as_deref(),
        Some(&b"DEST-keep"[..]),
        "the dest-only file must survive a retried merge untouched"
    );
    assert_eq!(
        flaky.write_calls(),
        2,
        "the fixture must actually have blipped, or this test proves nothing"
    );
    assert_eq!(
        read_all(&dest_inner, "/album/fresh.txt").await.as_deref(),
        Some(&b"SRC-fresh"[..]),
        "the retried child must arrive whole"
    );
    assert_eq!(
        read_all(&dest_inner, "/album/clash.txt").await.as_deref(),
        Some(&b"SRC-clash"[..]),
        "the shadowed file follows the policy, exactly as without the blip"
    );
}

/// A retry must not ask the user again. Conflict resolution runs on the driver,
/// ABOVE the retry loop, so a file that is run again re-runs only its write —
/// carrying the decision the user already made, and never re-emitting the prompt.
///
/// The other half of the same decision: the answer is not stale either. It was
/// given for this file, in this operation, seconds ago; re-asking would be the
/// surprising behavior, not the safe one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retried_file_never_re_asks_the_conflict_the_user_already_answered() {
    let (source, dest_inner) = merge_fixture().await;
    let state = make_state();
    let op = TestOperationGuard::register_state("retry-no-reprompt", Arc::clone(&state));
    // The CLASHING file blips, so the retry happens after its prompt was answered.
    let flaky = FlakyMergeDest::wrap(
        Arc::clone(&dest_inner),
        1,
        VolumeError::ConnectionTimeout("send timed out".into()),
        "clash.txt",
    );
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    copy_volumes_with_progress(
        events.clone(),
        op.id(),
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await
    .expect("a blip on the clashing child must not fail the merge");

    assert_eq!(
        file_conflict_count(&events.inner),
        1,
        "the one clash must be asked about exactly once, however many attempts its write took"
    );
    assert_eq!(
        flaky.write_calls(),
        2,
        "the fixture must actually have blipped, or this test proves nothing"
    );
    assert_eq!(
        read_all(&dest_inner, "/album/clash.txt").await.as_deref(),
        Some(&b"SRC-clash"[..]),
        "the answered Overwrite must still be what lands"
    );
    assert_eq!(
        read_all(&dest_inner, "/album/keep.txt").await.as_deref(),
        Some(&b"DEST-keep"[..]),
        "and the dest-only file is still not the source's to touch"
    );
}
