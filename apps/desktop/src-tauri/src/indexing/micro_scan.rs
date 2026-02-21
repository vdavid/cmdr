//! Priority micro-scan manager for on-demand subtree scans.
//!
//! Manages targeted scans triggered by user navigation (`CurrentDir`) and Space key
//! (`UserSelected`). Runs up to `max_concurrent` scans simultaneously. Pending requests
//! are queued by priority. All scans are skipped once the full background scan completes
//! (at that point, all dir_stats are authoritative).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::indexing::scanner;
use crate::indexing::writer::IndexWriter;

// ── Types ────────────────────────────────────────────────────────────

/// Priority levels for micro-scans. Higher ordinal = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanPriority {
    /// Navigation: auto-cancelled when leaving the directory.
    CurrentDir = 0,
    /// Space key: never auto-cancelled, persists until complete.
    UserSelected = 1,
}

/// Tracks a single active micro-scan task.
struct ActiveScan {
    priority: ScanPriority,
    cancelled: Arc<AtomicBool>,
    handle: tokio::task::JoinHandle<()>,
}

/// Inner state behind the async mutex.
struct MicroScanManagerInner {
    active: HashMap<PathBuf, ActiveScan>,
    completed: HashSet<PathBuf>,
    full_scan_complete: bool,
    max_concurrent: usize,
    queue: VecDeque<(ScanPriority, PathBuf)>,
    writer: IndexWriter,
}

// ── MicroScanManager ─────────────────────────────────────────────────

/// Thread-safe handle for managing on-demand subtree scans.
///
/// Clone-friendly via internal `Arc`. All public methods acquire an async mutex.
#[derive(Clone)]
pub struct MicroScanManager {
    inner: Arc<tokio::sync::Mutex<MicroScanManagerInner>>,
}

impl MicroScanManager {
    /// Create a new manager.
    ///
    /// - `writer`: shared handle to the single-writer thread.
    /// - `max_concurrent`: maximum number of micro-scans running at once (2-4 recommended).
    pub fn new(writer: IndexWriter, max_concurrent: usize) -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(MicroScanManagerInner {
                active: HashMap::new(),
                completed: HashSet::new(),
                full_scan_complete: false,
                max_concurrent,
                queue: VecDeque::new(),
                writer,
            })),
        }
    }

    /// Request a micro-scan for `path` at the given priority.
    ///
    /// Deduplication rules:
    /// - If the path is already completed or the full scan is done, the request is skipped.
    /// - If the path is already active at the same or higher priority, the request is skipped.
    /// - If active at a lower priority, the existing scan is cancelled and re-queued at
    ///   the higher priority.
    pub async fn request_scan(&self, path: PathBuf, priority: ScanPriority) {
        let mut inner = self.inner.lock().await;

        if inner.full_scan_complete || inner.completed.contains(&path) {
            return;
        }

        // Check if already active
        if let Some(existing) = inner.active.get(&path) {
            if existing.priority >= priority {
                return; // Already running at same or higher priority
            }
            // Lower priority active: cancel it and re-queue at higher priority
            existing.cancelled.store(true, Ordering::Relaxed);
            existing.handle.abort();
            inner.active.remove(&path);
        }

        // Check if already queued at same or higher priority
        if inner
            .queue
            .iter()
            .any(|(p, queued_path)| queued_path == &path && *p >= priority)
        {
            return;
        }

        // Remove any lower-priority duplicate from queue
        inner.queue.retain(|(_, queued_path)| queued_path != &path);

        if inner.active.len() < inner.max_concurrent {
            start_scan(&mut inner, path, priority, &self.inner);
        } else {
            enqueue(&mut inner.queue, path, priority);
        }
    }

    /// Cancel all active and queued scans with `CurrentDir` priority whose path
    /// starts with `dir_path` (the directory the user navigated away from).
    pub async fn cancel_current_dir_scans(&self, dir_path: &Path) {
        let mut inner = self.inner.lock().await;
        let dir_str = dir_path.to_string_lossy().to_string();

        // Cancel active CurrentDir scans under this path
        let to_cancel: Vec<PathBuf> = inner
            .active
            .iter()
            .filter(|(p, scan)| scan.priority == ScanPriority::CurrentDir && is_child_of(p, &dir_str))
            .map(|(p, _)| p.clone())
            .collect();

        for path in &to_cancel {
            if let Some(scan) = inner.active.remove(path) {
                scan.cancelled.store(true, Ordering::Relaxed);
                scan.handle.abort();
            }
        }

        // Remove queued CurrentDir scans under this path
        inner
            .queue
            .retain(|(prio, p)| !(*prio == ScanPriority::CurrentDir && is_child_of(p, &dir_str)));

        // Start queued scans if slots opened up
        drain_queue(&mut inner, &self.inner);
    }

    /// Mark the full background scan as complete. Cancels all pending and active micro-scans.
    pub async fn mark_full_scan_complete(&self) {
        let mut inner = self.inner.lock().await;
        inner.full_scan_complete = true;
        inner.queue.clear();

        // Cancel all active scans
        let paths: Vec<PathBuf> = inner.active.keys().cloned().collect();
        for path in paths {
            if let Some(scan) = inner.active.remove(&path) {
                scan.cancelled.store(true, Ordering::Relaxed);
                scan.handle.abort();
            }
        }
    }

    /// Cancel everything. Used on app shutdown.
    pub async fn cancel_all(&self) {
        let mut inner = self.inner.lock().await;
        inner.queue.clear();

        let paths: Vec<PathBuf> = inner.active.keys().cloned().collect();
        for path in paths {
            if let Some(scan) = inner.active.remove(&path) {
                scan.cancelled.store(true, Ordering::Relaxed);
                scan.handle.abort();
            }
        }
    }

    /// Number of currently active scans.
    pub async fn active_count(&self) -> usize {
        self.inner.lock().await.active.len()
    }

    /// Number of pending (queued) scans.
    pub async fn queue_len(&self) -> usize {
        self.inner.lock().await.queue.len()
    }

    /// Check if a path has completed its micro-scan.
    pub async fn is_completed(&self, path: &Path) -> bool {
        self.inner.lock().await.completed.contains(path)
    }
}

// ── Internal helpers ─────────────────────────────────────────────────

/// Start a micro-scan task for the given path.
///
/// Spawns a `spawn_blocking` task that runs `scan_subtree`, plus a monitor
/// task that awaits the scan, moves it from `active` to `completed`, and
/// drains the queue to start pending scans.
fn start_scan(
    inner: &mut MicroScanManagerInner,
    path: PathBuf,
    priority: ScanPriority,
    manager_arc: &Arc<tokio::sync::Mutex<MicroScanManagerInner>>,
) {
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = Arc::clone(&cancelled);
    let writer = inner.writer.clone();
    let path_clone = path.clone();

    // Oneshot channel to signal the monitor when the scan completes
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::task::spawn_blocking(move || {
        let result = scanner::scan_subtree(&path_clone, &writer, &cancelled_clone);
        match result {
            Ok(summary) => {
                log::debug!(
                    "Micro-scan completed for {}: {} entries, {} dirs, {}ms",
                    path_clone.display(),
                    summary.total_entries,
                    summary.total_dirs,
                    summary.duration_ms,
                );
            }
            Err(e) => {
                log::debug!("Micro-scan for {} failed: {e}", path_clone.display());
            }
        }
        // Signal completion (receiver may already be dropped on cancel, that's fine)
        let _ = done_tx.send(());
    });

    // Monitor task: waits for the scan to finish, then cleans up and starts queued scans.
    let manager_clone = Arc::clone(manager_arc);
    let monitor_path = path.clone();
    tauri::async_runtime::spawn(async move {
        // Wait for the scan to signal completion (or for the sender to be dropped on cancel)
        let _ = done_rx.await;
        let mut guard = manager_clone.lock().await;
        if guard.active.remove(&monitor_path).is_some() {
            guard.completed.insert(monitor_path);
            drain_queue(&mut guard, &manager_clone);
        }
    });

    inner.active.insert(
        path,
        ActiveScan {
            priority,
            cancelled,
            handle,
        },
    );
}

/// Insert a scan request into the queue, maintaining priority order
/// (highest priority first, FIFO within the same priority).
fn enqueue(queue: &mut VecDeque<(ScanPriority, PathBuf)>, path: PathBuf, priority: ScanPriority) {
    // Find insertion point: after all items with >= priority
    let pos = queue.iter().position(|(p, _)| *p < priority).unwrap_or(queue.len());
    queue.insert(pos, (priority, path));
}

/// Start queued scans if there are free slots.
fn drain_queue(inner: &mut MicroScanManagerInner, manager_arc: &Arc<tokio::sync::Mutex<MicroScanManagerInner>>) {
    while inner.active.len() < inner.max_concurrent {
        if let Some((priority, path)) = inner.queue.pop_front() {
            if inner.completed.contains(&path) || inner.full_scan_complete {
                continue; // Skip already-completed or no longer needed
            }
            start_scan(inner, path, priority, manager_arc);
        } else {
            break;
        }
    }
}

/// Check if `path` is equal to or a child of `dir_str`.
fn is_child_of(path: &Path, dir_str: &str) -> bool {
    let path_str = path.to_string_lossy();
    path_str == dir_str || path_str.starts_with(&format!("{dir_str}/"))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::IndexStore;
    use std::fs;

    fn setup_writer() -> (IndexWriter, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test-micro.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path).expect("spawn writer");
        (writer, dir)
    }

    #[tokio::test]
    async fn request_scan_starts_immediately() {
        let (writer, _dir) = setup_writer();
        let mgr = MicroScanManager::new(writer.clone(), 4);

        let scan_root = tempfile::tempdir().unwrap();
        fs::write(scan_root.path().join("test.txt"), "content").unwrap();

        mgr.request_scan(scan_root.path().to_path_buf(), ScanPriority::UserSelected)
            .await;

        assert_eq!(mgr.active_count().await, 1);
        assert_eq!(mgr.queue_len().await, 0);

        // Wait for scan to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        writer.shutdown();
    }

    #[tokio::test]
    async fn duplicate_request_is_skipped() {
        let (writer, _dir) = setup_writer();
        let mgr = MicroScanManager::new(writer.clone(), 4);

        let scan_root = tempfile::tempdir().unwrap();
        fs::create_dir_all(scan_root.path().join("deep/nested")).unwrap();
        for i in 0..100 {
            fs::write(
                scan_root.path().join(format!("deep/nested/file{i}.txt")),
                format!("content {i}"),
            )
            .unwrap();
        }

        let path = scan_root.path().to_path_buf();
        mgr.request_scan(path.clone(), ScanPriority::UserSelected).await;
        mgr.request_scan(path.clone(), ScanPriority::UserSelected).await;

        // Should still be just 1 active scan (duplicate skipped)
        assert_eq!(mgr.active_count().await, 1);

        writer.shutdown();
    }

    #[tokio::test]
    async fn max_concurrent_limits_active_scans() {
        let (writer, _dir) = setup_writer();
        let mgr = MicroScanManager::new(writer.clone(), 2);

        // Create 3 directories to scan
        let roots: Vec<tempfile::TempDir> = (0..3)
            .map(|i| {
                let root = tempfile::tempdir().unwrap();
                fs::write(root.path().join(format!("file{i}.txt")), "content").unwrap();
                root
            })
            .collect();

        for root in &roots {
            mgr.request_scan(root.path().to_path_buf(), ScanPriority::CurrentDir)
                .await;
        }

        let active = mgr.active_count().await;
        let queued = mgr.queue_len().await;
        assert!(active <= 2, "should respect max_concurrent, got {active}");
        assert!(
            queued >= 1 || active == 3,
            "excess should be queued (queued={queued}, active={active})"
        );

        writer.shutdown();
    }

    #[tokio::test]
    async fn cancel_current_dir_scans_leaves_user_selected() {
        let (writer, _dir) = setup_writer();
        let mgr = MicroScanManager::new(writer.clone(), 4);

        let parent = tempfile::tempdir().unwrap();
        let child1_path = parent.path().join("child1");
        let child2_path = parent.path().join("child2");
        fs::create_dir_all(&child1_path).unwrap();
        fs::create_dir_all(&child2_path).unwrap();
        fs::write(child1_path.join("f.txt"), "a").unwrap();
        fs::write(child2_path.join("f.txt"), "b").unwrap();

        // Queue child1 as CurrentDir, child2 as UserSelected
        mgr.request_scan(child1_path.clone(), ScanPriority::CurrentDir).await;
        mgr.request_scan(child2_path.clone(), ScanPriority::UserSelected).await;

        // Cancel CurrentDir scans under parent
        mgr.cancel_current_dir_scans(parent.path()).await;

        // child2 (UserSelected) should remain active
        // child1 (CurrentDir) should be cancelled
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let active = mgr.active_count().await;
        // UserSelected scan should still be there (may have completed by now too)
        assert!(active <= 1, "only UserSelected should remain, got {active}");

        writer.shutdown();
    }

    #[tokio::test]
    async fn mark_full_scan_complete_cancels_everything() {
        let (writer, _dir) = setup_writer();
        let mgr = MicroScanManager::new(writer.clone(), 4);

        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("f.txt"), "x").unwrap();

        mgr.request_scan(root.path().to_path_buf(), ScanPriority::UserSelected)
            .await;
        mgr.mark_full_scan_complete().await;

        assert_eq!(mgr.active_count().await, 0);
        assert_eq!(mgr.queue_len().await, 0);

        // New requests should be skipped
        mgr.request_scan(root.path().to_path_buf(), ScanPriority::UserSelected)
            .await;
        assert_eq!(mgr.active_count().await, 0);

        writer.shutdown();
    }

    #[tokio::test]
    async fn cancel_all_clears_everything() {
        let (writer, _dir) = setup_writer();
        let mgr = MicroScanManager::new(writer.clone(), 4);

        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("f.txt"), "x").unwrap();

        mgr.request_scan(root.path().to_path_buf(), ScanPriority::UserSelected)
            .await;
        mgr.cancel_all().await;

        assert_eq!(mgr.active_count().await, 0);
        assert_eq!(mgr.queue_len().await, 0);

        writer.shutdown();
    }

    #[tokio::test]
    async fn priority_ordering_in_queue() {
        let (writer, _dir) = setup_writer();
        // max_concurrent = 0 forces everything into the queue for testing
        let mgr = MicroScanManager::new(writer.clone(), 0);

        let path_a = PathBuf::from("/tmp/test_micro_a");
        let path_b = PathBuf::from("/tmp/test_micro_b");
        let path_c = PathBuf::from("/tmp/test_micro_c");

        // Queue: CurrentDir, then UserSelected, then CurrentDir
        mgr.request_scan(path_a.clone(), ScanPriority::CurrentDir).await;
        mgr.request_scan(path_b.clone(), ScanPriority::UserSelected).await;
        mgr.request_scan(path_c.clone(), ScanPriority::CurrentDir).await;

        let inner = mgr.inner.lock().await;
        assert_eq!(inner.queue.len(), 3);

        // UserSelected should be first (highest priority)
        assert_eq!(inner.queue[0].0, ScanPriority::UserSelected);
        assert_eq!(inner.queue[0].1, path_b);

        // Then the two CurrentDir in FIFO order
        assert_eq!(inner.queue[1].0, ScanPriority::CurrentDir);
        assert_eq!(inner.queue[1].1, path_a);
        assert_eq!(inner.queue[2].0, ScanPriority::CurrentDir);
        assert_eq!(inner.queue[2].1, path_c);

        drop(inner);
        writer.shutdown();
    }

    #[test]
    fn is_child_of_checks() {
        assert!(is_child_of(Path::new("/Users/foo"), "/Users/foo"));
        assert!(is_child_of(Path::new("/Users/foo/bar"), "/Users/foo"));
        assert!(!is_child_of(Path::new("/Users/foobar"), "/Users/foo"));
        assert!(!is_child_of(Path::new("/Users"), "/Users/foo"));
    }

    #[test]
    fn enqueue_priority_ordering() {
        let mut queue = VecDeque::new();

        enqueue(&mut queue, PathBuf::from("/a"), ScanPriority::CurrentDir);
        enqueue(&mut queue, PathBuf::from("/b"), ScanPriority::UserSelected);
        enqueue(&mut queue, PathBuf::from("/c"), ScanPriority::CurrentDir);

        assert_eq!(queue[0].0, ScanPriority::UserSelected);
        assert_eq!(queue[1].0, ScanPriority::CurrentDir);
        assert_eq!(queue[2].0, ScanPriority::CurrentDir);
        assert_eq!(queue[1].1, PathBuf::from("/a")); // FIFO within same priority
        assert_eq!(queue[2].1, PathBuf::from("/c"));
    }
}
