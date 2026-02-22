//! Drive indexing module.
//!
//! Background-indexes local volumes into a per-volume SQLite database,
//! tracking every file and directory with recursive size aggregates.
//! See `docs/specs/drive-indexing/plan.md` for the full design.

// Infrastructure being built incrementally; callers come in later milestones
#![allow(unused, reason = "Skeleton module; public API consumers arrive in later milestones")]

pub mod aggregator;
pub mod firmlinks;
pub mod store;
pub mod writer;

mod micro_scan;
mod reconciler;
pub(crate) mod scanner;
mod verifier; // Placeholder: per-navigation background readdir diff (future milestone)
pub(crate) mod watcher;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

use micro_scan::{MicroScanManager, ScanPriority};
use reconciler::EventReconciler;
use scanner::ScanConfig;
use store::{DirStats, IndexStatus, IndexStore};
use watcher::DriveWatcher;
use writer::{IndexWriter, WriteMessage};

use crate::file_system::listing::FileEntry;

// ── Re-exports for commands ──────────────────────────────────────────

pub use micro_scan::ScanPriority as PubScanPriority;

// ── Global read-only index store for enrichment ──────────────────────

/// Global read-only index store, set when IndexManager is created.
/// Used by `enrich_entries_with_index` to avoid passing AppHandle through
/// the listing pipeline. Holds a separate read connection (WAL allows it).
/// Uses `std::sync::Mutex` (not `RwLock`) because `IndexStore` contains a
/// `rusqlite::Connection` which is `Send` but not `Sync`.
static GLOBAL_INDEX_STORE: LazyLock<std::sync::Mutex<Option<IndexStore>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

/// Set the global read-only index store. Called from `IndexManager::new`.
fn set_global_index_store(store: IndexStore) {
    if let Ok(mut guard) = GLOBAL_INDEX_STORE.lock() {
        *guard = Some(store);
    }
}

/// Clear the global read-only index store. Called on shutdown/clear.
fn clear_global_index_store() {
    if let Ok(mut guard) = GLOBAL_INDEX_STORE.lock() {
        *guard = None;
    }
}

/// Enrich directory entries with recursive size data from the index.
///
/// Called from `get_file_range` on every page fetch. Does nothing if
/// indexing is not initialized. For directories (non-symlinks), populates
/// `recursive_size`, `recursive_file_count`, and `recursive_dir_count`
/// from the `dir_stats` table via a batch SQLite read.
pub fn enrich_entries_with_index(entries: &mut [FileEntry]) {
    let guard = match GLOBAL_INDEX_STORE.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let store = match guard.as_ref() {
        Some(s) => s,
        None => return,
    };

    // Collect directory paths that need enrichment
    let dir_paths: Vec<String> = entries
        .iter()
        .filter(|e| e.is_directory && !e.is_symlink)
        .map(|e| firmlinks::normalize_path(&e.path))
        .collect();

    if dir_paths.is_empty() {
        return;
    }

    let refs: Vec<&str> = dir_paths.iter().map(String::as_str).collect();
    let stats = match store.get_dir_stats_batch(&refs) {
        Ok(s) => s,
        Err(e) => {
            log::debug!("Index enrichment failed: {e}");
            return;
        }
    };

    // Map normalized path -> DirStats for lookup
    let mut stats_map = std::collections::HashMap::new();
    for (path, stat) in dir_paths.iter().zip(stats.into_iter()) {
        if let Some(s) = stat {
            stats_map.insert(path.as_str(), s);
        }
    }

    // Apply to entries
    for entry in entries.iter_mut().filter(|e| e.is_directory && !e.is_symlink) {
        let normalized = firmlinks::normalize_path(&entry.path);
        if let Some(stats) = stats_map.get(normalized.as_str()) {
            entry.recursive_size = Some(stats.recursive_size);
            entry.recursive_file_count = Some(stats.recursive_file_count);
            entry.recursive_dir_count = Some(stats.recursive_dir_count);
        }
    }
}

// ── Event payloads (Rust -> Frontend) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexScanStartedEvent {
    pub volume_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexScanProgressEvent {
    pub volume_id: String,
    pub entries_scanned: u64,
    pub dirs_found: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexScanCompleteEvent {
    pub volume_id: String,
    pub total_entries: u64,
    pub total_dirs: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDirUpdatedEvent {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexReplayProgressEvent {
    pub volume_id: String,
    pub events_processed: u64,
    pub estimated_total: Option<u64>,
}

// ── Response types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatusResponse {
    pub initialized: bool,
    pub scanning: bool,
    pub entries_scanned: u64,
    pub dirs_found: u64,
    pub index_status: Option<IndexStatus>,
    pub db_file_size: Option<u64>,
}

// ── Global state ─────────────────────────────────────────────────────

/// Tauri-managed state wrapping the optional IndexManager.
///
/// Uses `std::sync::Mutex` (not tokio) because `IndexStore` holds a
/// `rusqlite::Connection` which is `Send` but not `Sync`. The std Mutex
/// provides the `Sync` guarantee needed for `tauri::State`.
pub struct IndexManagerState(pub std::sync::Mutex<Option<IndexManager>>);

// ── IndexManager ─────────────────────────────────────────────────────

/// Central coordinator for the drive indexing system.
///
/// Owns the SQLite store (reads), the writer thread (writes), the scanner handle,
/// and the micro-scan manager. Exposed to Tauri commands via `IndexManagerState`.
pub struct IndexManager {
    /// Volume ID (for example, "root" for /)
    volume_id: String,
    /// Volume root path
    volume_root: PathBuf,
    /// SQLite store for reads
    store: IndexStore,
    /// Writer handle for sending writes
    writer: IndexWriter,
    /// Micro-scan manager
    micro_scans: MicroScanManager,
    /// Handle to the active full scan (if running)
    scan_handle: Option<scanner::ScanHandle>,
    /// FSEvents watcher (started alongside scan, persists after scan completes)
    drive_watcher: Option<DriveWatcher>,
    /// Live event processing task (runs after reconciliation completes)
    live_event_task: Option<tauri::async_runtime::JoinHandle<()>>,
    /// Tauri app handle for emitting events
    app: AppHandle,
    /// Whether a full scan is currently running. Shared with the completion handler.
    scanning: Arc<AtomicBool>,
}

impl IndexManager {
    /// Create a new IndexManager for a volume.
    ///
    /// Opens (or creates) the SQLite database, spawns the writer thread,
    /// and sets up the micro-scan manager.
    pub fn new(volume_id: String, volume_root: PathBuf, app: AppHandle) -> Result<Self, String> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {e}"))?;

        // Ensure the data directory exists
        std::fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create app data dir: {e}"))?;

        let db_path = data_dir.join(format!("index-{volume_id}.db"));

        let store = IndexStore::open(&db_path).map_err(|e| format!("Failed to open index store: {e}"))?;

        let writer = IndexWriter::spawn(&db_path).map_err(|e| format!("Failed to spawn index writer: {e}"))?;

        let micro_scans = MicroScanManager::new(writer.clone(), 3);

        // Open a separate read connection for global enrichment (used by get_file_range)
        match IndexStore::open(&db_path) {
            Ok(global_store) => set_global_index_store(global_store),
            Err(e) => log::warn!("Failed to open global index read connection: {e}"),
        }

        log::info!(
            "IndexManager created for volume '{volume_id}' at {}",
            volume_root.display()
        );

        Ok(Self {
            volume_id,
            volume_root,
            store,
            writer,
            micro_scans,
            scan_handle: None,
            drive_watcher: None,
            live_event_task: None,
            app,
            scanning: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Resume from an existing index or start a fresh full scan.
    ///
    /// **If existing index exists** (`scan_completed_at` is set in meta):
    /// 1. Load the SQLite index immediately (already done on open)
    /// 2. Read `last_event_id` from meta table
    /// 3. Start FSEvents watcher with `sinceWhen = last_event_id`
    /// 4. FSEvents replays its journal -> events processed as live events
    /// 5. If journal unavailable (gap detected): fall back to full scan
    ///
    /// **If no existing index** (first launch or cleared):
    /// 1. Full scan via `start_scan()`
    pub fn resume_or_scan(&mut self) -> Result<(), String> {
        let status = self
            .store
            .get_index_status()
            .map_err(|e| format!("Failed to get index status: {e}"))?;

        // Check if we have a completed scan with a stored event ID
        if status.scan_completed_at.is_some() {
            if let Some(ref last_event_id_str) = status.last_event_id {
                let last_event_id: u64 = last_event_id_str.parse().unwrap_or(0);
                if last_event_id > 0 {
                    log::info!(
                        "Existing index found (scan_completed_at={}, last_event_id={last_event_id}), \
                         attempting sinceWhen replay",
                        status.scan_completed_at.as_deref().unwrap_or("?"),
                    );
                    return self.start_replay(last_event_id);
                }
            }
            log::info!("Existing index found but no last_event_id, starting fresh scan");
        } else {
            log::info!("No existing index (scan_completed_at not set), starting fresh scan");
        }

        self.start_scan()
    }

    /// Resume from an existing index by replaying FSEvents journal since `since_event_id`.
    ///
    /// Starts the watcher with `sinceWhen = since_event_id`. The watcher replays
    /// journal events which are processed as live events. If the journal is
    /// unavailable (gap detected), falls back to a full scan.
    fn start_replay(&mut self, since_event_id: u64) -> Result<(), String> {
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let current_id = watcher::current_event_id();

        match DriveWatcher::start(&self.volume_root, since_event_id, event_tx) {
            Ok(watcher) => {
                self.drive_watcher = Some(watcher);
                log::info!("DriveWatcher started for replay (sinceWhen={since_event_id}, current={current_id})");
            }
            Err(e) => {
                log::warn!("Failed to start DriveWatcher for replay: {e}, falling back to full scan");
                return self.start_scan();
            }
        }

        // Estimated total events for progress reporting (approximate: not all IDs
        // in the range belong to our volume)
        let estimated_total = if current_id > since_event_id {
            Some(current_id - since_event_id)
        } else {
            None
        };

        // Suppress micro-scans during replay to avoid sending writes into the
        // writer's active BEGIN IMMEDIATE transaction.
        self.micro_scans.set_replay_active(true);

        // Spawn the replay event processing loop
        let writer = self.writer.clone();
        let app = self.app.clone();
        let volume_id = self.volume_id.clone();
        let micro_scans = self.micro_scans.clone();

        // We need a way for the replay loop to signal "journal unavailable, need full scan".
        // Use a oneshot channel: if the replay detects a gap, it sends a signal.
        let (fallback_tx, fallback_rx) = tokio::sync::oneshot::channel::<()>();

        // Use tauri::async_runtime::spawn because indexing can start from the
        // synchronous Tauri setup() hook where no Tokio runtime context exists.
        tauri::async_runtime::spawn(async move {
            let result = run_replay_event_loop(
                event_rx,
                writer.clone(),
                app.clone(),
                ReplayConfig {
                    volume_id: volume_id.clone(),
                    since_event_id,
                    estimated_total,
                },
                fallback_tx,
                micro_scans,
            )
            .await;

            if let Err(e) = result {
                log::warn!("Replay event loop error: {e}");
            }
        });

        // Spawn a task that watches for the fallback signal and triggers a full scan if needed.
        // This runs on the IndexManagerState through Tauri's managed state.
        let app_fallback = self.app.clone();
        tauri::async_runtime::spawn(async move {
            if fallback_rx.await.is_ok() {
                log::warn!("Journal replay detected gap, initiating full scan fallback");
                // Re-acquire the IndexManager through Tauri state and start a fresh scan
                let state = app_fallback.state::<IndexManagerState>();
                let mut guard = match state.0.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        log::warn!("Failed to lock state for fallback scan: {e}");
                        return;
                    }
                };
                if let Some(mgr) = guard.as_mut() {
                    // Stop the current watcher (replay detected it's useless)
                    if let Some(ref mut watcher) = mgr.drive_watcher {
                        watcher.stop();
                    }
                    mgr.drive_watcher = None;
                    if let Some(task) = mgr.live_event_task.take() {
                        task.abort();
                    }

                    if let Err(e) = mgr.start_scan() {
                        log::warn!("Fallback full scan failed: {e}");
                    }
                }
            }
        });

        Ok(())
    }

    /// Start a full volume scan with concurrent FSEvents watching.
    ///
    /// Flow:
    /// 1. Start DriveWatcher (sinceWhen=0) to buffer events during the scan
    /// 2. Record scan-start event ID
    /// 3. Start the full scan
    /// 4. On scan completion: replay buffered events, switch to live mode
    /// 5. Live events processed continuously until shutdown
    pub fn start_scan(&mut self) -> Result<(), String> {
        if self.scanning.load(Ordering::Relaxed) {
            return Err("Scan already running".to_string());
        }

        // Step 1: Start the FSEvents watcher BEFORE the scan so we don't miss events
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let scan_start_event_id = watcher::current_event_id();

        match DriveWatcher::start(&self.volume_root, 0, event_tx) {
            Ok(watcher) => {
                self.drive_watcher = Some(watcher);
                log::info!("DriveWatcher started (scan_start_event_id={scan_start_event_id})");
            }
            Err(e) => {
                // Watcher failure is non-fatal: scan works without it, just no live updates
                log::warn!("Failed to start DriveWatcher (scan will proceed without watcher): {e}");
            }
        }

        // Emit started event
        let _ = self.app.emit(
            "index-scan-started",
            IndexScanStartedEvent {
                volume_id: self.volume_id.clone(),
            },
        );

        // Step 2: Start the full scan
        let config = ScanConfig {
            root: self.volume_root.clone(),
            ..ScanConfig::default()
        };

        let (scan_handle, join_handle) =
            scanner::scan_volume(config, &self.writer).map_err(|e| format!("Failed to start scan: {e}"))?;

        self.scanning.store(true, Ordering::Relaxed);

        // Shared flag: set to true when the scan finishes (or fails/panics), so the
        // progress reporter can exit its loop.
        let scan_done = Arc::new(AtomicBool::new(false));

        // Spawn progress reporter (polls every 500ms, exits when scan_done is set).
        // Use tauri::async_runtime::spawn because indexing can start from the
        // synchronous Tauri setup() hook where no Tokio runtime context exists.
        let progress = Arc::clone(&scan_handle.progress);
        let volume_id_progress = self.volume_id.clone();
        let app_progress = self.app.clone();
        let scan_done_progress = Arc::clone(&scan_done);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if scan_done_progress.load(Ordering::Relaxed) {
                    break;
                }
                let (entries, dirs) = progress.snapshot();
                let _ = app_progress.emit(
                    "index-scan-progress",
                    IndexScanProgressEvent {
                        volume_id: volume_id_progress.clone(),
                        entries_scanned: entries,
                        dirs_found: dirs,
                    },
                );
            }
        });

        // Step 3: Spawn completion handler that also does reconciliation.
        // Use tauri::async_runtime::spawn because indexing can start from the
        // synchronous Tauri setup() hook where no Tokio runtime context exists.
        let volume_id = self.volume_id.clone();
        let app = self.app.clone();
        let writer = self.writer.clone();
        let micro_scans = self.micro_scans.clone();
        let scanning = Arc::clone(&self.scanning);
        tauri::async_runtime::spawn(async move {
            // Wait for scan to complete
            let join_result = tokio::task::spawn_blocking(move || join_handle.join()).await;

            // Signal the progress reporter to stop regardless of outcome
            scan_done.store(true, Ordering::Relaxed);
            // Reset scanning flag so get_status() reports correctly and new scans can start
            scanning.store(false, Ordering::Relaxed);

            // Flatten the outer Result (from spawn_blocking) and inner Result (from thread join)
            let result = match join_result {
                Ok(thread_result) => thread_result,
                Err(e) => {
                    log::warn!("Completion handler task failed: {e}");
                    return;
                }
            };

            match result {
                Ok(Ok(summary)) => {
                    log::info!(
                        "Volume scan complete: {} entries, {} dirs, {}ms",
                        summary.total_entries,
                        summary.total_dirs,
                        summary.duration_ms,
                    );

                    // Step 4: Reconcile buffered watcher events
                    let mut reconciler = EventReconciler::new();

                    // Drain all buffered events from the channel into the reconciler
                    let mut event_rx = event_rx;
                    let mut buffered_count = 0u64;
                    while let Ok(event) = event_rx.try_recv() {
                        reconciler.buffer_event(event);
                        buffered_count += 1;
                    }
                    log::info!("Reconciler: buffered {buffered_count} events during scan");

                    // Replay events that arrived after the scan read their paths
                    match reconciler.replay(scan_start_event_id, &writer, &app) {
                        Ok(last_id) => {
                            log::info!("Reconciler: replay complete (last_event_id={last_id})");
                        }
                        Err(e) => {
                            log::warn!("Reconciler: replay failed: {e}");
                        }
                    }

                    // Switch to live mode
                    reconciler.switch_to_live();

                    // Step 5: Start live event processing loop
                    let writer_live = writer.clone();
                    let app_live = app.clone();
                    tauri::async_runtime::spawn(async move {
                        run_live_event_loop(event_rx, reconciler, writer_live, app_live).await;
                    });

                    // Emit completion event
                    let _ = app.emit(
                        "index-scan-complete",
                        IndexScanCompleteEvent {
                            volume_id: volume_id.clone(),
                            total_entries: summary.total_entries,
                            total_dirs: summary.total_dirs,
                            duration_ms: summary.duration_ms,
                        },
                    );

                    // Store scan metadata via writer
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs().to_string())
                        .unwrap_or_default();
                    let _ = writer.send(WriteMessage::UpdateMeta {
                        key: "scan_completed_at".to_string(),
                        value: now,
                    });
                    let _ = writer.send(WriteMessage::UpdateMeta {
                        key: "scan_duration_ms".to_string(),
                        value: summary.duration_ms.to_string(),
                    });
                    let _ = writer.send(WriteMessage::UpdateMeta {
                        key: "total_entries".to_string(),
                        value: summary.total_entries.to_string(),
                    });
                    let _ = writer.send(WriteMessage::UpdateMeta {
                        key: "volume_path".to_string(),
                        value: "/".to_string(),
                    });

                    // Mark micro-scans as superseded (full scan data is authoritative)
                    micro_scans.mark_full_scan_complete().await;
                }
                Ok(Err(e)) => {
                    log::warn!("Volume scan failed: {e}");
                }
                Err(_) => {
                    log::warn!("Volume scan thread panicked");
                }
            }
        });

        self.scan_handle = Some(scan_handle);
        Ok(())
    }

    /// Stop the active full scan, watcher, and all micro-scans.
    pub fn stop_scan(&mut self) {
        if let Some(ref handle) = self.scan_handle {
            handle.cancel();
        }
        self.scan_handle = None;
        self.scanning.store(false, Ordering::Relaxed);

        // Stop the FSEvents watcher
        if let Some(ref mut watcher) = self.drive_watcher {
            watcher.stop();
        }
        self.drive_watcher = None;

        // Abort the live event processing task
        if let Some(task) = self.live_event_task.take() {
            task.abort();
        }

        let micro_scans = self.micro_scans.clone();
        tauri::async_runtime::spawn(async move {
            micro_scans.cancel_all().await;
        });
    }

    /// Get the current index status.
    pub fn get_status(&self) -> Result<IndexStatusResponse, String> {
        let index_status = self
            .store
            .get_index_status()
            .map_err(|e| format!("Failed to get index status: {e}"))?;

        let db_file_size = self.store.db_file_size().ok();

        let (entries_scanned, dirs_found) = self
            .scan_handle
            .as_ref()
            .map(|h| h.progress.snapshot())
            .unwrap_or((0, 0));

        Ok(IndexStatusResponse {
            initialized: true,
            scanning: self.scanning.load(Ordering::Relaxed),
            entries_scanned,
            dirs_found,
            index_status: Some(index_status),
            db_file_size,
        })
    }

    /// Look up recursive stats for a single directory.
    pub fn get_dir_stats(&self, path: &str) -> Result<Option<DirStats>, String> {
        let normalized = firmlinks::normalize_path(path);
        self.store
            .get_dir_stats(&normalized)
            .map_err(|e| format!("Failed to get dir stats: {e}"))
    }

    /// Batch lookup of dir_stats for multiple paths.
    pub fn get_dir_stats_batch(&self, paths: &[String]) -> Result<Vec<Option<DirStats>>, String> {
        let normalized: Vec<String> = paths.iter().map(|p| firmlinks::normalize_path(p)).collect();
        let refs: Vec<&str> = normalized.iter().map(String::as_str).collect();
        self.store
            .get_dir_stats_batch(&refs)
            .map_err(|e| format!("Failed to get dir stats batch: {e}"))
    }

    /// Request a priority micro-scan for a directory.
    pub fn prioritize_dir(&self, path: &str, priority: ScanPriority) {
        let normalized = firmlinks::normalize_path(path);
        let path_buf = PathBuf::from(normalized);
        let micro_scans = self.micro_scans.clone();
        tauri::async_runtime::spawn(async move {
            micro_scans.request_scan(path_buf, priority).await;
        });
    }

    /// Cancel current-directory micro-scans (called on navigate-away).
    pub fn cancel_nav_priority(&self, path: &str) {
        let normalized = firmlinks::normalize_path(path);
        let path_buf = PathBuf::from(normalized);
        let micro_scans = self.micro_scans.clone();
        tauri::async_runtime::spawn(async move {
            micro_scans.cancel_current_dir_scans(&path_buf).await;
        });
    }

    /// Return the DB file path for this index.
    pub fn db_path(&self) -> &Path {
        self.store.db_path()
    }

    /// Shut down the indexing system. Cancels all scans and stops the writer.
    pub fn shutdown(&mut self) {
        self.stop_scan();
        self.writer.shutdown();
        clear_global_index_store();
        log::info!("IndexManager shut down for volume '{}'", self.volume_id);
    }
}

// ── Live event loop ──────────────────────────────────────────────────

/// Process FSEvents in real time after scan + reconciliation completes.
///
/// Runs as a tokio task, reading events from the watcher channel and processing
/// them through the reconciler. Batches `index-dir-updated` notifications with
/// a 300 ms flush interval to avoid UI flicker from rapid per-event emits.
/// Exits when the channel closes (watcher stopped).
async fn run_live_event_loop(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<watcher::FsChangeEvent>,
    mut reconciler: EventReconciler,
    writer: IndexWriter,
    app: AppHandle,
) {
    log::info!("Live event processing started");
    let mut event_count = 0u64;
    let mut pending_paths = std::collections::HashSet::<String>::new();
    let mut flush_interval = tokio::time::interval(Duration::from_millis(300));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        reconciler.process_live_event(&event, &writer, &mut pending_paths);
                        event_count += 1;
                        if event_count.is_multiple_of(10_000) {
                            log::info!("Live event processing: {event_count} events processed so far");
                        }
                    }
                    None => {
                        // Channel closed: flush remaining paths before exit
                        if !pending_paths.is_empty() {
                            reconciler::emit_dir_updated(&app, pending_paths.drain().collect());
                        }
                        break;
                    }
                }
            }
            _ = flush_interval.tick() => {
                if !pending_paths.is_empty() {
                    reconciler::emit_dir_updated(&app, pending_paths.drain().collect());
                }
            }
        }
    }

    log::info!("Live event processing stopped after {event_count} events");
}

// ── Replay event loop (cold start sinceWhen) ─────────────────────────

/// Threshold for detecting a journal gap. If the first event ID received is
/// more than this many IDs ahead of the stored `since_event_id`, we consider
/// the journal unavailable and fall back to a full scan.
const JOURNAL_GAP_THRESHOLD: u64 = 1_000_000;

/// Configuration for a replay event loop.
struct ReplayConfig {
    volume_id: String,
    since_event_id: u64,
    estimated_total: Option<u64>,
}

/// Process FSEvents replayed from the journal on cold start.
///
/// Two-phase approach to avoid a race condition where `index-dir-updated`
/// notifications fire before the writer commits replay data to SQLite:
///
/// **Phase 1 (replay):** Process events via `process_fs_event` directly,
/// collecting affected parent paths in a `HashSet`. No per-event UI
/// notification. `UpdateLastEventId` sent every 1000 events to reduce
/// writer load.
///
/// **Phase 2 (after HistoryDone):** Send final `UpdateLastEventId`, flush
/// the writer (wait for all prior messages to commit), then emit a single
/// batched `index-dir-updated`. After that, continue processing live events
/// with per-event emit (live events arrive slowly enough for the writer to
/// keep up).
///
/// If a journal gap is detected (first event ID >> stored last_event_id),
/// sends a signal via `fallback_tx` to trigger a full scan.
async fn run_replay_event_loop(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<watcher::FsChangeEvent>,
    writer: IndexWriter,
    app: AppHandle,
    config: ReplayConfig,
    fallback_tx: tokio::sync::oneshot::Sender<()>,
    micro_scans: MicroScanManager,
) -> Result<(), String> {
    let ReplayConfig {
        volume_id,
        since_event_id,
        estimated_total,
    } = config;
    log::info!("Replay event processing started (since_event_id={since_event_id})");

    let mut event_count = 0u64;
    let mut first_event_checked = false;
    let mut fallback_tx = Some(fallback_tx);
    let mut last_event_id = since_event_id;

    // Collect all affected parent paths during replay (deduplicated)
    let mut affected_paths = std::collections::HashSet::<String>::new();

    // MustScanSubDirs paths to queue after replay
    let mut pending_rescans = Vec::<String>::new();

    // Progress reporting interval
    let mut last_progress = std::time::Instant::now();
    let replay_start = std::time::Instant::now();

    // Wrap all replay writes in a single SQLite transaction.
    // Without this, each write is auto-committed (separate fsync), making
    // 50K+ writes take minutes. With a transaction, it takes seconds.
    if let Err(e) = writer.send(WriteMessage::BeginTransaction) {
        log::warn!("Replay: BeginTransaction send failed: {e}");
    }

    // ── Phase 1: Replay (before HistoryDone) ─────────────────────────

    while let Some(event) = event_rx.recv().await {
        // Check for journal gap on the first event
        if !first_event_checked {
            first_event_checked = true;
            if event.event_id > since_event_id + JOURNAL_GAP_THRESHOLD {
                log::warn!(
                    "Journal gap detected: stored last_event_id={since_event_id}, \
                     first received event_id={}, gap={}",
                    event.event_id,
                    event.event_id - since_event_id,
                );
                // Re-enable micro-scans before falling back to full scan
                micro_scans.set_replay_active(false);
                if let Some(tx) = fallback_tx.take() {
                    let _ = tx.send(());
                }
                return Ok(());
            }
            log::info!(
                "Replay: first event_id={}, gap from stored={}, journal appears available",
                event.event_id,
                event.event_id.saturating_sub(since_event_id),
            );
        }

        // HistoryDone marks end of replay phase
        if event.flags.history_done {
            log::info!("Replay: HistoryDone received after {event_count} events");

            // Process the HistoryDone event itself (it may carry other flags)
            if let Some(paths) = reconciler::process_fs_event(&event, &writer) {
                affected_paths.extend(paths);
            }
            last_event_id = event.event_id;
            event_count += 1;

            break; // Exit Phase 1, enter Phase 2
        }

        // Handle MustScanSubDirs: queue for after replay (don't start during replay)
        if event.flags.must_scan_sub_dirs {
            let normalized = firmlinks::normalize_path(&event.path);
            pending_rescans.push(normalized);
            last_event_id = event.event_id;
            event_count += 1;
            continue;
        }

        // Process event and collect affected paths
        if let Some(paths) = reconciler::process_fs_event(&event, &writer) {
            affected_paths.extend(paths);
        }

        last_event_id = event.event_id;
        event_count += 1;

        // Batch UpdateLastEventId every 1000 events (reduces writer load ~10x)
        if event_count.is_multiple_of(1000)
            && let Err(e) = writer.send(WriteMessage::UpdateLastEventId(last_event_id))
        {
            log::warn!("Replay: UpdateLastEventId send failed: {e}");
        }

        // Emit progress every 500ms during replay
        if last_progress.elapsed() >= Duration::from_millis(500) {
            let _ = app.emit(
                "index-replay-progress",
                IndexReplayProgressEvent {
                    volume_id: volume_id.clone(),
                    events_processed: event_count,
                    estimated_total,
                },
            );
            last_progress = std::time::Instant::now();
        }

        // Log milestone counts
        if event_count.is_multiple_of(10_000) {
            log::info!("Replay: {event_count} events processed so far");
        }
    }

    // ── Phase 2: After HistoryDone ───────────────────────────────────

    // Send final UpdateLastEventId
    if last_event_id > since_event_id
        && let Err(e) = writer.send(WriteMessage::UpdateLastEventId(last_event_id))
    {
        log::warn!("Replay: final UpdateLastEventId send failed: {e}");
    }

    // Commit the replay transaction (all writes become visible in one fsync)
    if let Err(e) = writer.send(WriteMessage::CommitTransaction) {
        log::warn!("Replay: CommitTransaction send failed: {e}");
    }

    // Flush: wait for the writer to commit all replay data
    let flush_start = std::time::Instant::now();
    match writer.flush().await {
        Ok(()) => {
            let flush_ms = flush_start.elapsed().as_millis();
            log::info!(
                "Replay complete: {event_count} events, {} affected dirs, {flush_ms}ms writer flush, \
                 {}ms total",
                affected_paths.len(),
                replay_start.elapsed().as_millis(),
            );
        }
        Err(e) => {
            log::warn!("Replay: flush failed (writer may have shut down): {e}");
        }
    }

    // Emit final progress
    let _ = app.emit(
        "index-replay-progress",
        IndexReplayProgressEvent {
            volume_id: volume_id.clone(),
            events_processed: event_count,
            estimated_total,
        },
    );

    // Emit a single batched index-dir-updated with all collected paths
    if !affected_paths.is_empty() {
        reconciler::emit_dir_updated(&app, affected_paths.iter().cloned().collect());
    }

    // ── Switch to live mode immediately (before verification) ────────

    log::info!("Replay: switching to live mode");
    micro_scans.set_replay_active(false);
    log::debug!("Replay: micro-scans re-enabled for live mode");
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    // Spawn background verification: runs concurrently with live events.
    // The writer serializes all writes, so this is safe.
    let verify_writer = writer.clone();
    let verify_app = app.clone();
    tauri::async_runtime::spawn(async move {
        run_background_verification(affected_paths, verify_writer, verify_app).await;
    });

    // Queue any MustScanSubDirs rescans that were deferred during replay
    for path in pending_rescans {
        reconciler.queue_must_scan_sub_dirs(PathBuf::from(path), &writer);
    }

    let mut live_count = 0u64;
    let mut live_pending_paths = std::collections::HashSet::<String>::new();
    let mut flush_interval = tokio::time::interval(Duration::from_millis(300));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        reconciler.process_live_event(&event, &writer, &mut live_pending_paths);
                        live_count += 1;
                        if live_count.is_multiple_of(10_000) {
                            log::info!("Live event processing (post-replay): {live_count} events");
                        }
                    }
                    None => {
                        if !live_pending_paths.is_empty() {
                            reconciler::emit_dir_updated(&app, live_pending_paths.drain().collect());
                        }
                        break;
                    }
                }
            }
            _ = flush_interval.tick() => {
                if !live_pending_paths.is_empty() {
                    reconciler::emit_dir_updated(&app, live_pending_paths.drain().collect());
                }
            }
        }
    }

    log::info!("Replay event loop stopped ({event_count} replay + {live_count} live events)");
    Ok(())
}

// ── Background verification ──────────────────────────────────────────

/// Run post-replay verification in the background.
///
/// Called after live mode starts so the app is responsive immediately.
/// Corrections found by verification go through the writer channel,
/// which serializes them with live writes.
async fn run_background_verification(
    affected_paths: std::collections::HashSet<String>,
    writer: IndexWriter,
    app: AppHandle,
) {
    let verify_start = std::time::Instant::now();
    log::info!(
        "Background verification started ({} affected dirs)",
        affected_paths.len(),
    );

    // Verify affected directories: FSEvents journal replay coalesces events,
    // so child deletions may only show as "parent dir modified," and new
    // children may not get individual creation events. Readdir each affected
    // parent and reconcile with DB.
    let verify_result = verify_affected_dirs(&affected_paths, &writer);

    // Scan newly discovered directories (inserts children + computes subtree aggregates)
    if !verify_result.new_dir_paths.is_empty() {
        let cancelled = AtomicBool::new(false);
        for dir_path in &verify_result.new_dir_paths {
            match scanner::scan_subtree(Path::new(dir_path), &writer, &cancelled) {
                Ok(summary) => {
                    log::debug!(
                        "Background verification: scanned new dir {dir_path} ({} entries, {}ms)",
                        summary.total_entries,
                        summary.duration_ms,
                    );
                }
                Err(e) => {
                    log::warn!("Background verification: scan_subtree({dir_path}) failed: {e}");
                }
            }
        }
    }

    let has_changes = verify_result.stale_count > 0
        || verify_result.new_file_count > 0
        || !verify_result.new_dir_paths.is_empty();

    if has_changes {
        log::info!(
            "Background verification found {} stale, {} new files, {} new dirs; flushing",
            verify_result.stale_count,
            verify_result.new_file_count,
            verify_result.new_dir_paths.len(),
        );
        if let Err(e) = writer.flush().await {
            log::warn!("Background verification flush failed: {e}");
        }

        // For new directories, propagate their subtree totals up the ancestor chain.
        // scan_subtree computes aggregates within the subtree but doesn't propagate
        // upward. Read the computed dir_stats and send PropagateDelta.
        if !verify_result.new_dir_paths.is_empty() {
            // Scope the mutex guard so it's dropped before the .await below
            {
                let guard = GLOBAL_INDEX_STORE.lock();
                if let Ok(ref guard) = guard
                    && let Some(store) = guard.as_ref()
                {
                    for dir_path in &verify_result.new_dir_paths {
                        if let Ok(Some(stats)) = store.get_dir_stats(dir_path) {
                            let _ = writer.send(WriteMessage::PropagateDelta {
                                path: PathBuf::from(dir_path),
                                size_delta: stats.recursive_size as i64,
                                file_count_delta: stats.recursive_file_count as i32,
                                dir_count_delta: (stats.recursive_dir_count as i32) + 1,
                            });
                        } else {
                            let _ = writer.send(WriteMessage::PropagateDelta {
                                path: PathBuf::from(dir_path),
                                size_delta: 0,
                                file_count_delta: 0,
                                dir_count_delta: 1,
                            });
                        }
                    }
                }
            }

            if let Err(e) = writer.flush().await {
                log::warn!("Background verification propagation flush failed: {e}");
            }
        }

        // Emit index-dir-updated for any corrected paths so the UI refreshes
        let mut corrected_paths: Vec<String> = affected_paths.into_iter().collect();
        corrected_paths.extend(verify_result.new_dir_paths.iter().cloned());
        reconciler::emit_dir_updated(&app, corrected_paths);
    }

    log::info!(
        "Background verification completed in {}ms",
        verify_start.elapsed().as_millis(),
    );
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Result of `verify_affected_dirs`.
struct VerifyResult {
    /// Entries in DB but not on disk (deleted).
    stale_count: u64,
    /// Files on disk but not in DB (inserted with delta propagation).
    new_file_count: u64,
    /// Directories on disk but not in DB (inserted, need subtree scan by caller).
    new_dir_paths: Vec<String>,
}

/// Verify that DB entries for affected directories match what's on disk.
///
/// FSEvents journal replay coalesces events: child deletions may appear as
/// "parent directory modified" without individual removal events. Similarly,
/// new children may not get individual creation events.
///
/// For each affected parent this function:
/// 1. **Stale entries**: DB children that no longer exist on disk get
///    `DeleteEntry`/`DeleteSubtree` (auto-propagates deltas).
/// 2. **Missing entries**: Disk children not in DB get `UpsertEntry`.
///    New files also get `PropagateDelta`. New directories are collected
///    in `new_dir_paths` for the caller to scan via `scan_subtree`.
fn verify_affected_dirs(affected_paths: &std::collections::HashSet<String>, writer: &IndexWriter) -> VerifyResult {
    let guard = match GLOBAL_INDEX_STORE.lock() {
        Ok(g) => g,
        Err(_) => {
            return VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            };
        }
    };
    let store = match guard.as_ref() {
        Some(s) => s,
        None => {
            return VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            };
        }
    };

    let mut stale_count = 0u64;
    let mut new_file_count = 0u64;
    let mut new_dir_paths = Vec::<String>::new();

    for parent_path in affected_paths {
        let db_children = match store.list_entries_by_parent(parent_path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        // Build a set of known DB child paths for fast lookup
        let db_child_paths: std::collections::HashSet<&str> = db_children.iter().map(|c| c.path.as_str()).collect();

        // Phase 1: detect stale entries (in DB but not on disk)
        for child in &db_children {
            if !Path::new(&child.path).exists() {
                if child.is_directory {
                    let _ = writer.send(WriteMessage::DeleteSubtree(child.path.clone()));
                } else {
                    let _ = writer.send(WriteMessage::DeleteEntry(child.path.clone()));
                }
                stale_count += 1;
            }
        }

        // Phase 2: detect missing entries (on disk but not in DB)
        let read_dir = match std::fs::read_dir(parent_path) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        for dir_entry in read_dir.flatten() {
            let child_path = dir_entry.path();
            let child_path_str = child_path.to_string_lossy().to_string();
            let normalized = firmlinks::normalize_path(&child_path_str);

            if db_child_paths.contains(normalized.as_str()) {
                continue;
            }

            let metadata = match std::fs::symlink_metadata(&child_path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_dir = metadata.is_dir();
            let is_symlink = metadata.is_symlink();
            let name = dir_entry.file_name().to_string_lossy().to_string();

            let (size, modified_at) = if is_dir || is_symlink {
                (None, reconciler::entry_modified_at(&metadata))
            } else {
                reconciler::entry_size_and_mtime(&metadata)
            };

            let entry = store::ScannedEntry {
                path: normalized.clone(),
                parent_path: parent_path.clone(),
                name,
                is_directory: is_dir,
                is_symlink,
                size,
                modified_at,
            };

            let _ = writer.send(WriteMessage::UpsertEntry(entry));

            if is_dir {
                new_dir_paths.push(normalized);
            } else if let Some(sz) = size {
                let _ = writer.send(WriteMessage::PropagateDelta {
                    path: PathBuf::from(&normalized),
                    size_delta: sz as i64,
                    file_count_delta: 1,
                    dir_count_delta: 0,
                });
                new_file_count += 1;
            }
        }
    }

    if stale_count > 0 || new_file_count > 0 || !new_dir_paths.is_empty() {
        log::info!(
            "Replay verification: {stale_count} stale, {new_file_count} new files, \
             {} new dirs across {} affected dirs",
            new_dir_paths.len(),
            affected_paths.len(),
        );
    }

    VerifyResult {
        stale_count,
        new_file_count,
        new_dir_paths,
    }
}

// ── Initialization ───────────────────────────────────────────────────

/// Register the `IndexManagerState` in Tauri's managed state.
///
/// Call during app setup. Does NOT start scanning (that requires explicit
/// `start_indexing()` or the dev env var `CMDR_DRIVE_INDEX=1`).
pub fn init(app: &AppHandle) {
    app.manage(IndexManagerState(std::sync::Mutex::new(None)));
    log::info!("Indexing state registered");
}

/// Whether indexing should auto-start on launch.
///
/// - If settings say disabled (`indexing_enabled == Some(false)`): never auto-start.
/// - **Dev builds** (`debug_assertions`): requires `CMDR_DRIVE_INDEX=1` env var.
/// - **Release builds**: auto-start by default.
pub fn should_auto_start(indexing_enabled: Option<bool>) -> bool {
    // User explicitly disabled indexing in settings
    if indexing_enabled == Some(false) {
        return false;
    }

    if cfg!(debug_assertions) {
        std::env::var("CMDR_DRIVE_INDEX").is_ok_and(|v| v == "1")
    } else {
        // Default true (setting not yet stored means first launch, enabled by default)
        true
    }
}

/// Stop all scans, watcher, and micro-scans without deleting the DB.
///
/// Called when the user disables indexing via settings. The index stays on disk
/// but no scanning or watching runs. Directory sizes revert to `<dir>`.
pub fn stop_indexing(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<IndexManagerState>();
    let mut guard = state.0.lock().map_err(|e| format!("Failed to lock state: {e}"))?;

    if let Some(mut mgr) = guard.take() {
        mgr.shutdown();
        log::info!("Indexing stopped (DB preserved on disk)");
    }

    Ok(())
}

/// Create the IndexManager for the root volume, store it in managed state,
/// and auto-start indexing (resume from existing index or fresh scan).
///
/// Call after `init()`. On startup this checks for an existing index: if found,
/// it replays the FSEvents journal from the stored `last_event_id`; otherwise
/// it starts a fresh full scan.
pub fn start_indexing(app: &AppHandle) -> Result<(), String> {
    let mut manager = IndexManager::new("root".to_string(), PathBuf::from("/"), app.clone())?;

    // Resume from existing index or start a fresh scan
    manager.resume_or_scan()?;

    let state = app.state::<IndexManagerState>();
    let mut guard = state.0.lock().map_err(|e| format!("Failed to lock state: {e}"))?;
    *guard = Some(manager);

    log::info!("Indexing system initialized for root volume");
    Ok(())
}

/// Stop all scans, shut down the writer, delete the DB file, and reset state.
///
/// After this call the `IndexManagerState` holds `None`. Call `start_indexing()`
/// to create a fresh index.
pub fn clear_index(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<IndexManagerState>();
    let mut guard = state.0.lock().map_err(|e| format!("Failed to lock state: {e}"))?;

    if let Some(mut mgr) = guard.take() {
        let db_path = mgr.db_path().to_path_buf();
        mgr.shutdown();

        // Delete DB file and WAL/SHM sidecars
        for path in [
            db_path.clone(),
            db_path.with_extension("db-wal"),
            db_path.with_extension("db-shm"),
        ] {
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| format!("Failed to delete {}: {e}", path.display()))?;
            }
        }
        log::info!("Drive index cleared (DB deleted)");
    } else {
        log::info!("Drive index clear requested but indexing was not initialized");
    }

    Ok(())
}
