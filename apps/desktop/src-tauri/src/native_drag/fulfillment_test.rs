//! Tests for drag-out fulfillment (`fulfillment.rs`).

use super::*;
use crate::file_system::volume::{InMemoryVolume, ListingProgress};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A resolver that always returns the same volume (or `None`).
struct FixedResolver(Option<Arc<dyn Volume>>);

impl VolumeResolver for FixedResolver {
    fn resolve(&self, _volume_id: &str, _source_path: &Path) -> Option<Arc<dyn Volume>> {
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
    fn total_size(&self) -> crate::file_system::volume::StreamLength {
        crate::file_system::volume::StreamLength::Known(self.total)
    }
    fn bytes_read(&self) -> u64 {
        if self.first_chunk_sent { 1024 } else { 0 }
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
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
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
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
    assert!(!err.nserror_title().is_empty(), "must carry friendly copy");
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
    assert!(!err.nserror_title().is_empty());
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
    assert!(!err.nserror_title().is_empty());
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
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
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
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
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
    assert!(!err.nserror_title().is_empty());
    // The whole created tree must be gone — not a half-downloaded folder.
    assert!(
        !dest.exists(),
        "a failed folder fulfillment must remove the entire created tree"
    );
}

/// ❗ A phone or server that lists `../escape.txt` inside a dragged folder
/// must not write next to the folder Finder made: the fulfillment refuses
/// the name, and nothing lands outside the drop.
#[tokio::test]
async fn folder_with_a_listed_name_that_climbs_out_is_refused() {
    let v = populated_volume();
    add_dir(&v, "/DCIM").await;
    add_file(&v, "/DCIM/evil.jpg", b"EVIL").await;
    v.set_reported_name(Path::new("/DCIM/evil.jpg"), "../escape.jpg");
    // Where the raw join would read from, so a missing source can't be what
    // stops the escape.
    add_file(&v, "/DCIM/../escape.jpg", b"EVIL").await;

    let dest_dir = tempfile::tempdir().unwrap();
    let dest = dest_dir.path().join("DCIM");

    let resolver = FixedResolver(Some(v));
    let err = fulfill_with_resolver(&resolver, "phone", Path::new("/DCIM"), &dest)
        .await
        .expect_err("a listed name that climbs out must fail the fulfillment");
    assert!(!err.cancelled);
    assert!(
        !dest_dir.path().join("escape.jpg").exists(),
        "the listed name wrote outside the dropped folder"
    );
}
