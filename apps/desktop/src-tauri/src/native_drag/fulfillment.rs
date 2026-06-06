//! Drag-out file-promise fulfillment: the plain-Rust service that actually
//! downloads a virtual file (MTP/SMB) to the exact destination Finder chose.
//!
//! This module has NO AppKit dependency. The delegate in [`super::promises`]
//! drives it from the promise operation-queue thread; everything here is plain
//! async Rust that unit tests exercise with no `NSFilePromiseProvider`, no
//! `block2`, and no main-thread runloop. That split is deliberate: all the real
//! logic (volume resolution, streaming, partial-file cleanup, error→friendly
//! mapping) lives here where it's testable; the Objective-C surface stays
//! paper-thin.
//!
//! ## Sequence (per the drag-out plan, M2)
//!
//! [`fulfill`] resolves the source volume from the registry, marks it busy for
//! the eject guard, notes the destination as a Cmdr-own write (so dropping a
//! phone photo into `~/Downloads` doesn't pop a spurious "Downloaded …" toast),
//! then streams the bytes:
//!
//! - **File**: `open_read_stream` → `write_from_stream(dest, …)` to the EXACT
//!   Finder-chosen leaf path. Finder uniquifies collisions ("sunset 2.jpg") and
//!   we honor that leaf, never the source basename.
//! - **Folder**: `create_dir` at the dest, then a recursive walk (list → mkdir →
//!   per-file `write_from_stream`). The cross-volume copy engine derives landed
//!   names from source basenames and can't be pointed at a Finder-renamed root,
//!   so the recursion is hand-rolled on the same per-file primitive.
//!
//! ## Cleanup contract (load-bearing)
//!
//! On ANY `Err`, the destination this fulfillment created is removed before the
//! error is returned. The "ANY `Err`" wording matters:
//! `LocalPosixVolume::write_from_stream` self-cleans its partial ONLY on the
//! cancel branch (`ControlFlow::Break`), NOT on a propagated source-read error
//! — and a source-read error (device unplugged mid-stream) is exactly the
//! promise failure mode. So a single-file fulfillment removes the partial dest
//! itself; a folder fulfillment removes the whole tree it created. The dest is
//! a fresh Finder-created path (Finder hands us a brand-new URL it just made),
//! so wholesale removal of what we created never touches pre-existing user
//! content.
//!
//! ## Main-thread invariant
//!
//! The service NEVER performs synchronous main-thread work. Volume I/O runs on
//! the tokio runtime; `note_pending_write_for_cmdr` is a cheap prefix-scoped
//! mutex (no main-thread hop, usually a no-op since Finder destinations are
//! rarely inside Downloads). The delegate calls `fulfill` from the promise
//! queue thread via `block_on`; if the service hopped synchronously to the main
//! thread there, and the main thread were itself busy or waiting, it would
//! deadlock. It doesn't, by construction — there is no `run_on_main_thread`
//! anywhere below.

use std::path::{Path, PathBuf};

use crate::file_system::volume::Volume;
use crate::file_system::volume::friendly_error::{FriendlyError, friendly_error_from_volume_error};
use crate::file_system::volume::{VolumeError, VolumeReadStream};

/// A drag-out fulfillment failure, carrying a fully-rendered [`FriendlyError`]
/// so the delegate can surface its text through the promise completion
/// handler's `NSError` (Finder shows its own alert).
#[derive(Debug, Clone)]
pub struct FulfillError {
    /// User-facing, rendered copy (title / explanation / suggestion / category).
    pub friendly: FriendlyError,
    /// Whether this was a user/system cancellation (app quit, device
    /// disconnect mid-stream surfaces as a read error, not this). The delegate
    /// maps a cancel to a Cancelled-shaped `NSError` so Finder doesn't shout.
    pub cancelled: bool,
}

impl FulfillError {
    /// Builds a `FulfillError` from a `VolumeError` and the destination path
    /// (used for provider-aware friendly copy). The path is the DEST so the
    /// friendly mapper can detect the destination provider; for a source-read
    /// error the dest is still the most useful path to show the user.
    fn from_volume_error(err: &VolumeError, dest: &Path) -> Self {
        let cancelled = matches!(err, VolumeError::Cancelled(_));
        Self {
            friendly: friendly_error_from_volume_error(err, dest),
            cancelled,
        }
    }
}

/// The volume side of a fulfillment, abstracted so unit tests can drive the
/// service without the global `VolumeManager`. Production resolves through
/// [`RegistryResolver`]; tests pass a fixed `InMemoryVolume`. `Send + Sync`
/// because the delegate drives `fulfill` from the promise queue thread.
pub trait VolumeResolver: Send + Sync {
    /// Returns the source volume for `volume_id`, or `None` if it's gone
    /// (unmounted / disconnected since the drag started).
    fn resolve(&self, volume_id: &str) -> Option<std::sync::Arc<dyn Volume>>;
}

/// Production resolver: the global `VolumeManager`.
pub struct RegistryResolver;

impl VolumeResolver for RegistryResolver {
    fn resolve(&self, volume_id: &str) -> Option<std::sync::Arc<dyn Volume>> {
        crate::file_system::get_volume_manager().get(volume_id)
    }
}

/// What a successful fulfillment produced, so the session-summary accounting can
/// split the completion toast by kind ("Copied 2 files and 1 folder.").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FulfillOutcome {
    /// Whether the dragged item was a directory (recursively downloaded) vs a
    /// single file.
    pub is_dir: bool,
}

/// Fulfills one dragged item: downloads `source_path` from the volume identified
/// by `source_volume_id` to the exact `dest_path` Finder supplied.
///
/// Marks the source volume busy for the eject guard for the whole transfer
/// (released on every exit via an RAII guard). Returns the resolved
/// [`FulfillOutcome`] (file vs folder) on success; on any failure removes the
/// partial/created destination and returns a [`FulfillError`] carrying friendly
/// copy.
pub async fn fulfill(
    source_volume_id: &str,
    source_path: &Path,
    dest_path: &Path,
) -> Result<FulfillOutcome, FulfillError> {
    fulfill_with_resolver(&RegistryResolver, source_volume_id, source_path, dest_path).await
}

/// RAII guard that marks a volume busy on construction and releases it on drop,
/// so the eject guard clears no matter how the fulfillment exits (success,
/// error, or a panic unwinding through the `.await`s).
struct BusyGuard {
    op_id: String,
}

impl BusyGuard {
    fn new(volume_id: &str) -> Self {
        let op_id = format!("drag-out-{}", uuid::Uuid::new_v4());
        crate::file_system::write_operations::register_external_volume_op(&op_id, vec![volume_id.to_string()]);
        Self { op_id }
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        crate::file_system::write_operations::release_external_volume_op(&self.op_id);
    }
}

/// Testable core of [`fulfill`] with an injectable volume resolver.
pub(crate) async fn fulfill_with_resolver(
    resolver: &dyn VolumeResolver,
    source_volume_id: &str,
    source_path: &Path,
    dest_path: &Path,
) -> Result<FulfillOutcome, FulfillError> {
    let Some(volume) = resolver.resolve(source_volume_id) else {
        // The source vanished between drag-start and fulfillment.
        let err = VolumeError::DeviceDisconnected(format!("Volume '{source_volume_id}' is no longer available"));
        return Err(FulfillError::from_volume_error(&err, dest_path));
    };

    // Busy-guard the source for the whole transfer (eject guard). Released on
    // every exit path below via Drop.
    let _busy = BusyGuard::new(source_volume_id);

    // Suppress the downloads watcher for this destination (a no-op unless dest
    // is inside ~/Downloads). Cheap mutex, never a main-thread hop.
    crate::downloads::note_pending_write_for_cmdr(dest_path);

    log::info!(
        target: "drag_out",
        "fulfilling promise: {} {} -> {}",
        source_volume_id,
        source_path.display(),
        dest_path.display()
    );

    let is_dir = volume
        .is_directory(source_path)
        .await
        .map_err(|e| FulfillError::from_volume_error(&e, dest_path))?;

    let result = if is_dir {
        fulfill_directory(volume.as_ref(), source_path, dest_path).await
    } else {
        fulfill_file(volume.as_ref(), source_path, dest_path).await
    };

    if let Err(ref err) = result {
        log::warn!(
            target: "drag_out",
            "promise fulfillment failed for {} -> {}: {}",
            source_path.display(),
            dest_path.display(),
            err.friendly.title
        );
    }
    result.map(|()| FulfillOutcome { is_dir })
}

/// Streams one source file to the EXACT `dest_path`. On any error, removes the
/// partial destination (the local writer leaves it on a read error — see the
/// module-level cleanup contract).
async fn fulfill_file(volume: &dyn Volume, source_path: &Path, dest_path: &Path) -> Result<(), FulfillError> {
    let result = stream_one_file(volume, source_path, dest_path).await;
    if result.is_err() {
        // Best-effort: remove whatever partial landed at the Finder-chosen path.
        let _ = remove_file_best_effort(dest_path).await;
    }
    result
}

/// The happy-path stream of one file: open the source reader, write it to the
/// destination, no cleanup (the caller owns cleanup on error).
async fn stream_one_file(volume: &dyn Volume, source_path: &Path, dest_path: &Path) -> Result<(), FulfillError> {
    let size_hint = volume.get_metadata(source_path).await.ok().and_then(|m| m.size);

    let stream: Box<dyn VolumeReadStream> = volume
        .open_read_stream(source_path)
        .await
        .map_err(|e| FulfillError::from_volume_error(&e, dest_path))?;
    let size = size_hint.unwrap_or_else(|| stream.total_size());

    // No cancel from this path in v1 (Finder owns the gesture, no progress UI);
    // always Continue. App-quit / device-disconnect aborts arrive as the source
    // stream dropping mid-flight or `next_chunk` erroring, handled by cleanup.
    let on_progress = &|_done: u64, _total: u64| std::ops::ControlFlow::<()>::Continue(());

    write_to_local_dest(dest_path, size, stream, on_progress)
        .await
        .map_err(|e| FulfillError::from_volume_error(&e, dest_path))?;
    Ok(())
}

/// Writes a source stream to a LOCAL destination path. The destination is
/// always a local FS path (Finder hands us a `file://` URL on the user's disk),
/// so we resolve through the local-FS write primitive directly rather than the
/// VolumeManager: the dest "volume" is whatever local disk Finder picked, and
/// `LocalPosixVolume` rooted at `/` writes to any absolute local path.
async fn write_to_local_dest(
    dest_path: &Path,
    size: u64,
    stream: Box<dyn VolumeReadStream>,
    on_progress: &(dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
) -> Result<u64, VolumeError> {
    let local = crate::file_system::volume::LocalPosixVolume::new("Local", PathBuf::from("/"));
    local.write_from_stream(dest_path, size, stream, on_progress).await
}

/// Recursively downloads a source directory into the Finder-created `dest_path`.
///
/// Creates the dest dir, then walks the source (list → mkdir per subdir →
/// per-file stream). On any error, removes the ENTIRE created tree
/// (`remove_dir_all`): the dest is a freshly Finder-created directory, so
/// wholesale removal of what we created can't touch pre-existing user content.
async fn fulfill_directory(volume: &dyn Volume, source_path: &Path, dest_path: &Path) -> Result<(), FulfillError> {
    let result = populate_directory(volume, source_path, dest_path).await;
    if result.is_err() {
        let _ = remove_dir_all_best_effort(dest_path).await;
    }
    result
}

/// The happy-path recursive populate. No cleanup (the caller removes the whole
/// created tree on error).
async fn populate_directory(volume: &dyn Volume, source_path: &Path, dest_path: &Path) -> Result<(), FulfillError> {
    create_local_dir(dest_path)
        .await
        .map_err(|e| FulfillError::from_volume_error(&e, dest_path))?;

    let entries = volume
        .list_directory(source_path, None)
        .await
        .map_err(|e| FulfillError::from_volume_error(&e, dest_path))?;

    for entry in entries {
        let child_source = source_path.join(&entry.name);
        let child_dest = dest_path.join(&entry.name);
        if entry.is_directory {
            // `Box::pin` because this is an async-recursive call into the same
            // function (Rust needs the future boxed to size it).
            Box::pin(populate_directory(volume, &child_source, &child_dest)).await?;
        } else {
            stream_one_file(volume, &child_source, &child_dest).await?;
        }
    }
    Ok(())
}

/// Creates a single local directory (the dest tree's leaf-by-leaf mkdir). Uses
/// `create_dir_all` so an intermediate level that an earlier recursion already
/// made doesn't error.
async fn create_local_dir(dest_path: &Path) -> Result<(), VolumeError> {
    let dest = dest_path.to_path_buf();
    tokio::task::spawn_blocking(move || std::fs::create_dir_all(&dest))
        .await
        .map_err(|e| VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: None,
        })?
        .map_err(VolumeError::from)
}

/// Best-effort removal of a partial destination file. Never fails the caller.
async fn remove_file_best_effort(dest_path: &Path) -> std::io::Result<()> {
    let dest = dest_path.to_path_buf();
    match tokio::task::spawn_blocking(move || std::fs::remove_file(&dest)).await {
        Ok(r) => r,
        Err(_) => Ok(()),
    }
}

/// Best-effort removal of the entire created destination tree. Safe because the
/// dest is a fresh Finder-created directory (no pre-existing user content).
async fn remove_dir_all_best_effort(dest_path: &Path) -> std::io::Result<()> {
    let dest = dest_path.to_path_buf();
    match tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&dest)).await {
        Ok(r) => r,
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::{InMemoryVolume, ListingProgress};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    /// A resolver that always returns the same volume (or `None`).
    struct FixedResolver(Option<Arc<dyn Volume>>);

    impl VolumeResolver for FixedResolver {
        fn resolve(&self, _volume_id: &str) -> Option<Arc<dyn Volume>> {
            self.0.clone()
        }
    }

    fn populated_volume() -> Arc<InMemoryVolume> {
        let v = InMemoryVolume::new("Phone");
        // create_file is async; block on it inline (tests run under tokio).
        Arc::new(v)
    }

    async fn add_file(v: &InMemoryVolume, path: &str, content: &[u8]) {
        v.create_file(Path::new(path), content).await.unwrap();
    }

    async fn add_dir(v: &InMemoryVolume, path: &str) {
        v.create_directory(Path::new(path)).await.unwrap();
    }

    // ---- Happy path: landed filename equals the Finder-chosen leaf EXACTLY ----

    #[tokio::test]
    async fn file_lands_under_the_exact_finder_chosen_leaf_not_source_basename() {
        let v = populated_volume();
        add_file(&v, "/DCIM/photo-001.jpg", b"sunset bytes").await;
        let dest_dir = tempfile::tempdir().unwrap();
        // Finder uniquified the collision: the leaf is "sunset 2.jpg", NOT the
        // source basename "photo-001.jpg". The landed file must match THIS.
        let dest = dest_dir.path().join("sunset 2.jpg");

        let resolver = FixedResolver(Some(v));
        let outcome = fulfill_with_resolver(&resolver, "phone", Path::new("/DCIM/photo-001.jpg"), &dest)
            .await
            .expect("fulfillment should succeed");
        assert!(!outcome.is_dir, "a single file fulfillment reports is_dir = false");

        assert!(dest.exists(), "file must land at the exact Finder leaf");
        assert_eq!(std::fs::read(&dest).unwrap(), b"sunset bytes");
        // The source basename must NOT appear at the dest dir.
        assert!(
            !dest_dir.path().join("photo-001.jpg").exists(),
            "must not land under the source basename"
        );
    }

    // ---- Read failure mid-stream: no file at dest + typed/friendly error ----

    /// A read stream that yields one chunk, then errors — simulates a device
    /// unplugged mid-download. Mirrors the local writer's NON-self-cleaning
    /// read-error branch (`chunk_result?` propagates, partial left behind).
    struct FailingReadStream {
        first_chunk_sent: bool,
        total: u64,
    }

    impl VolumeReadStream for FailingReadStream {
        fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
            Box::pin(async move {
                if !self.first_chunk_sent {
                    self.first_chunk_sent = true;
                    Some(Ok(vec![0u8; 1024]))
                } else {
                    Some(Err(VolumeError::DeviceDisconnected("cable yanked".into())))
                }
            })
        }
        fn total_size(&self) -> u64 {
            self.total
        }
        fn bytes_read(&self) -> u64 {
            if self.first_chunk_sent { 1024 } else { 0 }
        }
    }

    /// A volume whose `open_read_stream` hands back a stream that fails mid-way.
    struct MidStreamFailVolume;

    impl Volume for MidStreamFailVolume {
        fn name(&self) -> &str {
            "Failing"
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
        ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
        {
            Box::pin(async { Ok(vec![]) })
        }
        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>>
        {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            Box::pin(async move {
                Ok(crate::file_system::listing::FileEntry {
                    size: Some(4096),
                    ..crate::file_system::listing::FileEntry::new(name, path.display().to_string(), false, false)
                })
            })
        }
        fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async { true })
        }
        fn is_directory<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
            Box::pin(async { Ok(false) })
        }
        fn open_read_stream<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
            Box::pin(async {
                Ok(Box::new(FailingReadStream {
                    first_chunk_sent: false,
                    total: 4096,
                }) as Box<dyn VolumeReadStream>)
            })
        }
    }

    #[tokio::test]
    async fn read_failure_midstream_leaves_no_file_at_dest_and_returns_friendly_error() {
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("video.mov");

        let resolver = FixedResolver(Some(Arc::new(MidStreamFailVolume)));
        let err = fulfill_with_resolver(&resolver, "phone", Path::new("/video.mov"), &dest)
            .await
            .expect_err("a mid-stream read failure must surface an error");

        assert!(!err.cancelled, "a device disconnect is not a user cancel");
        assert!(!err.friendly.title.is_empty(), "must carry friendly copy");
        // The cleanup contract: the local writer does NOT self-clean on a read
        // error, so the service must remove the partial itself.
        assert!(
            !dest.exists(),
            "a failed fulfillment must leave NO partial file at the Finder-chosen dest"
        );
    }

    // ---- Unwritable destination ----

    #[tokio::test]
    async fn unwritable_destination_returns_error_and_lands_nothing() {
        let v = populated_volume();
        add_file(&v, "/a.txt", b"hello").await;
        // A dest path whose parent is a FILE, not a directory: create_dir_all /
        // File::create will fail (ENOTDIR).
        let dest_dir = tempfile::tempdir().unwrap();
        let blocker = dest_dir.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let dest = blocker.join("nested").join("a.txt");

        let resolver = FixedResolver(Some(v));
        let err = fulfill_with_resolver(&resolver, "phone", Path::new("/a.txt"), &dest)
            .await
            .expect_err("an unwritable destination must error");
        assert!(!err.friendly.title.is_empty());
        assert!(!dest.exists());
    }

    // ---- Missing source volume ----

    #[tokio::test]
    async fn missing_source_volume_returns_disconnected_friendly_error() {
        let resolver = FixedResolver(None);
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("x.jpg");
        let err = fulfill_with_resolver(&resolver, "gone", Path::new("/x.jpg"), &dest)
            .await
            .expect_err("a vanished source volume must error");
        assert!(!err.friendly.title.is_empty());
        assert!(!dest.exists());
    }

    // ---- Folder fulfillment: recursive content lands ----

    #[tokio::test]
    async fn folder_fulfillment_lands_recursive_content() {
        let v = InMemoryVolume::new("Phone");
        add_dir(&v, "/DCIM").await;
        add_file(&v, "/DCIM/a.jpg", b"aaa").await;
        add_dir(&v, "/DCIM/sub").await;
        add_file(&v, "/DCIM/sub/b.jpg", b"bbbb").await;
        let v = Arc::new(v);

        let dest_dir = tempfile::tempdir().unwrap();
        // Finder uniquified the folder name too.
        let dest = dest_dir.path().join("DCIM copy");

        let resolver = FixedResolver(Some(v));
        let outcome = fulfill_with_resolver(&resolver, "phone", Path::new("/DCIM"), &dest)
            .await
            .expect("folder fulfillment should succeed");
        assert!(outcome.is_dir, "a folder fulfillment reports is_dir = true");

        assert!(dest.is_dir(), "the dest folder must land under the Finder leaf");
        assert_eq!(std::fs::read(dest.join("a.jpg")).unwrap(), b"aaa");
        assert!(dest.join("sub").is_dir(), "nested subdir must land");
        assert_eq!(std::fs::read(dest.join("sub").join("b.jpg")).unwrap(), b"bbbb");
    }

    // ---- Busy-volume seam: source registered during, released after ----

    /// A volume whose `open_read_stream` blocks on a barrier so the test can
    /// observe the busy set WHILE the fulfillment is mid-flight, then release it.
    struct BlockingVolume {
        inner: InMemoryVolume,
        gate: Arc<tokio::sync::Notify>,
        reached: Arc<tokio::sync::Notify>,
    }

    impl Volume for BlockingVolume {
        fn name(&self) -> &str {
            "Blocking"
        }
        fn root(&self) -> &Path {
            Path::new("/")
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
        {
            self.inner.list_directory(path, on_progress)
        }
        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>>
        {
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
        fn open_read_stream<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
            let gate = Arc::clone(&self.gate);
            let reached = Arc::clone(&self.reached);
            Box::pin(async move {
                // Signal "I'm streaming now" then wait for the test to release.
                reached.notify_one();
                gate.notified().await;
                self.inner.open_read_stream(path).await
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn source_volume_is_busy_during_fulfillment_and_released_after() {
        use crate::file_system::write_operations::busy_volume_ids;

        let inner = InMemoryVolume::new("Phone");
        add_file(&inner, "/clip.mov", b"video bytes").await;
        let gate = Arc::new(tokio::sync::Notify::new());
        let reached = Arc::new(tokio::sync::Notify::new());
        let volume = Arc::new(BlockingVolume {
            inner,
            gate: Arc::clone(&gate),
            reached: Arc::clone(&reached),
        });

        // A unique source volume id so the assertion isn't polluted by other
        // parallel tests sharing the global busy set.
        let volume_id = format!("phone-{}", uuid::Uuid::new_v4());
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("clip.mov");

        let resolver = Arc::new(FixedResolver(Some(volume)));
        let resolver_clone = Arc::clone(&resolver);
        let vid = volume_id.clone();
        let dest_for_task = dest.clone();
        let handle = tokio::spawn(async move {
            fulfill_with_resolver(resolver_clone.as_ref(), &vid, Path::new("/clip.mov"), &dest_for_task).await
        });

        // Wait until the fulfillment is mid-stream, then assert the source is busy.
        reached.notified().await;
        assert!(
            busy_volume_ids().contains(&volume_id),
            "the source volume must be busy while the promise is streaming (eject guard)"
        );

        // Release the stream; the fulfillment finishes.
        gate.notify_one();
        handle.await.unwrap().expect("fulfillment should succeed");

        assert!(
            !busy_volume_ids().contains(&volume_id),
            "the source volume must clear from the busy set once the fulfillment finishes"
        );
        assert!(dest.exists());
    }

    // ---- Folder fulfillment: error mid-folder removes the created tree ----

    /// A volume that lists a dir with one good file and one file whose read
    /// stream fails — to prove a mid-folder failure cleans the created tree.
    struct PartialFailFolderVolume {
        inner: InMemoryVolume,
    }

    impl Volume for PartialFailFolderVolume {
        fn name(&self) -> &str {
            "PartialFail"
        }
        fn root(&self) -> &Path {
            Path::new("/")
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
        {
            self.inner.list_directory(path, on_progress)
        }
        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>>
        {
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
        fn open_read_stream<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
            // The "bad.jpg" file fails mid-stream; everything else streams fine.
            if path.file_name().map(|n| n == "bad.jpg").unwrap_or(false) {
                Box::pin(async {
                    Ok(Box::new(FailingReadStream {
                        first_chunk_sent: false,
                        total: 4096,
                    }) as Box<dyn VolumeReadStream>)
                })
            } else {
                self.inner.open_read_stream(path)
            }
        }
    }

    #[tokio::test]
    async fn folder_error_midstream_removes_the_created_tree() {
        let inner = InMemoryVolume::new("Phone");
        add_dir(&inner, "/DCIM").await;
        add_file(&inner, "/DCIM/good.jpg", b"good").await;
        add_file(&inner, "/DCIM/bad.jpg", b"will fail").await;
        let v = Arc::new(PartialFailFolderVolume { inner });

        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("DCIM");

        let resolver = FixedResolver(Some(v));
        let err = fulfill_with_resolver(&resolver, "phone", Path::new("/DCIM"), &dest)
            .await
            .expect_err("a mid-folder read failure must surface an error");
        assert!(!err.friendly.title.is_empty());
        // The whole created tree must be gone — not a half-downloaded folder.
        assert!(
            !dest.exists(),
            "a failed folder fulfillment must remove the entire created tree"
        );
    }
}
