//! Tests for cooperative cancel propagation into volume backends. See
//! `crate::mtp` § "Cancel propagation" for the contract.
//!
//! These tests pin the contract that `OperationIntent::Stopped` actually stops
//! the wire activity (per-USB-roundtrip granularity), not just the loop above
//! it. They drive `delete_volume_files_with_progress_inner` against a
//! `CancellingVolume` that mimics MTP's per-handle loop with an explicit
//! cancel check — when the token flips, the listing/delete bails promptly
//! instead of running to completion.
//!
//! Real-device end-to-end coverage (a Pixel with a 950-entry `/DCIM/Camera`) is a
//! manual smoke test; these unit tests pin the wiring so the prompt-cancel behaviour
//! is regression-safe.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::super::state::{
    OperationIntent, WRITE_OPERATION_STATE, WriteOperationState, cancel_write_operation, load_intent,
};
use super::super::types::{CollectorEventSink, WriteOperationConfig};
use super::walker::delete_volume_files_with_progress_inner;
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
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
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
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
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
                    cb(crate::file_system::volume::ListingProgress {
                        files: yielded.len(),
                        dirs: 0,
                        bytes: 0,
                    });
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

/// Pins the settle-event contract: when an MTP-style volume delete is
/// cancelled, the spawn task's `WriteSettledGuard` Drop fires `write-settled`
/// AFTER the handler emitted `write-cancelled`. The FE needs that ordering to
/// keep "Cancelling…" up until the volume is genuinely torn down.
///
/// We can't drive the full `tokio::spawn` path here (it needs a real
/// `AppHandle`), so we manually reproduce the spawn-task scope shape:
///   1. Construct `WriteSettledGuard` first (RAII).
///   2. Run the handler inner.
///   3. Let the guard drop at end of scope.
/// The collector captures the relative order on the sink.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_cancel_emits_write_settled_event() {
    use crate::file_system::write_operations::state::WriteSettledGuard;
    use crate::file_system::write_operations::types::{OperationEventSink, WriteOperationType, WriteSettledEvent};

    /// Test sink that records both terminal events AND settled in arrival order.
    struct OrderedSink {
        events: std::sync::Mutex<Vec<&'static str>>,
        inner_collector: Arc<CollectorEventSink>,
    }

    impl OperationEventSink for OrderedSink {
        fn emit_progress(&self, e: super::super::types::WriteProgressEvent) {
            self.inner_collector.emit_progress(e);
        }
        fn emit_complete(&self, e: super::super::types::WriteCompleteEvent) {
            self.events.lock().unwrap().push("complete");
            self.inner_collector.emit_complete(e);
        }
        fn emit_cancelled(&self, e: super::super::types::WriteCancelledEvent) {
            self.events.lock().unwrap().push("cancelled");
            self.inner_collector.emit_cancelled(e);
        }
        fn emit_error(&self, e: super::super::types::WriteErrorEvent) {
            self.events.lock().unwrap().push("error");
            self.inner_collector.emit_error(e);
        }
        fn emit_conflict(&self, e: super::super::types::WriteConflictEvent) {
            self.inner_collector.emit_conflict(e);
        }
        fn emit_source_item_done(&self, e: super::super::types::WriteSourceItemDoneEvent) {
            self.inner_collector.emit_source_item_done(e);
        }
        fn emit_scan_progress(&self, e: super::super::types::ScanProgressEvent) {
            self.inner_collector.emit_scan_progress(e);
        }
        fn emit_scan_conflict(&self, c: super::super::types::ConflictInfo) {
            self.inner_collector.emit_scan_conflict(c);
        }
        fn emit_dry_run_complete(&self, r: super::super::types::DryRunResult) {
            self.inner_collector.emit_dry_run_complete(r);
        }
        fn emit_settled(&self, e: WriteSettledEvent) {
            self.events.lock().unwrap().push("settled");
            self.inner_collector.emit_settled(e);
        }
    }

    let inner_collector = Arc::new(CollectorEventSink::new());
    let sink = Arc::new(OrderedSink {
        events: std::sync::Mutex::new(Vec::new()),
        inner_collector: Arc::clone(&inner_collector),
    });

    // Use a moderately-sized directory so the test reliably fires a cancel
    // mid-operation. Timing of scan vs. delete phase is irrelevant: the guard
    // fires no matter what phase ends the op.
    let children: Vec<_> = (0..200)
        .map(|i| make_file_entry(&format!("photo-{:04}.jpg", i), "/dir", false))
        .collect();
    let vol_name = unique("settle-cancel");
    let vol = Arc::new(CancellingVolume::new(&vol_name, children));
    get_volume_manager().register(&vol_name, vol.clone() as Arc<dyn Volume>);

    let op_id = unique("op");
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    WRITE_OPERATION_STATE
        .write()
        .unwrap()
        .insert(op_id.clone(), Arc::clone(&state));

    let op_id_for_cancel = op_id.clone();
    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel_write_operation(&op_id_for_cancel, false);
    });

    let sources = vec![PathBuf::from("/dir")];
    let config = WriteOperationConfig::default();

    // Mirror production scope shape: guard constructed FIRST, drops LAST.
    {
        let sink_for_guard: Arc<dyn OperationEventSink> = Arc::clone(&sink) as Arc<dyn OperationEventSink>;
        let _settled_guard = WriteSettledGuard::new_with_sink(
            sink_for_guard,
            op_id.clone(),
            WriteOperationType::Delete,
            Some(vol_name.clone()),
        );

        let result = delete_volume_files_with_progress_inner(
            vol.clone() as Arc<dyn Volume>,
            &vol_name,
            &*sink,
            &op_id,
            &state,
            &sources,
            &config,
        )
        .await;
        assert!(
            matches!(
                result,
                Err(crate::file_system::write_operations::types::WriteOperationError::Cancelled { .. })
            ),
            "expected Cancelled, got {result:?}"
        );
    }
    canceller.await.unwrap();

    // Settle must always fire exactly once.
    // The ordering of cancelled vs. settled relative to each other is pinned
    // in `settle_event_tests::settled_event_order_is_after_terminal_outcome_event`;
    // here we just pin "settle fires for the cancel flow at all", since the
    // exact cancel-emit site (scan vs. delete phase) is timing-dependent.
    let settled = inner_collector.settled.lock().unwrap();
    assert_eq!(settled.len(), 1, "settle must fire exactly once after cancel");
    assert_eq!(settled[0].operation_id, op_id);
    assert_eq!(settled[0].volume_id.as_deref(), Some(vol_name.as_str()));

    // Sanity: the order vec, if non-empty, ends with "settled" (the guard
    // drops last in the test scope).
    let order = sink.events.lock().unwrap().clone();
    assert_eq!(
        order.last().copied(),
        Some("settled"),
        "settled must be the last event to fire (guard drops at end of scope)"
    );

    WRITE_OPERATION_STATE.write().unwrap().remove(&op_id);
}

/// Pins that successful volume delete also gets a `write-settled` event.
/// Same harness as the cancel test, just doesn't fire the canceller. The
/// guard fires unconditionally on scope exit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_complete_emits_write_settled_event() {
    use crate::file_system::write_operations::state::WriteSettledGuard;
    use crate::file_system::write_operations::types::WriteOperationType;

    let inner_collector = Arc::new(CollectorEventSink::new());

    // Small, fast directory: no cancel, runs to completion.
    let children: Vec<_> = (0..5)
        .map(|i| make_file_entry(&format!("file-{}.txt", i), "/dir", false))
        .collect();
    let vol_name = unique("settle-complete");
    let vol = Arc::new(CancellingVolume::new(&vol_name, children));
    get_volume_manager().register(&vol_name, vol.clone() as Arc<dyn Volume>);

    let op_id = unique("op");
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    WRITE_OPERATION_STATE
        .write()
        .unwrap()
        .insert(op_id.clone(), Arc::clone(&state));

    let sources = vec![PathBuf::from("/dir")];
    let config = WriteOperationConfig::default();

    {
        let sink_for_guard: Arc<dyn crate::file_system::write_operations::types::OperationEventSink> =
            Arc::clone(&inner_collector) as Arc<dyn crate::file_system::write_operations::types::OperationEventSink>;
        let _settled_guard = WriteSettledGuard::new_with_sink(
            sink_for_guard,
            op_id.clone(),
            WriteOperationType::Delete,
            Some(vol_name.clone()),
        );

        let result = delete_volume_files_with_progress_inner(
            vol.clone() as Arc<dyn Volume>,
            &vol_name,
            &*inner_collector,
            &op_id,
            &state,
            &sources,
            &config,
        )
        .await;
        assert!(result.is_ok(), "delete must succeed: {result:?}");
    }

    let settled = inner_collector.settled.lock().unwrap();
    assert_eq!(settled.len(), 1, "settle must fire once on successful completion");
    assert_eq!(settled[0].operation_id, op_id);

    WRITE_OPERATION_STATE.write().unwrap().remove(&op_id);
}

/// Pins that an error path also fires `write-settled`. We use a volume that
/// errors on a list_directory call to force the error branch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_error_emits_write_settled_event() {
    use crate::file_system::write_operations::state::WriteSettledGuard;
    use crate::file_system::write_operations::types::WriteOperationType;

    /// Volume that errors on list_directory_with_cancel.
    struct FailingVolume {
        inner: InMemoryVolume,
    }

    impl Volume for FailingVolume {
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
            _path: &'a Path,
            _on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
        {
            Box::pin(async move {
                Err(VolumeError::IoError {
                    message: "simulated".to_string(),
                    raw_os_error: None,
                })
            })
        }
        fn list_directory_with_cancel<'a>(
            &'a self,
            _path: &'a Path,
            _on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
            _cancel: Option<&'a Arc<AtomicBool>>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
        {
            Box::pin(async move {
                Err(VolumeError::IoError {
                    message: "simulated".to_string(),
                    raw_os_error: None,
                })
            })
        }
        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>>
        {
            Box::pin(async move {
                let path_str = path.to_string_lossy();
                if path_str == "/dir" || path_str == "dir" {
                    return Ok(make_file_entry("dir", "/", true));
                }
                Err(VolumeError::NotFound(path.display().to_string()))
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
    }

    let inner_collector = Arc::new(CollectorEventSink::new());

    let vol_name = unique("settle-error");
    let vol = Arc::new(FailingVolume {
        inner: InMemoryVolume::new(&vol_name),
    });
    get_volume_manager().register(&vol_name, vol.clone() as Arc<dyn Volume>);

    let op_id = unique("op");
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    WRITE_OPERATION_STATE
        .write()
        .unwrap()
        .insert(op_id.clone(), Arc::clone(&state));

    let sources = vec![PathBuf::from("/dir")];
    let config = WriteOperationConfig::default();

    {
        let sink_for_guard: Arc<dyn crate::file_system::write_operations::types::OperationEventSink> =
            Arc::clone(&inner_collector) as Arc<dyn crate::file_system::write_operations::types::OperationEventSink>;
        let _settled_guard = WriteSettledGuard::new_with_sink(
            sink_for_guard,
            op_id.clone(),
            WriteOperationType::Delete,
            Some(vol_name.clone()),
        );

        let _result = delete_volume_files_with_progress_inner(
            vol.clone() as Arc<dyn Volume>,
            &vol_name,
            &*inner_collector,
            &op_id,
            &state,
            &sources,
            &config,
        )
        .await;
        // Result kind is path-dependent (scan error vs. delete error); the
        // contract here is "settle fires regardless", not the specific error
        // shape.
    }

    let settled = inner_collector.settled.lock().unwrap();
    assert_eq!(settled.len(), 1, "settle must fire once even on error path");

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
