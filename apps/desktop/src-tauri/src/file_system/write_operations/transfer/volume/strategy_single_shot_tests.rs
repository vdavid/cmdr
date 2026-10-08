//! A write the destination lands in ONE shot needs no `.cmdr-tmp-*` staging.
//!
//! Staging exists so a killed transfer can't leave a byte-incomplete file at the
//! user's real filename (`volume/copy_staged_write_tests.rs`). A write that goes
//! out as a single indivisible frame has no such moment, so it skips the staging
//! and the extra rename round trip that lands it.
//!
//! These tests pin BOTH directions: the destination's answer is what decides,
//! and a `false` answer (too big, or a backend with no such guarantee) still
//! stages. ❌ The condition must never become "the file is small".

use super::test_support::make_state;
use super::*;
use crate::file_system::write_operations::transfer::transfer_driver::LeafProgressLedger;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::WriteMode;
use crate::file_system::volume::{
    InMemoryVolume, ListingProgress, StreamLength, StreamWriteProgress, Volume, VolumeError, VolumeReadStream,
};
use crate::ignore_poison::IgnorePoison;

/// A destination that reports single-shot writes up to `limit` and records every
/// path it was asked to write to (and every rename), so a test can tell a write
/// at the final name from a staged one without reading any bytes back.
struct SingleShotDest {
    /// `Some(n)`: writes of 1..=n bytes are single-shot. `None`: the volume makes
    /// no such promise (the trait default), so everything must stage.
    limit: Option<u64>,
    /// What `publishes_writes_whole` answers: an object store's promise, for
    /// every size, with no single-shot one beside it.
    whole: bool,
    writes: Arc<StdMutex<Vec<(PathBuf, WriteMode)>>>,
    renames: Arc<StdMutex<Vec<(PathBuf, PathBuf)>>>,
}

/// A `SingleShotDest` plus handles on what it recorded.
struct Fixture {
    dest: Arc<dyn Volume>,
    writes: Arc<StdMutex<Vec<(PathBuf, WriteMode)>>>,
    renames: Arc<StdMutex<Vec<(PathBuf, PathBuf)>>>,
}

impl SingleShotDest {
    fn fixture(limit: Option<u64>, whole: bool) -> Fixture {
        let writes = Arc::new(StdMutex::new(Vec::new()));
        let renames = Arc::new(StdMutex::new(Vec::new()));
        let dest: Arc<dyn Volume> = Arc::new(Self {
            limit,
            whole,
            writes: Arc::clone(&writes),
            renames: Arc::clone(&renames),
        });
        Fixture { dest, writes, renames }
    }
}

impl Volume for SingleShotDest {
    fn name(&self) -> &str {
        "single-shot-dest"
    }
    fn root(&self) -> &Path {
        Path::new("/")
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }
    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(false) })
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        _force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let renames = Arc::clone(&self.renames);
        let pair = (from.to_path_buf(), to.to_path_buf());
        Box::pin(async move {
            renames.lock_ignore_poison().push(pair);
            Ok(())
        })
    }
    fn write_is_single_shot<'a>(&'a self, length: StreamLength) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let answer = length
            .known()
            .is_some_and(|size| self.limit.is_some_and(|limit| size > 0 && size <= limit));
        Box::pin(async move { answer })
    }
    fn publishes_writes_whole(&self) -> bool {
        self.whole
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let Some(size) = length.known() else {
            return Box::pin(async { Err(VolumeError::NotSupported) });
        };
        let writes = Arc::clone(&self.writes);
        let recorded = (dest.to_path_buf(), mode);
        Box::pin(async move {
            writes.lock_ignore_poison().push(recorded);
            let mut written = 0u64;
            while let Some(chunk) = stream.next_chunk().await {
                written += chunk?.len() as u64;
                if on_progress(StreamWriteProgress {
                    bytes_written: written,
                    expected_length: StreamLength::Known(size),
                })
                .is_break()
                {
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                }
            }
            Ok(written)
        })
    }
}

/// Copies one in-memory source file of `size` bytes into a `SingleShotDest`,
/// returning the paths it was asked to write to and the renames it saw.
async fn copy_one(
    size: usize,
    limit: Option<u64>,
    staging: WriteStaging,
    dest_path: &Path,
) -> (Vec<PathBuf>, Vec<(PathBuf, PathBuf)>, Arc<WriteOperationState>) {
    let (writes, renames, state) = copy_one_to(size, limit, false, staging, dest_path).await;
    (writes.into_iter().map(|(path, _)| path).collect(), renames, state)
}

/// [`copy_one`], into a destination that may also publish whole, keeping the
/// mode each write was asked for.
async fn copy_one_to(
    size: usize,
    limit: Option<u64>,
    whole: bool,
    staging: WriteStaging,
    dest_path: &Path,
) -> (
    Vec<(PathBuf, WriteMode)>,
    Vec<(PathBuf, PathBuf)>,
    Arc<WriteOperationState>,
) {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("source"));
    source
        .create_file(Path::new("/notes.txt"), &vec![0xAB; size])
        .await
        .unwrap();
    let Fixture { dest, writes, renames } = SingleShotDest::fixture(limit, whole);
    let state = make_state();

    copy_single_path(
        &source,
        Path::new("/notes.txt"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        dest_path,
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        staging,
    )
    .await
    .expect("the copy should succeed");

    let writes = writes.lock_ignore_poison().clone();
    let renames = renames.lock_ignore_poison().clone();
    (writes, renames, state)
}

fn is_temp(path: &Path) -> bool {
    path.to_string_lossy().contains(".cmdr-tmp-")
}

/// The exemption: a destination that lands this write in one shot gets the bytes
/// at the file's FINAL name, with no temp and no landing rename. That rename is
/// a whole extra round trip per file on SMB, which roughly doubles the wire cost
/// of a file the compound fast path would otherwise finish in one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_shot_write_skips_staging_entirely() {
    let (writes, renames, state) = copy_one(100, Some(1024), WriteStaging::Stage, Path::new("/notes.txt")).await;

    assert_eq!(
        writes,
        vec![PathBuf::from("/notes.txt")],
        "a single-shot write must go straight to the final name; got {writes:?}"
    );
    assert!(
        renames.is_empty(),
        "nothing was staged, so nothing needs landing; got {renames:?}"
    );
    assert!(
        state.in_flight_temps.lock_ignore_poison().is_empty(),
        "an unstaged write owns no partial to track"
    );
}

/// The other direction: too big for the destination's one-shot guarantee, so the
/// bytes land on a `.cmdr-tmp-*` and take the final name only after the last one
/// — the invariant the 2026-07-31 wedge cost us.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_the_destination_cant_do_in_one_shot_still_stages() {
    let (writes, renames, _state) = copy_one(4096, Some(1024), WriteStaging::Stage, Path::new("/notes.txt")).await;

    assert_eq!(writes.len(), 1, "one write attempt; got {writes:?}");
    assert!(
        is_temp(&writes[0]),
        "an oversized write must stream into a .cmdr-tmp-* sibling; got {writes:?}"
    );
    assert_eq!(
        renames,
        vec![(writes[0].clone(), PathBuf::from("/notes.txt"))],
        "the temp takes the final name only after the last byte; got {renames:?}"
    );
}

/// A backend that makes no single-shot promise (the trait default: MTP, local FS,
/// archives, in-memory) stages every write, however small the file is. Smallness
/// is NOT what buys the exemption.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tiny_write_to_a_backend_without_the_guarantee_still_stages() {
    let (writes, renames, _state) = copy_one(100, None, WriteStaging::Stage, Path::new("/notes.txt")).await;

    assert_eq!(writes.len(), 1, "one write attempt; got {writes:?}");
    assert!(
        is_temp(&writes[0]),
        "a backend with no one-shot guarantee must stage even a 100-byte file; got {writes:?}"
    );
    assert_eq!(
        renames,
        vec![(writes[0].clone(), PathBuf::from("/notes.txt"))],
        "the staged temp still has to be landed; got {renames:?}"
    );
}

/// A caller-staged write (the conflict layer's safe-replace temp) is passed
/// through untouched even when the destination could do it in one shot: the
/// caller keeps the ORIGINAL in place until the temp is complete, and landing it
/// is the caller's job.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_caller_staged_write_is_never_turned_into_a_single_shot() {
    let caller_temp = Path::new("/notes.txt.cmdr-tmp-abc");
    let (writes, renames, _state) = copy_one(100, Some(1024), WriteStaging::AlreadyStaged, caller_temp).await;

    assert_eq!(
        writes,
        vec![caller_temp.to_path_buf()],
        "the caller's temp is the write target; got {writes:?}"
    );
    assert!(renames.is_empty(), "the caller lands its own temp; got {renames:?}");
}

/// An object store publishes every write whole (S3's PUT and multipart
/// completion), so a fresh file goes straight to its final name at ANY size: a
/// staged landing there would be a server-side copy plus a delete. With no
/// landing to refuse a name someone took mid-upload, the write itself carries
/// `CreateNew`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_whole_publishing_destination_takes_a_fresh_file_at_its_final_name() {
    let (writes, renames, state) = copy_one_to(4096, None, true, WriteStaging::Stage, Path::new("/notes.txt")).await;

    assert_eq!(
        writes,
        vec![(PathBuf::from("/notes.txt"), WriteMode::CreateNew)],
        "a whole-publish write goes to the final name and refuses a taken one; got {writes:?}"
    );
    assert!(
        renames.is_empty(),
        "nothing was staged, so nothing lands; got {renames:?}"
    );
    assert!(
        state.in_flight_temps.lock_ignore_poison().is_empty(),
        "an unstaged write owns no partial to track"
    );
}

/// A name the caller claimed is the caller's to write over, so it goes out as
/// `CreateOrReplace`, still at the final name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_whole_publishing_destination_writes_a_claimed_name_over_in_place() {
    let (writes, renames, _state) = copy_one_to(
        4096,
        None,
        true,
        WriteStaging::StageOntoClaimedName,
        Path::new("/notes.txt"),
    )
    .await;

    assert_eq!(
        writes,
        vec![(PathBuf::from("/notes.txt"), WriteMode::CreateOrReplace)],
        "got {writes:?}"
    );
    assert!(renames.is_empty(), "got {renames:?}");
}
