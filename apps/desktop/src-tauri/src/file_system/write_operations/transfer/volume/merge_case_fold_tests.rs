//! A deep merge child whose name only FOLDS onto a destination name.
//!
//! `Report.docx` arriving where the user keeps `report.docx`, or an NFD name
//! arriving where the destination stores NFC: two byte strings, one file, on
//! every case-insensitive share and every macOS volume. The level listing can't
//! settle that, so `merge_level` asks `DestNameIndex` and pays one
//! `get_metadata` for what it can't answer. These cells pin the three answers —
//! a conflict on a case-insensitive destination, two files on a case-sensitive
//! one, and a failed item when the probe itself can't answer.
//!
//! The wrapper below models the backend (SMB, APFS): it resolves every path
//! through the folded name a listing reports, and refuses a `rename` onto a
//! name a case-folded sibling already holds. Merge semantics themselves are in
//! `merge_tests.rs`; the same-volume engine's twin is
//! `rename_merge_case_fold_tests.rs`.

use super::super::super::conflict_responder_test_support::{ConflictResponderSink, file_conflict_count};
use super::tests::make_state;
use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{CopyScanResult, InMemoryVolume, ListingProgress, SpaceInfo, VolumeReadStream};
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use unicode_normalization::UnicodeNormalization;

/// The key two names share on a backend that resolves case and normalization:
/// NFC plus lowercase, the same rule `dest_name_index::fold` states for the
/// transfer layer.
fn fold_key(name: &str) -> String {
    name.nfc().flat_map(char::to_lowercase).collect()
}

/// An `InMemoryVolume` that resolves every path through the folded name it
/// already holds, the way an SMB share or a case-insensitive APFS volume does.
///
/// `rename` refuses a target a case-folded sibling holds (`AlreadyExists`, what
/// `renamex_np(RENAME_EXCL)` and SMB's stat-first rename both answer), which is
/// the moment a merge that never resolved the clash would clear the user's file
/// out of the way.
struct CaseFoldingDest {
    inner: Arc<InMemoryVolume>,
    /// Every path this destination is asked to `get_metadata`, so a cell can
    /// pin what the fold-aware lookup COSTS: an ordinary tree must pay none
    /// INSIDE the merge (the top-level source keeps the driver's own pre-check).
    probes: Mutex<Vec<PathBuf>>,
    /// Makes a `get_metadata` of a name that only FOLDS onto a stored one fail
    /// with a transport error instead of answering. An unanswerable probe must
    /// fail its item, never read as "nothing is there".
    probe_is_unanswerable: bool,
}

impl CaseFoldingDest {
    fn new(inner: &Arc<InMemoryVolume>) -> Self {
        Self {
            inner: Arc::clone(inner),
            probes: Mutex::new(Vec::new()),
            probe_is_unanswerable: false,
        }
    }

    fn with_an_unanswerable_probe(inner: &Arc<InMemoryVolume>) -> Self {
        Self {
            inner: Arc::clone(inner),
            probes: Mutex::new(Vec::new()),
            probe_is_unanswerable: true,
        }
    }

    /// The path this backend really resolves `path` to: every component
    /// replaced by the entry that folds onto it, where one exists.
    fn fold<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = PathBuf> + Send + 'a>> {
        Box::pin(async move {
            let (Some(parent), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
                return path.to_path_buf();
            };
            let parent = self.fold(parent).await;
            let key = fold_key(name);
            if let Ok(entries) = self.inner.list_directory(&parent, None).await
                && let Some(hit) = entries.into_iter().find(|e| fold_key(&e.name) == key)
            {
                return parent.join(&hit.name);
            }
            parent.join(name)
        })
    }
}

impl Volume for CaseFoldingDest {
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
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.list_directory(&folded, on_progress).await
        })
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.probes.lock_ignore_poison().push(path.to_path_buf());
            let folded = self.fold(path).await;
            if self.probe_is_unanswerable && folded != *path {
                return Err(VolumeError::DeviceDisconnected(
                    "the share stopped answering".to_string(),
                ));
            }
            self.inner.get_metadata(&folded).await
        })
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.exists(&folded).await
        })
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.is_directory(&folded).await
        })
    }
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.create_file(&folded, content).await
        })
    }
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.create_directory(&folded).await
        })
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.delete(&folded).await
        })
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded_to = self.fold(to).await;
            if !force && folded_to != *to && self.inner.exists(&folded_to).await {
                return Err(VolumeError::AlreadyExists(to.display().to_string()));
            }
            let folded_from = self.fold(from).await;
            // The landing name resolves through the folded parents too, so a
            // file renamed into `Sub/` arrives in the `sub/` the backend has.
            self.inner.rename(&folded_from, &folded_to, force).await
        })
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(dest).await;
            self.inner.write_from_stream(&folded, size, stream, on_progress).await
        })
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let folded = self.fold(path).await;
            self.inner.open_read_stream(&folded).await
        })
    }
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        self.inner.create_directory_errors_on_existing_dir()
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }
}

/// A source volume holding `/album/<source_name>` and a destination holding
/// `/album/<dest_name>`, the two names differing only by fold.
async fn folded_pair(source_name: &str, dest_name: &str) -> (Arc<dyn Volume>, Arc<InMemoryVolume>) {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_directory(Path::new("/album")).await.unwrap();
    source
        .create_file(&Path::new("/album").join(source_name), b"SOURCE")
        .await
        .unwrap();

    let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(&Path::new("/album").join(dest_name), b"THE USER'S FILE")
        .await
        .unwrap();
    (source, dest)
}

/// Reads a file straight off the in-memory backing store, so an assertion sees
/// the bytes the backend really holds rather than what a folded lookup finds.
async fn stored_bytes(dest: &Arc<InMemoryVolume>, path: &str) -> Option<Vec<u8>> {
    let mut stream = dest.open_read_stream(Path::new(path)).await.ok()?;
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    Some(out)
}

/// Copies `/album` from `source` into `dest`'s root under `policy`, answering
/// nothing (no prompt is expected unless the caller scripts one).
async fn merge_album(
    label: &str,
    source: &Arc<dyn Volume>,
    dest: Arc<dyn Volume>,
    policy: ConflictResolution,
) -> (Arc<ConflictResponderSink>, Arc<WriteOperationState>) {
    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Skip, false));
    let config = VolumeCopyConfig {
        conflict_resolution: policy,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let result = copy_volumes_with_progress(
        events.clone(),
        label,
        &state,
        Arc::clone(source),
        &[PathBuf::from("/album")],
        dest,
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "{label}: the copy should finish, got {result:?}");
    (events, state)
}

/// A deep child differing only in CASE, landing on a case-insensitive
/// destination under Skip: the user's file must still hold its own bytes, and
/// the child must be reported as the skip it is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deep_child_differing_only_in_case_is_a_conflict_the_policy_decides() {
    let (source, dest_inner) = folded_pair("Report.docx", "report.docx").await;
    let dest: Arc<dyn Volume> = Arc::new(CaseFoldingDest::new(&dest_inner));

    let (_events, state) = merge_album("op-fold-case", &source, dest, ConflictResolution::Skip).await;

    assert_eq!(
        stored_bytes(&dest_inner, "/album/report.docx").await.as_deref(),
        Some(&b"THE USER'S FILE"[..]),
        "a Skip must leave the destination file the source only folds onto"
    );
    assert_eq!(
        stored_bytes(&dest_inner, "/album/Report.docx").await,
        None,
        "and must not land the source under the other spelling either"
    );
    assert_eq!(
        state.skipped_totals().0,
        1,
        "the child the policy declined has to be reported as skipped"
    );
}

/// The same clash spelled in Unicode: an NFD source name against an NFC
/// destination name is one file on macOS and SMB.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deep_child_differing_only_in_normalization_is_a_conflict_too() {
    // "café.txt" decomposed (e + U+0301) against the composed spelling.
    let (source, dest_inner) = folded_pair("cafe\u{301}.txt", "caf\u{e9}.txt").await;
    let dest: Arc<dyn Volume> = Arc::new(CaseFoldingDest::new(&dest_inner));

    let (_events, state) = merge_album("op-fold-nfd", &source, dest, ConflictResolution::Skip).await;

    assert_eq!(
        stored_bytes(&dest_inner, "/album/caf\u{e9}.txt").await.as_deref(),
        Some(&b"THE USER'S FILE"[..]),
        "a Skip must leave the destination file the NFD source only folds onto"
    );
    assert_eq!(
        stored_bytes(&dest_inner, "/album/cafe\u{301}.txt").await,
        None,
        "and must not land the source under the decomposed spelling"
    );
    assert_eq!(state.skipped_totals().0, 1);
}

/// A case-SENSITIVE destination legitimately holds both spellings, so the probe
/// answers `NotFound` and the child lands beside its neighbor with no prompt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_case_sensitive_destination_keeps_both_spellings_and_prompts_for_neither() {
    let (source, dest_inner) = folded_pair("Report.docx", "report.docx").await;
    let dest: Arc<dyn Volume> = Arc::clone(&dest_inner) as Arc<dyn Volume>;

    let (events, _state) = merge_album("op-fold-sensitive", &source, dest, ConflictResolution::Stop).await;

    assert_eq!(
        stored_bytes(&dest_inner, "/album/report.docx").await.as_deref(),
        Some(&b"THE USER'S FILE"[..]),
        "the destination's own file is untouched"
    );
    assert_eq!(
        stored_bytes(&dest_inner, "/album/Report.docx").await.as_deref(),
        Some(&b"SOURCE"[..]),
        "and the source's differently-cased name is a second, new file here"
    );
    assert_eq!(
        file_conflict_count(&events.inner),
        0,
        "two names a case-sensitive destination keeps apart are not a clash"
    );
}

/// A probe that can't answer fails its item. ❌ Never "nothing is there": that
/// reading writes over the file the probe was asked about.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unanswerable_probe_fails_the_item_instead_of_writing() {
    let (source, dest_inner) = folded_pair("Report.docx", "report.docx").await;
    let dest: Arc<dyn Volume> = Arc::new(CaseFoldingDest::with_an_unanswerable_probe(&dest_inner));

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Skip, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let outcome = copy_volumes_with_progress(
        events.clone(),
        "op-fold-unanswerable",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        dest,
        Path::new("/"),
        &config,
    )
    .await;

    assert_eq!(
        stored_bytes(&dest_inner, "/album/report.docx").await.as_deref(),
        Some(&b"THE USER'S FILE"[..]),
        "an unanswerable probe must not cost the user their file"
    );
    assert_eq!(
        stored_bytes(&dest_inner, "/album/Report.docx").await,
        None,
        "and nothing may be written at the name it couldn't answer for"
    );
    assert!(
        matches!(
            outcome,
            Err(WriteFailure {
                error: WriteOperationError::DeviceDisconnected { ref path },
            }) if path == "/album/Report.docx"
        ),
        "the item has to fail, naming the child whose destination couldn't be settled; got {outcome:?}"
    );
}

/// A DIRECTORY child differing only in case merges into the directory the
/// destination already has, rather than standing a second one beside it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_case_differing_directory_child_merges_into_the_one_that_is_there() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/Sub")).await.unwrap();
    source
        .create_file(Path::new("/album/Sub/fresh.txt"), b"SOURCE")
        .await
        .unwrap();

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_directory(Path::new("/album")).await.unwrap();
    dest_inner.create_directory(Path::new("/album/sub")).await.unwrap();
    dest_inner
        .create_file(Path::new("/album/sub/keep.txt"), b"THE USER'S FILE")
        .await
        .unwrap();
    let dest: Arc<dyn Volume> = Arc::new(CaseFoldingDest::new(&dest_inner));

    let (events, _state) = merge_album("op-fold-dir", &source, dest, ConflictResolution::Stop).await;

    assert_eq!(
        stored_bytes(&dest_inner, "/album/sub/keep.txt").await.as_deref(),
        Some(&b"THE USER'S FILE"[..]),
        "the dest-only file inside the folded directory survives"
    );
    assert_eq!(
        stored_bytes(&dest_inner, "/album/sub/fresh.txt").await.as_deref(),
        Some(&b"SOURCE"[..]),
        "and the source child lands inside the directory that was already there"
    );
    assert!(
        !dest_inner.exists(Path::new("/album/Sub")).await,
        "a second directory differing only in case must not appear"
    );
    assert_eq!(
        file_conflict_count(&events.inner),
        0,
        "a dir-vs-dir merge never prompts, folded or not"
    );
}

/// A destination that reports one name as absent and then refuses the landing
/// rename for it: what a file ARRIVING between the level listing and the write
/// looks like from here.
///
/// The listing can't answer for that file however carefully it folds, which is
/// why the landing carries the caller's own expectation
/// (`staged_write.rs::LandingName`) as well.
struct LateArrivalDest {
    inner: Arc<InMemoryVolume>,
    /// The path the listing pretends isn't there yet.
    arriving: PathBuf,
}

impl Volume for LateArrivalDest {
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
        Box::pin(async move {
            let entries = self.inner.list_directory(path, on_progress).await?;
            Ok(entries
                .into_iter()
                .filter(|e| Path::new(&e.path) != self.arriving)
                .collect())
        })
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if path == self.arriving {
                return Err(VolumeError::NotFound(path.display().to_string()));
            }
            self.inner.get_metadata(path).await
        })
    }
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_directory(path)
    }
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_file(path, content)
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.rename(from, to, force)
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.inner.write_from_stream(dest, size, stream, on_progress)
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        self.inner.create_directory_errors_on_existing_dir()
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }
}

/// The belt and braces: a name the level listing reported as FREE, taken by the
/// time the bytes come to claim it. Nothing resolved a conflict for it, so the
/// landing must leave it alone rather than clear the way.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_that_fills_up_after_the_listing_is_not_cleared_by_the_landing() {
    let (source, dest_inner) = folded_pair("notes.txt", "notes.txt").await;
    let dest: Arc<dyn Volume> = Arc::new(LateArrivalDest {
        inner: Arc::clone(&dest_inner),
        arriving: PathBuf::from("/album/notes.txt"),
    });

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Skip, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let outcome = copy_volumes_with_progress(
        events.clone(),
        "op-late-arrival",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        dest,
        Path::new("/"),
        &config,
    )
    .await;

    assert!(
        matches!(
            outcome,
            Err(WriteFailure {
                error: WriteOperationError::DestinationExists { .. },
            })
        ),
        "the clash has to be reported; got {outcome:?}"
    );
    assert_eq!(
        stored_bytes(&dest_inner, "/album/notes.txt").await.as_deref(),
        Some(&b"THE USER'S FILE"[..]),
        "a file that arrived after the listing is still the user's, and nothing answered for it"
    );
}

/// The cost side: an ordinary tree pays NO probe. A byte-exact hit and a name
/// nothing in the listing can fold onto are both settled in memory, so the
/// `get_metadata` only a fold-only name needs is never asked for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ordinary_merge_costs_no_probes() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    for path in ["/album/fresh.txt", "/album/clash.txt", "/album/sub/deep.txt"] {
        source.create_file(Path::new(path), b"SOURCE").await.unwrap();
    }

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_directory(Path::new("/album")).await.unwrap();
    dest_inner.create_directory(Path::new("/album/sub")).await.unwrap();
    dest_inner
        .create_file(Path::new("/album/clash.txt"), b"THE USER'S FILE")
        .await
        .unwrap();
    let counting = Arc::new(CaseFoldingDest::new(&dest_inner));
    let dest: Arc<dyn Volume> = Arc::clone(&counting) as Arc<dyn Volume>;

    let (_events, _state) = merge_album("op-fold-cost", &source, dest, ConflictResolution::Skip).await;

    // The one probe an ordinary copy still pays is the SERIAL driver's own
    // top-level pre-check for `/album`, which predates this and is documented
    // in `DETAILS.md` § "Answering the pre-check from one listing".
    let inside_the_merge: Vec<PathBuf> = counting
        .probes
        .lock_ignore_poison()
        .iter()
        .filter(|p| *p != Path::new("/album"))
        .cloned()
        .collect();
    assert!(
        inside_the_merge.is_empty(),
        "a merge of plain ASCII names must settle every child from the level listing, probed {inside_the_merge:?}"
    );
    assert_eq!(
        stored_bytes(&dest_inner, "/album/fresh.txt").await.as_deref(),
        Some(&b"SOURCE"[..]),
        "and it still copies what it should"
    );
}
