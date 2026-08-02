//! When the concurrent driver may skip its per-file destination pre-check.
//!
//! Before spawning each top-level source, the concurrent driver awaits
//! `dest_volume.get_metadata(dest_item_path)`: `Ok` means something is already
//! there, so conflict resolution runs. That probe is one round trip per file,
//! serialized on the driver, and against a QNAP at 3.7 ms RTT it measured
//! **2.378 s of a 3.224 s best run — 74%** for 500 files
//! (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`).
//!
//! It can be skipped in exactly one situation: the destination directory is one
//! THIS operation created, so nothing the user already had can be inside it.
//! Everything else in this file exists to keep that boundary honest, because
//! the failure mode of getting it wrong is silent: a conflict that would have
//! prompted becomes an overwrite.
//!
//! ❌ "The destination is empty" is NOT the same claim and must not be
//! substituted for it — see `an_empty_but_preexisting_destination_is_still_probed`.

use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    CopyScanResult, DirectoryCreation, InMemoryVolume, ListingProgress, SpaceInfo, VolumeReadStream,
};
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{CollectorEventSink, ConflictResolution};
use crate::ignore_poison::IgnorePoison;
use std::ops::ControlFlow;

/// An `InMemoryVolume` destination that records every path the driver probes
/// with `get_metadata`, so a test can assert on the round trips rather than on
/// wall-clock (which an in-memory volume can't show).
struct ProbeRecordingDest {
    inner: Arc<InMemoryVolume>,
    probed: std::sync::Mutex<Vec<PathBuf>>,
}

impl ProbeRecordingDest {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(InMemoryVolume::new("dest").with_space_info(10_000_000, 10_000_000)),
            probed: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Probes of direct children of `dir` — the top-level pre-check, and
    /// nothing else the copy might stat along the way.
    fn probes_under(&self, dir: &str) -> Vec<String> {
        self.probed
            .lock_ignore_poison()
            .iter()
            .filter(|p| p.parent() == Some(Path::new(dir)))
            .map(|p| p.display().to_string())
            .collect()
    }

    async fn read(&self, path: &str) -> Option<Vec<u8>> {
        let mut stream = self.inner.open_read_stream(Path::new(path)).await.ok()?;
        let mut buf = Vec::new();
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            buf.extend_from_slice(&chunk);
        }
        Some(buf)
    }
}

impl Volume for ProbeRecordingDest {
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
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.probed.lock_ignore_poison().push(path.to_path_buf());
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
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_directory(path)
    }
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        self.inner.create_directory_all(path)
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
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        self.inner.create_directory_errors_on_existing_dir()
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.inner.write_from_stream(dest, size, stream, on_progress)
    }
}

/// Five sources, enough to take the concurrent path (`>= 3` and a window above
/// 1), each with distinguishable content.
async fn five_sources() -> (Arc<InMemoryVolume>, Vec<PathBuf>) {
    let source = Arc::new(InMemoryVolume::new("source").with_space_info(10_000_000, 10_000_000));
    let mut paths = Vec::new();
    for index in 0..5 {
        let name = format!("/doc-{index}.txt");
        source
            .create_file(Path::new(&name), format!("source {index}").as_bytes())
            .await
            .expect("seed a source file");
        paths.push(PathBuf::from(name));
    }
    (source, paths)
}

async fn run_copy(
    source: &Arc<InMemoryVolume>,
    paths: &[PathBuf],
    dest: &Arc<ProbeRecordingDest>,
    dest_dir: &str,
    conflict_resolution: ConflictResolution,
) -> Result<(), WriteFailure> {
    // Unique id, drop-unregisters: a literal id shared by four concurrently
    // running tests is exactly the global-state collision `TestOperationGuard`
    // exists to prevent (`write_operations/CLAUDE.md` § Test isolation).
    let guard = TestOperationGuard::register("precheck");
    let state = Arc::clone(guard.state());
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution,
        ..VolumeCopyConfig::default()
    };
    copy_volumes_with_progress(
        events,
        guard.id(),
        &state,
        Arc::clone(source) as Arc<dyn Volume>,
        paths,
        Arc::clone(dest) as Arc<dyn Volume>,
        Path::new(dest_dir),
        &config,
    )
    .await
}

/// The win. A copy into a folder that didn't exist a moment ago creates it, and
/// nothing can already be inside a folder this operation just made — so every
/// per-file probe is a guaranteed miss and the driver shouldn't pay for it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_destination_directory_this_operation_created_is_never_probed() {
    let (source, paths) = five_sources().await;
    let dest = ProbeRecordingDest::new();

    run_copy(&source, &paths, &dest, "/backup/2026-08-02", ConflictResolution::Stop)
        .await
        .expect("a copy into a fresh destination has nothing to conflict with");

    assert_eq!(
        dest.probes_under("/backup/2026-08-02"),
        Vec::<String>::new(),
        "the destination directory was created by this very operation, so every \
         per-file conflict probe is a round trip spent proving nothing"
    );
    // And the copy still actually happened.
    for index in 0..5 {
        assert_eq!(
            dest.read(&format!("/backup/2026-08-02/doc-{index}.txt")).await,
            Some(format!("source {index}").into_bytes()),
        );
    }
}

/// ❌ The data-safety case, and the reason this isn't a blanket optimization. A
/// copy MERGING into the user's existing folder must keep probing every source:
/// that is the only place a real conflict can be.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_into_a_preexisting_directory_still_probes_every_source() {
    let (source, paths) = five_sources().await;
    let dest = ProbeRecordingDest::new();
    // The user's folder, with one file that collides with a source name.
    dest.inner
        .create_directory(Path::new("/inbox"))
        .await
        .expect("seed the pre-existing destination");
    dest.inner
        .create_file(Path::new("/inbox/doc-2.txt"), b"the user's own copy")
        .await
        .expect("seed the colliding file");

    run_copy(&source, &paths, &dest, "/inbox", ConflictResolution::Skip)
        .await
        .expect("a Skip-policy merge completes");

    let mut probed = dest.probes_under("/inbox");
    probed.sort();
    assert_eq!(
        probed,
        vec![
            "/inbox/doc-0.txt",
            "/inbox/doc-1.txt",
            "/inbox/doc-2.txt",
            "/inbox/doc-3.txt",
            "/inbox/doc-4.txt",
        ],
        "every source landing in a pre-existing directory must still be checked"
    );
    // The conflict was seen and honored: the user's file is untouched.
    assert_eq!(
        dest.read("/inbox/doc-2.txt").await,
        Some(b"the user's own copy".to_vec()),
        "Skip must leave the pre-existing file exactly as it was"
    );
}

/// ❌ "Created by us" is not "happens to be empty". A directory that already
/// existed can gain an entry from another process between any two instants, so
/// an empty pre-existing destination keeps every probe. Nothing here should
/// ever be relaxed into an emptiness check.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_empty_but_preexisting_destination_is_still_probed() {
    let (source, paths) = five_sources().await;
    let dest = ProbeRecordingDest::new();
    dest.inner
        .create_directory(Path::new("/empty"))
        .await
        .expect("seed an empty pre-existing destination");

    run_copy(&source, &paths, &dest, "/empty", ConflictResolution::Stop)
        .await
        .expect("nothing is actually there, so the copy completes");

    assert_eq!(
        dest.probes_under("/empty").len(),
        5,
        "an empty directory somebody else made is not a directory we made"
    );
}

/// The volume ROOT always pre-exists, so a copy into it is a merge and keeps
/// every probe. This is the shape most of the existing suite uses, and the
/// shape a user gets when they copy into the other pane's current folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_into_the_volume_root_is_still_probed() {
    let (source, paths) = five_sources().await;
    let dest = ProbeRecordingDest::new();

    run_copy(&source, &paths, &dest, "/", ConflictResolution::Stop)
        .await
        .expect("an empty root takes the copy");

    assert_eq!(
        dest.probes_under("/").len(),
        5,
        "the volume root is nobody's fresh directory"
    );
}
