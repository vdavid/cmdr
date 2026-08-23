//! How the concurrent driver answers "is this top-level name already taken?".
//!
//! Three ways, cheapest first: not at all for a destination directory THIS
//! operation created; from the one destination listing Phase 0.6 already pays
//! for; and, when neither settles it, from a per-file `get_metadata` probe. That
//! probe is one round trip per source, serialized on the driver, and against a
//! QNAP at 3.7 ms RTT it measured **2.378 s of a 3.224 s best run for 500
//! files** (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`).
//!
//! Everything here exists to keep those boundaries honest, because the failure
//! mode of getting one wrong is silent: a conflict that would have prompted
//! becomes an overwrite. A name lookup in a listing is NOT the same question a
//! `get_metadata` asks, so the destination volume in this file behaves like the
//! real ones do and resolves names case- and normalization-insensitively, the
//! way SMB shares and macOS volumes do. Under an exact-match name map, every
//! test that seeds a differently-spelled destination file goes green while the
//! user's data is gone.
//!
//! ❌ "The destination is empty" is NOT "the destination is one we made" and
//! must never be substituted for it — see
//! `an_empty_but_preexisting_destination_is_never_assumed_free`.

use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    CopyScanResult, DirectoryCreation, InMemoryVolume, ListingProgress, SpaceInfo, VolumeReadStream,
};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::ConflictResolution;
use crate::ignore_poison::IgnorePoison;
use std::ops::ControlFlow;
use unicode_normalization::UnicodeNormalization;

/// How a case- and normalization-insensitive backend compares two names.
///
/// Deliberately written here rather than reused from the production index: a
/// test that resolves names with the same helper the code under test uses can
/// only prove the helper agrees with itself.
fn backend_fold(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

/// A destination volume shaped like the ones this change is for: every
/// operation is a round trip (`operations_are_local()` is the default `false`),
/// and names resolve case- and normalization-insensitively, so
/// `get_metadata("doc.txt")` finds a stored `Doc.TXT` and a write to
/// `caf\u{e9}.txt` (NFC) lands on a stored `cafe\u{301}.txt` (NFD).
///
/// It records what the driver asked of it: per-file `get_metadata` probes and
/// `list_directory` calls, so a test can assert on round trips rather than on
/// wall-clock (which an in-memory volume can't show).
struct RecordingDest {
    inner: Arc<InMemoryVolume>,
    probed: std::sync::Mutex<Vec<PathBuf>>,
    listed: std::sync::Mutex<Vec<PathBuf>>,
    /// When set, the driver-visible `list_directory` fails with this error. The
    /// fake's own internal name resolution keeps working, modelling a transient
    /// transport failure rather than a broken share.
    listing_failure: Option<&'static str>,
    /// A destination whose operations cost microseconds, not a round trip.
    operations_are_local: bool,
}

impl RecordingDest {
    fn new() -> Arc<Self> {
        Self::configured(None, false)
    }

    fn with_failing_listing() -> Arc<Self> {
        Self::configured(Some("the share dropped the listing"), false)
    }

    fn local() -> Arc<Self> {
        Self::configured(None, true)
    }

    fn configured(listing_failure: Option<&'static str>, operations_are_local: bool) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(InMemoryVolume::new("dest").with_space_info(10_000_000, 10_000_000)),
            probed: std::sync::Mutex::new(Vec::new()),
            listed: std::sync::Mutex::new(Vec::new()),
            listing_failure,
            operations_are_local,
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

    fn listings_of(&self, dir: &str) -> usize {
        self.listed
            .lock_ignore_poison()
            .iter()
            .filter(|p| p.as_path() == Path::new(dir))
            .count()
    }

    /// Maps a requested path onto the name actually stored, the way a
    /// case-insensitive share does. An unmatched name is returned unchanged
    /// (that's a create).
    async fn resolve(&self, path: &Path) -> PathBuf {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return path.to_path_buf();
        };
        let name = name.to_string_lossy();
        let Ok(entries) = self.inner.list_directory(parent, None).await else {
            return path.to_path_buf();
        };
        let folded = backend_fold(&name);
        for entry in entries {
            if entry.name != name && backend_fold(&entry.name) == folded {
                return parent.join(&entry.name);
            }
        }
        path.to_path_buf()
    }

    async fn read(&self, path: &str) -> Option<Vec<u8>> {
        let mut stream = self.inner.open_read_stream(Path::new(path)).await.ok()?;
        let mut buf = Vec::new();
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            buf.extend_from_slice(&chunk);
        }
        Some(buf)
    }

    /// The names stored directly under `dir`, sorted. Used to prove a copy
    /// didn't quietly add a second spelling of a name that was already taken.
    async fn names_under(&self, dir: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .inner
            .list_directory(Path::new(dir), None)
            .await
            .expect("listing the destination")
            .into_iter()
            .map(|e| e.name)
            .collect();
        names.sort();
        names
    }
}

impl Volume for RecordingDest {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn operations_are_local(&self) -> bool {
        self.operations_are_local
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
        self.listed.lock_ignore_poison().push(path.to_path_buf());
        if let Some(message) = self.listing_failure {
            return Box::pin(async move {
                Err(VolumeError::IoError {
                    message: message.to_string(),
                    raw_os_error: None,
                })
            });
        }
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.probed.lock_ignore_poison().push(path.to_path_buf());
        Box::pin(async move {
            let real = self.resolve(path).await;
            self.inner.get_metadata(&real).await
        })
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let real = self.resolve(path).await;
            self.inner.exists(&real).await
        })
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let real = self.resolve(path).await;
            self.inner.is_directory(&real).await
        })
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
        Box::pin(async move {
            let real = self.resolve(path).await;
            self.inner.create_file(&real, content).await
        })
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let real = self.resolve(path).await;
            self.inner.delete(&real).await
        })
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let real_from = self.resolve(from).await;
            let real_to = self.resolve(to).await;
            self.inner.rename(&real_from, &real_to, force).await
        })
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
        Box::pin(async move {
            let real = self.resolve(path).await;
            self.inner.open_read_stream(&real).await
        })
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
        Box::pin(async move {
            let real = self.resolve(dest).await;
            self.inner.write_from_stream(&real, size, stream, on_progress).await
        })
    }
}

/// Five sources, enough to take the concurrent path (`>= 3` and a window above
/// 1), each with distinguishable content.
async fn five_sources() -> (Arc<InMemoryVolume>, Vec<PathBuf>) {
    sources_named(&["doc-0.txt", "doc-1.txt", "doc-2.txt", "doc-3.txt", "doc-4.txt"]).await
}

async fn sources_named(names: &[&str]) -> (Arc<InMemoryVolume>, Vec<PathBuf>) {
    let source = Arc::new(InMemoryVolume::new("source").with_space_info(10_000_000, 10_000_000));
    let mut paths = Vec::new();
    for name in names {
        let path = format!("/{name}");
        source
            .create_file(Path::new(&path), format!("source {name}").as_bytes())
            .await
            .expect("seed a source file");
        paths.push(PathBuf::from(path));
    }
    (source, paths)
}

async fn run_copy(
    source: &Arc<InMemoryVolume>,
    paths: &[PathBuf],
    dest: &Arc<RecordingDest>,
    dest_dir: &str,
    conflict_resolution: ConflictResolution,
) -> Result<(), WriteFailure> {
    let guard = TestOperationGuard::register("listing-precheck");
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

/// The win: a merge into the user's existing folder answers every top-level
/// conflict question from the one listing Phase 0.6 already pays for, instead
/// of one round trip per source.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_answers_every_conflict_from_one_listing() {
    let (source, paths) = five_sources().await;
    let dest = RecordingDest::new();
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

    assert_eq!(
        dest.probes_under("/inbox"),
        Vec::<String>::new(),
        "the destination listing already answers which names are taken; a probe per source \
         is a round trip spent re-asking it"
    );
    assert_eq!(
        dest.listings_of("/inbox"),
        1,
        "and it must stay ONE listing — the stale-temp reap's, reused, not a second one"
    );
    // The conflict was seen and honored: the user's file is untouched, and the
    // four non-colliding sources landed.
    assert_eq!(
        dest.read("/inbox/doc-2.txt").await,
        Some(b"the user's own copy".to_vec()),
        "Skip must leave the pre-existing file exactly as it was"
    );
    for index in [0, 1, 3, 4] {
        assert_eq!(
            dest.read(&format!("/inbox/doc-{index}.txt")).await,
            Some(format!("source doc-{index}.txt").into_bytes()),
        );
    }
}

/// ❌ Trap 1: SMB shares and macOS volumes resolve names case-INsensitively, so
/// `get_metadata("doc-2.txt")` finds a stored `Doc-2.TXT`. An exact-match name
/// map does not, and the miss is silent: a file the user would have been
/// prompted about gets overwritten.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_destination_name_differing_only_in_case_is_still_a_conflict() {
    let (source, paths) = five_sources().await;
    let dest = RecordingDest::new();
    dest.inner
        .create_directory(Path::new("/inbox"))
        .await
        .expect("seed the pre-existing destination");
    dest.inner
        .create_file(Path::new("/inbox/DOC-2.TXT"), b"the user's own copy")
        .await
        .expect("seed the colliding file, spelled differently");

    run_copy(&source, &paths, &dest, "/inbox", ConflictResolution::Skip)
        .await
        .expect("a Skip-policy merge completes");

    assert_eq!(
        dest.read("/inbox/DOC-2.TXT").await,
        Some(b"the user's own copy".to_vec()),
        "the share would have resolved `doc-2.txt` onto this file, so it is a conflict and \
         Skip must leave it alone"
    );
    assert_eq!(
        dest.names_under("/inbox").await,
        vec![
            "DOC-2.TXT".to_string(),
            "doc-0.txt".to_string(),
            "doc-1.txt".to_string(),
            "doc-3.txt".to_string(),
            "doc-4.txt".to_string(),
        ],
        "and no second spelling of the taken name may appear"
    );
    assert_eq!(
        dest.probes_under("/inbox"),
        vec!["/inbox/doc-2.txt".to_string()],
        "only the one name the listing couldn't answer exactly falls back to a probe"
    );
}

/// ❌ Trap 2: macOS and SMB move paths between NFC and NFD, so the same
/// user-visible name can be two different byte strings. A byte-exact map key
/// misses an entry that differs only by normalization; the backend would not.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_destination_name_differing_only_in_unicode_normalization_is_still_a_conflict() {
    // NFD: `e` + COMBINING ACUTE ACCENT. The destination stores NFC.
    let (source, paths) = sources_named(&["a.txt", "b.txt", "cafe\u{301}.txt"]).await;
    let dest = RecordingDest::new();
    dest.inner
        .create_directory(Path::new("/inbox"))
        .await
        .expect("seed the pre-existing destination");
    dest.inner
        .create_file(Path::new("/inbox/caf\u{e9}.txt"), b"the user's own copy")
        .await
        .expect("seed the colliding file, composed differently");

    run_copy(&source, &paths, &dest, "/inbox", ConflictResolution::Skip)
        .await
        .expect("a Skip-policy merge completes");

    assert_eq!(
        dest.read("/inbox/caf\u{e9}.txt").await,
        Some(b"the user's own copy".to_vec()),
        "NFC and NFD spell the same name; Skip must leave the user's file alone"
    );
    assert_eq!(
        dest.names_under("/inbox").await,
        vec!["a.txt".to_string(), "b.txt".to_string(), "caf\u{e9}.txt".to_string()],
        "and no second, decomposed spelling of the taken name may appear"
    );
}

/// ❌ Trap 3: fail safe, never fast. A listing that errors says nothing about
/// what is at the destination, so every source must fall back to its own probe
/// rather than be treated as conflict-free.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_listing_falls_back_to_per_file_probes() {
    let (source, paths) = five_sources().await;
    let dest = RecordingDest::with_failing_listing();
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
        "an unanswered listing is not an answer of `nothing is there`"
    );
    assert_eq!(
        dest.read("/inbox/doc-2.txt").await,
        Some(b"the user's own copy".to_vec()),
        "and the conflict is still caught"
    );
}

/// The existing created-directory skip short-circuits before any listing work:
/// nothing the user had can be inside a folder that didn't exist a moment ago,
/// so there is nothing to index and nothing to probe.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_destination_directory_this_operation_created_needs_neither_probe_nor_index() {
    let (source, paths) = five_sources().await;
    let dest = RecordingDest::new();

    run_copy(&source, &paths, &dest, "/backup/2026-08-02", ConflictResolution::Stop)
        .await
        .expect("a copy into a fresh destination has nothing to conflict with");

    assert_eq!(
        dest.probes_under("/backup/2026-08-02"),
        Vec::<String>::new(),
        "a folder this operation created can hold nothing to conflict with"
    );
    for index in 0..5 {
        assert_eq!(
            dest.read(&format!("/backup/2026-08-02/doc-{index}.txt")).await,
            Some(format!("source doc-{index}.txt").into_bytes()),
        );
    }
}

/// ❌ "Created by us" is not "happens to be empty". A pre-existing empty
/// directory can gain an entry from another process between any two instants;
/// one we just created cannot have held anything BEFORE we made it. Only the
/// second claim licenses skipping the question entirely.
///
/// The discriminator is what happens when the listing can't be taken: an empty
/// pre-existing destination falls back to a probe per source, where one this
/// operation created asks nothing at all. (With a working listing both answer
/// "free" without a probe, which is why that isn't the test.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_empty_but_preexisting_destination_is_never_assumed_free() {
    let (source, paths) = five_sources().await;
    let dest = RecordingDest::with_failing_listing();
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

    // The same failing listing, into a directory this operation creates.
    let (source, paths) = five_sources().await;
    let fresh = RecordingDest::with_failing_listing();
    run_copy(&source, &paths, &fresh, "/backup/2026-08-02", ConflictResolution::Stop)
        .await
        .expect("a copy into a fresh destination has nothing to conflict with");
    assert_eq!(
        fresh.probes_under("/backup/2026-08-02").len(),
        0,
        "and a directory we DID make needs no fallback either"
    );
}

/// The volume ROOT always pre-exists, so a copy into it is a merge — the shape
/// most of the suite uses, and the one a user gets copying into the other
/// pane's current folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_into_the_volume_root_is_a_merge() {
    let (source, paths) = five_sources().await;
    let dest = RecordingDest::new();
    dest.inner
        .create_file(Path::new("/doc-2.txt"), b"the user's own copy")
        .await
        .expect("seed the colliding file at the root");

    run_copy(&source, &paths, &dest, "/", ConflictResolution::Skip)
        .await
        .expect("a Skip-policy merge completes");

    assert_eq!(
        dest.probes_under("/"),
        Vec::<String>::new(),
        "the root listing answers for the root"
    );
    assert_eq!(
        dest.read("/doc-2.txt").await,
        Some(b"the user's own copy".to_vec()),
        "the volume root is nobody's fresh directory, so its conflicts are real"
    );
}

/// A LOCAL destination keeps exactly the behavior it had. `get_metadata` there
/// is a microsecond `stat`, so folding every name in a folder that might hold
/// 200k entries to copy a handful of files into it is the worse trade.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_local_destination_keeps_its_per_file_probes() {
    let (source, paths) = five_sources().await;
    let dest = RecordingDest::local();
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

    assert_eq!(
        dest.probes_under("/inbox").len(),
        5,
        "a local stat costs less than the index that would replace it"
    );
    assert_eq!(
        dest.read("/inbox/doc-2.txt").await,
        Some(b"the user's own copy".to_vec()),
        "and the conflict is caught the same way it always was"
    );
}

/// A directory source landing on a same-named destination directory still
/// merges (dir-vs-dir is never a conflict), and the destination-only file the
/// user already had inside it survives: the merge invariant, now that the
/// top-level answer comes from a listing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_directory_source_still_merges_and_keeps_destination_only_files() {
    let source = Arc::new(InMemoryVolume::new("source").with_space_info(10_000_000, 10_000_000));
    for name in ["/loose-a.txt", "/loose-b.txt"] {
        source
            .create_file(Path::new(name), b"loose")
            .await
            .expect("seed a loose source file");
    }
    source
        .create_directory(Path::new("/photos"))
        .await
        .expect("seed the source directory");
    source
        .create_file(Path::new("/photos/new.jpg"), b"new photo")
        .await
        .expect("seed a file inside the source directory");
    let paths = vec![
        PathBuf::from("/loose-a.txt"),
        PathBuf::from("/loose-b.txt"),
        PathBuf::from("/photos"),
    ];

    let dest = RecordingDest::new();
    dest.inner
        .create_directory(Path::new("/inbox"))
        .await
        .expect("seed the pre-existing destination");
    dest.inner
        .create_directory(Path::new("/inbox/photos"))
        .await
        .expect("seed the same-named destination directory");
    dest.inner
        .create_file(Path::new("/inbox/photos/old.jpg"), b"the user's own photo")
        .await
        .expect("seed a destination-only file inside it");

    run_copy(&source, &paths, &dest, "/inbox", ConflictResolution::Stop)
        .await
        .expect("dir-vs-dir merges without prompting");

    assert_eq!(
        dest.read("/inbox/photos/old.jpg").await,
        Some(b"the user's own photo".to_vec()),
        "a merge never removes a destination file the source doesn't shadow"
    );
    assert_eq!(dest.read("/inbox/photos/new.jpg").await, Some(b"new photo".to_vec()));
}
