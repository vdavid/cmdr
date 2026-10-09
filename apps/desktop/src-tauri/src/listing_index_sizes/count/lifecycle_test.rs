//! Publication ownership ends before an owned backend operation necessarily does.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cmdr_fs::volume::{BatchScanResult, InMemoryVolume, ListingProgress, ScanBoundary, Volume, VolumeError};
use tokio::sync::Notify;

use super::{cancel, count_resolving, count_with, plan};
use crate::file_system::listing::caching_test_support::TestListing;
use crate::file_system::listing::metadata::FileEntry;

#[derive(Default)]
struct Gate {
    entered: Notify,
    release: Notify,
    exited: Notify,
    dropped: AtomicBool,
}

struct InFlight(Arc<Gate>);

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::SeqCst);
        self.0.exited.notify_one();
    }
}

struct GatedScan {
    inner: InMemoryVolume,
    gate: Arc<Gate>,
}

impl Volume for GatedScan {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let _operation = InFlight(Arc::clone(&self.gate));
            self.gate.entered.notify_one();
            self.gate.release.notified().await;
            // Models a protocol operation that must return before consulting stop.
            boundary.check().await?;
            self.inner.scan_for_copy_batch_with_boundary(paths, boundary).await
        })
    }
}

fn folder() -> FileEntry {
    FileEntry::new("a".to_string(), "/data/a".to_string(), true, false)
}

fn volume() -> Arc<dyn Volume> {
    Arc::new(InMemoryVolume::with_entries(
        "new",
        vec![
            folder(),
            FileEntry {
                size: Some(7),
                ..FileEntry::new("f".to_string(), "/data/a/f".to_string(), false, false)
            },
        ],
    ))
}

#[tokio::test(start_paused = true)]
async fn cancelled_backend_finishes_cooperatively_without_overwriting_a_replacement() {
    let listing = TestListing::new()
        .path("/data")
        .entries(vec![folder()])
        .insert("lifecycle-old-new");
    let gate = Arc::new(Gate::default());
    let old: Arc<dyn Volume> = Arc::new(GatedScan {
        inner: InMemoryVolume::new("old"),
        gate: Arc::clone(&gate),
    });
    let id = listing.id().to_string();
    let requested = plan(&id, false, None, |_| false).expect("cached");
    let counting = tokio::spawn(async move { count_with(&id, requested, old, &|_| {}).await });
    gate.entered.notified().await;
    assert!(cancel(listing.id()));
    assert!(counting.await.expect("UI completes").cancelled);
    assert!(!gate.dropped.load(Ordering::SeqCst), "in-flight scan was not dropped");

    let requested = plan(listing.id(), false, None, |_| false).expect("cached");
    assert_eq!(count_with(listing.id(), requested, volume(), &|_| {}).await.counted, 1);
    gate.release.notify_one();
    gate.exited.notified().await;
    let row = listing.entries().pop().expect("row");
    assert_eq!(
        (row.recursive_size, row.recursive_size_complete),
        (Some(7), Some(true)),
        "old worker cannot publish"
    );
}

#[tokio::test(start_paused = true)]
async fn cancellation_during_pending_resolution_completes_the_ui_without_dropping_io() {
    let listing = TestListing::new()
        .path("/data")
        .entries(vec![folder()])
        .insert("lifecycle-resolving");
    let gate = Arc::new(Gate::default());
    let resolving_gate = Arc::clone(&gate);
    let id = listing.id().to_string();
    let requested = plan(&id, false, None, |_| false).expect("cached");
    let counting = tokio::spawn(async move {
        count_resolving(
            &id,
            requested,
            async move {
                let _operation = InFlight(Arc::clone(&resolving_gate));
                resolving_gate.entered.notify_one();
                resolving_gate.release.notified().await;
                Some(volume())
            },
            &|_| {},
        )
        .await
    });
    gate.entered.notified().await;
    assert!(cancel(listing.id()));
    assert!(
        counting
            .await
            .expect("UI completes")
            .expect("cancelled outcome")
            .cancelled
    );
    assert!(!gate.dropped.load(Ordering::SeqCst));
    assert!(!cancel(listing.id()), "publication ownership is released");
    gate.release.notify_one();
    gate.exited.notified().await;
    assert!(listing.entries()[0].recursive_size.is_none(), "no late walk starts");
}

#[tokio::test]
async fn unreadable_and_quickly_cancelled_retries_preserve_a_manual_partial() {
    let dir = crate::test_support::TestDir::new("count-manual-unreadable");
    let missing_path = dir.join("missing").to_string_lossy().to_string();
    let listing = TestListing::new()
        .path(dir.to_path_buf())
        .entries(vec![FileEntry::new(
            "missing".to_string(),
            missing_path.clone(),
            true,
            false,
        )])
        .insert("lifecycle-manual");
    let _ = super::publish::publish(
        listing.id(),
        &missing_path,
        super::publish::reading(
            &missing_path,
            ListingProgress {
                files: 1,
                dirs: 0,
                bytes: 3,
            },
            3,
            super::publish::Walk::Partial,
        ),
        &|_| {},
    );
    let requested = plan(listing.id(), false, None, |_| true).expect("cached");
    let missing: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::local_folder(
        "missing", "/",
    ));
    assert_eq!(
        count_with(listing.id(), requested, missing, &|_| {}).await.unreadable,
        1
    );
    let requested = plan(listing.id(), false, None, |_| true).expect("still retryable");
    assert_eq!(requested.folders.len(), 1);
    let id = listing.id().to_string();
    let outcome = count_resolving(
        listing.id(),
        requested,
        async move {
            assert!(cancel(&id));
            Some(volume())
        },
        &|_| {},
    )
    .await
    .expect("cancelled");
    assert!(outcome.cancelled);
    assert_eq!(
        plan(listing.id(), false, None, |_| true)
            .expect("retryable")
            .folders
            .len(),
        1
    );
    assert_eq!(listing.entries()[0].recursive_size, Some(3));
}
