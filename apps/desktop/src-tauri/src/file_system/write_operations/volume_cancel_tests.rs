//! Tests for cooperative cancel propagation into volume backends (M2 of
//! `cancel-settled-plan.md`).
//!
//! These tests pin the contract that `OperationIntent::Stopped` actually stops
//! the wire activity (per-USB-roundtrip granularity), not just the loop above
//! it. They drive `delete_volume_files_with_progress_inner` against a
//! `CancellingVolume` that mimics MTP's per-handle loop with an explicit
//! cancel check — when the token flips, the listing/delete bails promptly
//! instead of running to completion.
//!
//! Real-device end-to-end coverage (a Pixel with a 950-entry `/DCIM/Camera`)
//! lives in M5 of the plan; these tests pin the wiring so the prompt-cancel
//! behaviour is regression-safe.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::delete::delete_volume_files_with_progress_inner;
use super::state::{OperationIntent, WRITE_OPERATION_STATE, WriteOperationState, cancel_write_operation, load_intent};
use super::types::{CollectorEventSink, WriteOperationConfig};
use crate::file_system::get_volume_manager;
use crate::file_system::volume::{InMemoryVolume, Volume, VolumeError};

// ----------------------------------------------------------------------------
// Volume that honors `list_directory_with_cancel` / `delete_with_cancel`
// ----------------------------------------------------------------------------

/// Wraps an `InMemoryVolume` and:
/// - increments per-entry "stat" simulating an MTP `GetObjectInfo` USB loop.
/// - between each simulated stat, checks the cancel flag and bails with
///   `VolumeError::Cancelled` if set.
/// - records how far it got before the bail so tests can assert "promptly".
///
/// This mirrors what mtp-rs does internally: the per-handle iteration inside
/// `ObjectListing::next` checks the token before issuing each USB roundtrip.
struct CancellingVolume {
    inner: InMemoryVolume,
    /// Entries handed back per listing, simulating a real device's response.
    children: Vec<crate::file_system::listing::FileEntry>,
    /// Number of entries the per-handle loop fetched before bailing on a
    /// flipped cancel. Reset per listing call.
    fetched_before_cancel: AtomicUsize,
    /// Whether a listing call observed a cancel.
    listing_observed_cancel: AtomicBool,
    /// Whether the per-leaf delete observed a cancel.
    delete_observed_cancel: AtomicBool,
}

impl CancellingVolume {
    fn new(name: &str, children: Vec<crate::file_system::listing::FileEntry>) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            children,
            fetched_before_cancel: AtomicUsize::new(0),
            listing_observed_cancel: AtomicBool::new(false),
            delete_observed_cancel: AtomicBool::new(false),
        }
    }
}

impl Volume for CancellingVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
    {
        // No-cancel path: fall through to inner. Tests don't hit this; the
        // production code calls `list_directory_with_cancel`.
        let _ = on_progress;
        let _ = path;
        Box::pin(async move { Ok(self.children.clone()) })
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
        cancel: Option<&'a Arc<AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
    {
        let _ = path;
        Box::pin(async move {
            // Simulate the MTP per-handle loop: check cancel before each
            // simulated USB roundtrip, then "fetch" the entry.
            let mut yielded = Vec::with_capacity(self.children.len());
            for (i, entry) in self.children.iter().enumerate() {
                if let Some(flag) = cancel
                    && flag.load(Ordering::Acquire)
                {
                    self.fetched_before_cancel.store(i, Ordering::Release);
                    self.listing_observed_cancel.store(true, Ordering::Release);
                    return Err(VolumeError::Cancelled("listing cancelled".to_string()));
                }
                yielded.push(entry.clone());
                if let Some(cb) = on_progress {
                    cb(yielded.len());
                }
                // Simulate a slow USB roundtrip per handle. Each `GetObjectInfo`
                // on a real MTP device is on the order of milliseconds; we use
                // a similar delay so a concurrent cancel reliably lands between
                // iterations under test conditions.
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            self.fetched_before_cancel.store(yielded.len(), Ordering::Release);
            Ok(yielded)
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
        // Mirror MTP's "find by name in parent listing" behavior so the
        // delete path's `is_directory` probe resolves. The top-level test
        // directory "/dir" is synthesized as a directory entry on the fly so
        // the scan recursion treats it as a folder and descends into the
        // simulated 500-entry listing below.
        Box::pin(async move {
            let path_str = path.to_string_lossy();
            if path_str == "/dir" || path_str == "dir" {
                return Ok(make_file_entry("dir", "/", true));
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.is_empty() {
                return Err(VolumeError::NotFound(path.display().to_string()));
            }
            self.children
                .iter()
                .find(|e| e.name == name)
                .cloned()
                .ok_or_else(|| VolumeError::NotFound(path.display().to_string()))
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.is_ok() })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.map(|e| e.is_directory) })
    }

    fn delete<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn delete_with_cancel<'a>(
        &'a self,
        _path: &'a Path,
        cancel: Option<&'a Arc<AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(flag) = cancel
                && flag.load(Ordering::Acquire)
            {
                self.delete_observed_cancel.store(true, Ordering::Release);
                return Err(VolumeError::Cancelled("delete cancelled".to_string()));
            }
            Ok(())
        })
    }
}

// ----------------------------------------------------------------------------
// Test helpers
// ----------------------------------------------------------------------------

fn unique(suffix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "cancel_{}_{}_{}",
        suffix,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

fn make_file_entry(name: &str, parent: &str, is_dir: bool) -> crate::file_system::listing::FileEntry {
    crate::file_system::listing::FileEntry {
        size: if is_dir { None } else { Some(10) },
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "test".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..crate::file_system::listing::FileEntry::new(
            name.to_string(),
            format!("{}/{}", parent.trim_end_matches('/'), name),
            is_dir,
            false,
        )
    }
}

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

/// Asserts that `cancel_write_operation` flipping the intent ALSO flips
/// `backend_cancel`, and that the volume's `list_directory_with_cancel` sees
/// the flag and bails. End-to-end at the cmdr level.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mtp_listing_cancels_promptly_when_intent_flips() {
    // A directory with 500 simulated entries. The real incident was 950 in
    // `/DCIM/Camera`; smaller here so the un-cancelled wall-clock stays inside
    // the assert's 3 s headroom on slow CI.
    let total_children = 500usize;
    let children: Vec<_> = (0..total_children)
        .map(|i| make_file_entry(&format!("photo-{:04}.jpg", i), "/dir", false))
        .collect();

    let vol_name = unique("listing-cancel");
    let vol = Arc::new(CancellingVolume::new(&vol_name, children));
    get_volume_manager().register(&vol_name, vol.clone() as Arc<dyn Volume>);

    let op_id = unique("op");
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    WRITE_OPERATION_STATE
        .write()
        .unwrap()
        .insert(op_id.clone(), Arc::clone(&state));

    // Spawn the delete; while it's running, fire cancel from another task.
    let op_id_for_cancel = op_id.clone();
    let canceller = tokio::spawn(async move {
        // Let the listing get going before we cancel.
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel_write_operation(&op_id_for_cancel, false);
    });

    let sink = CollectorEventSink::new();
    let started = Instant::now();
    let sources = vec![PathBuf::from("/dir")];
    let config = WriteOperationConfig::default();
    let result = delete_volume_files_with_progress_inner(
        vol.clone() as Arc<dyn Volume>,
        &vol_name,
        &sink,
        &op_id,
        &state,
        &sources,
        &config,
    )
    .await;
    let elapsed = started.elapsed();
    canceller.await.unwrap();

    // The op must error with Cancelled.
    assert!(
        matches!(
            result,
            Err(crate::file_system::write_operations::types::WriteOperationError::Cancelled { .. })
        ),
        "expected Cancelled, got {result:?}"
    );

    // Intent and backend_cancel must both be flipped.
    assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);
    assert!(
        state.backend_cancel.load(Ordering::Acquire),
        "backend_cancel must be flipped so an MTP-style backend bails the in-flight USB loop"
    );

    // The listing must have observed the cancel and bailed early — not after
    // fetching all 950 entries.
    assert!(
        vol.listing_observed_cancel.load(Ordering::Acquire),
        "the volume's list_directory_with_cancel must observe the cancel flag"
    );
    let fetched = vol.fetched_before_cancel.load(Ordering::Acquire);
    assert!(
        fetched < total_children,
        // allowed-pluralize-noun: total_children is the const 500.
        "expected listing to bail before fetching all {total_children} entries, got {fetched}"
    );

    // Should be much faster than the 30 s wedge from the incident. With a
    // sleep-then-cancel of 20 ms and prompt propagation, the whole op fits in
    // a few hundred ms; we give ourselves comfortable headroom for slow CI.
    assert!(
        elapsed < Duration::from_secs(3),
        "cancel must propagate promptly; took {elapsed:?}"
    );

    WRITE_OPERATION_STATE.write().unwrap().remove(&op_id);
}

/// Asserts that the cancel-flag adapter wiring is correct: flipping
/// `state.intent` via the public `cancel_write_operation` API flips
/// `state.backend_cancel`, which is the handle MTP code consults.
#[test]
fn cancel_flag_adapter_links_intent_to_backend_cancel() {
    let op_id = unique("adapter");
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    WRITE_OPERATION_STATE
        .write()
        .unwrap()
        .insert(op_id.clone(), Arc::clone(&state));

    assert!(
        !state.backend_cancel.load(Ordering::Acquire),
        "backend_cancel must start unset"
    );

    cancel_write_operation(&op_id, false);

    assert!(
        state.backend_cancel.load(Ordering::Acquire),
        "cancel_write_operation must flip backend_cancel as a side effect of intent → Stopped"
    );

    WRITE_OPERATION_STATE.write().unwrap().remove(&op_id);
}
