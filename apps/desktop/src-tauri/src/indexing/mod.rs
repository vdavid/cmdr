//! Drive indexing module.
//!
//! Background-indexes local volumes into a per-volume SQLite database,
//! tracking every file and directory with recursive size aggregates.
//! See `docs/specs/drive-indexing/plan.md` for the full design.

pub mod aggregator;
pub mod firmlinks;
pub mod path_resolver;
pub mod store;
pub mod writer;

mod micro_scan;
mod reconciler;
pub(crate) mod scanner;
mod verifier; // Placeholder: per-navigation background readdir diff (future milestone)
pub(crate) mod watcher;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use micro_scan::{MicroScanManager, ScanPriority};
use path_resolver::PathResolver;
use reconciler::EventReconciler;
use scanner::ScanConfig;
use serde::{Deserialize, Serialize};
use store::{DirStats, IndexStatus, IndexStore};
use tauri::{AppHandle, Emitter, Manager};
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
/// indexing is not initialized. Uses `try_lock` to avoid blocking the
/// listing pipeline when background verification holds the lock at startup.
/// Skipped enrichment is retried on subsequent fetches.
///
/// **Integer-keyed optimization**: Instead of resolving each directory path
/// individually, resolves the common parent directory once, gets all child
/// dir `(id, name)` pairs via `idx_parent_name`, then batch-fetches their
/// `dir_stats` by integer IDs. Two indexed queries total.
pub fn enrich_entries_with_index(entries: &mut [FileEntry]) {
    let guard = match GLOBAL_INDEX_STORE.try_lock() {
        Ok(g) => g,
        Err(_) => {
            log::debug!("Index enrichment skipped: store lock is held (background verification likely in progress)");
            return;
        }
    };
    let store = match guard.as_ref() {
        Some(s) => s,
        None => return,
    };

    // Find directory entries that need enrichment
    let has_dirs = entries.iter().any(|e| e.is_directory && !e.is_symlink);
    if !has_dirs {
        return;
    }

    // Determine the common parent directory from the first directory entry.
    // All entries in a listing share the same parent (they're siblings).
    let parent_path = match entries.iter().find(|e| e.is_directory && !e.is_symlink) {
        Some(e) => {
            let normalized = firmlinks::normalize_path(&e.path);
            // Parent = path without the last component
            match normalized.rfind('/') {
                Some(0) => "/".to_string(),
                Some(pos) => normalized[..pos].to_string(),
                None => return, // Malformed path, skip
            }
        }
        None => return,
    };

    // Use the integer-keyed fast path: resolve parent once, batch-fetch child stats
    if let Err(e) = enrich_via_parent_id(entries, store, &parent_path) {
        log::debug!("Index enrichment (integer-keyed) failed: {e}, trying path-based fallback");
        // Fallback: resolve each path individually (handles mixed-parent edge cases)
        enrich_via_individual_paths(entries, store);
    }
}

/// Fast path: resolve parent dir → id, get child dir IDs, batch-fetch stats.
fn enrich_via_parent_id(entries: &mut [FileEntry], store: &IndexStore, parent_path: &str) -> Result<(), String> {
    let conn = store.read_conn();

    // Resolve parent directory path → entry ID (one tree walk, almost always cached)
    let parent_id = match store::resolve_path(conn, parent_path).map_err(|e| format!("{e}"))? {
        Some(id) => id,
        None => return Err(format!("Parent path not found in index: {parent_path}")),
    };

    // Get all child directory (id, name) pairs
    let child_dirs = IndexStore::list_child_dir_ids_and_names(conn, parent_id).map_err(|e| format!("{e}"))?;

    if child_dirs.is_empty() {
        return Ok(());
    }

    // Batch-fetch dir_stats by integer IDs
    let child_ids: Vec<i64> = child_dirs.iter().map(|(id, _)| *id).collect();
    let stats_batch = IndexStore::get_dir_stats_batch_by_ids(conn, &child_ids).map_err(|e| format!("{e}"))?;

    // Build name → DirStatsById map (using normalized names for matching)
    let mut name_to_stats: std::collections::HashMap<String, store::DirStatsById> =
        std::collections::HashMap::with_capacity(child_dirs.len());
    for ((_, name), stats_opt) in child_dirs.into_iter().zip(stats_batch) {
        if let Some(stats) = stats_opt {
            name_to_stats.insert(store::normalize_for_comparison(&name), stats);
        }
    }

    // Apply stats to entries by matching normalized basenames
    for entry in entries.iter_mut().filter(|e| e.is_directory && !e.is_symlink) {
        let basename = match entry.path.rfind('/') {
            Some(pos) => &entry.path[pos + 1..],
            None => &entry.path,
        };
        let normalized_name = store::normalize_for_comparison(basename);
        if let Some(stats) = name_to_stats.get(&normalized_name) {
            entry.recursive_size = Some(stats.recursive_size);
            entry.recursive_file_count = Some(stats.recursive_file_count);
            entry.recursive_dir_count = Some(stats.recursive_dir_count);
        }
    }
    Ok(())
}

/// Fallback: resolve each directory path individually (handles mixed-parent entries).
fn enrich_via_individual_paths(entries: &mut [FileEntry], store: &IndexStore) {
    let conn = store.read_conn();

    // Resolve each dir path → entry_id, then batch-fetch stats
    let mut id_to_path: Vec<(i64, String)> = Vec::new();
    for entry in entries.iter().filter(|e| e.is_directory && !e.is_symlink) {
        let normalized = firmlinks::normalize_path(&entry.path);
        if let Ok(Some(id)) = store::resolve_path(conn, &normalized) {
            id_to_path.push((id, normalized));
        }
    }

    if id_to_path.is_empty() {
        return;
    }

    let ids: Vec<i64> = id_to_path.iter().map(|(id, _)| *id).collect();
    let stats_batch = match IndexStore::get_dir_stats_batch_by_ids(conn, &ids) {
        Ok(s) => s,
        Err(e) => {
            log::debug!("Index enrichment fallback failed: {e}");
            return;
        }
    };

    // Map normalized path -> DirStatsById for lookup
    let mut stats_map: std::collections::HashMap<String, store::DirStatsById> =
        std::collections::HashMap::with_capacity(id_to_path.len());
    for ((_, path), stats_opt) in id_to_path.into_iter().zip(stats_batch) {
        if let Some(s) = stats_opt {
            stats_map.insert(path, s);
        }
    }

    // Apply to entries
    for entry in entries.iter_mut().filter(|e| e.is_directory && !e.is_symlink) {
        let normalized = firmlinks::normalize_path(&entry.path);
        if let Some(stats) = stats_map.get(&normalized) {
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
/// Owns the SQLite store (reads), the writer thread (writes), the path resolver
/// (LRU-cached path→ID mapping), the scanner handle, and the micro-scan manager.
/// Exposed to Tauri commands via `IndexManagerState`.
pub struct IndexManager {
    /// Volume ID (for example, "root" for /)
    volume_id: String,
    /// Volume root path
    volume_root: PathBuf,
    /// SQLite store for reads
    store: IndexStore,
    /// Writer handle for sending writes
    writer: IndexWriter,
    /// Path resolver with LRU cache for path → entry ID resolution
    path_resolver: PathResolver,
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

        let path_resolver = PathResolver::new();

        // Open a separate read connection for global enrichment (used by get_file_range)
        match IndexStore::open(&db_path) {
            Ok(global_store) => set_global_index_store(global_store),
            Err(e) => log::warn!("Failed to open global index read connection: {e}"),
        }

        log::debug!(
            "IndexManager created for volume '{volume_id}' at {}",
            volume_root.display()
        );

        Ok(Self {
            volume_id,
            volume_root,
            store,
            writer,
            path_resolver,
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
    /// **macOS (with event replay support):**
    /// If an existing index exists (`scan_completed_at` is set in meta) and we have a
    /// stored `last_event_id`, start the FSEvents watcher with `sinceWhen = last_event_id`
    /// to replay the journal. If the journal is unavailable, fall back to a full scan.
    ///
    /// **Linux (no event replay):**
    /// Always does a full scan on startup. The existing index DB is kept as-is for
    /// instant enrichment; the scan overwrites stale entries. The watcher starts
    /// alongside the scan for live events.
    ///
    /// **No existing index:** Full scan via `start_scan()`.
    pub fn resume_or_scan(&mut self) -> Result<(), String> {
        let status = self
            .store
            .get_index_status()
            .map_err(|e| format!("Failed to get index status: {e}"))?;

        // Event ID replay is only available on macOS (FSEvents journal).
        // On Linux (inotify), always rescan -- there's no journal to replay.
        if watcher::supports_event_replay() && status.scan_completed_at.is_some() {
            if let Some(ref last_event_id_str) = status.last_event_id {
                let last_event_id: u64 = last_event_id_str.parse().unwrap_or(0);
                if last_event_id > 0 {
                    log::debug!(
                        "Existing index found (scan_completed_at={}, last_event_id={last_event_id}), \
                         attempting sinceWhen replay",
                        status.scan_completed_at.as_deref().unwrap_or("?"),
                    );
                    return self.start_replay(last_event_id);
                }
            }
            log::debug!("Existing index found but no last_event_id, starting fresh scan");
        } else if status.scan_completed_at.is_some() {
            log::debug!("Existing index found, starting rescan (no event replay on this platform)");
        } else {
            log::debug!("No existing index (scan_completed_at not set), starting fresh scan");
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
                log::debug!("DriveWatcher started for replay (sinceWhen={since_event_id}, current={current_id})");
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
                log::debug!("DriveWatcher started (scan_start_event_id={scan_start_event_id})");
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
                    log::debug!(
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
                    log::debug!("Reconciler: buffered {buffered_count} events during scan");

                    // Flush the writer to ensure all scan batches are committed
                    // before opening the read connection. Without this, the WAL
                    // snapshot may not include the latest InsertEntriesV2 batches,
                    // causing resolve_path to fail for recently-scanned parents.
                    if let Err(e) = writer.flush().await {
                        log::warn!("Reconciler: writer flush before replay failed: {e}");
                    }

                    // Open a read connection for path resolution during replay
                    let replay_conn = match IndexStore::open_write_connection(&writer.db_path()) {
                        Ok(c) => c,
                        Err(e) => {
                            log::warn!("Reconciler: failed to open read connection for replay: {e}");
                            return;
                        }
                    };

                    // Replay events that arrived after the scan read their paths
                    match reconciler.replay(scan_start_event_id, &replay_conn, &writer, &app) {
                        Ok(last_id) => {
                            log::debug!("Reconciler: replay complete (last_event_id={last_id})");
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
    ///
    /// Resolves the path to an entry ID using the `PathResolver` (LRU-cached),
    /// then fetches `dir_stats` by integer ID.
    pub fn get_dir_stats(&mut self, path: &str) -> Result<Option<DirStats>, String> {
        let normalized = firmlinks::normalize_path(path);
        let conn = self.store.read_conn();

        let entry_id = match self
            .path_resolver
            .resolve(conn, &normalized)
            .map_err(|e| format!("Failed to resolve path: {e}"))?
        {
            Some(id) => id,
            None => return Ok(None),
        };

        let stats =
            IndexStore::get_dir_stats_by_id(conn, entry_id).map_err(|e| format!("Failed to get dir stats: {e}"))?;

        Ok(stats.map(|s| DirStats {
            path: normalized,
            recursive_size: s.recursive_size,
            recursive_file_count: s.recursive_file_count,
            recursive_dir_count: s.recursive_dir_count,
        }))
    }

    /// Batch lookup of dir_stats for multiple paths.
    ///
    /// Resolves each path to an entry ID using the `PathResolver` (LRU-cached),
    /// then batch-fetches `dir_stats` by integer IDs.
    pub fn get_dir_stats_batch(&mut self, paths: &[String]) -> Result<Vec<Option<DirStats>>, String> {
        let conn = self.store.read_conn();

        let mut results = Vec::with_capacity(paths.len());
        let mut id_to_idx: Vec<(i64, usize, String)> = Vec::new();

        for (i, path) in paths.iter().enumerate() {
            let normalized = firmlinks::normalize_path(path);
            match self
                .path_resolver
                .resolve(conn, &normalized)
                .map_err(|e| format!("Failed to resolve path: {e}"))?
            {
                Some(id) => {
                    id_to_idx.push((id, i, normalized));
                    results.push(None); // Placeholder, filled below
                }
                None => results.push(None),
            }
        }

        if !id_to_idx.is_empty() {
            let ids: Vec<i64> = id_to_idx.iter().map(|(id, _, _)| *id).collect();
            let stats_batch = IndexStore::get_dir_stats_batch_by_ids(conn, &ids)
                .map_err(|e| format!("Failed to get dir stats batch: {e}"))?;

            for ((_, idx, normalized), stats_opt) in id_to_idx.into_iter().zip(stats_batch) {
                results[idx] = stats_opt.map(|s| DirStats {
                    path: normalized,
                    recursive_size: s.recursive_size,
                    recursive_file_count: s.recursive_file_count,
                    recursive_dir_count: s.recursive_dir_count,
                });
            }
        }

        Ok(results)
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
        log::debug!("IndexManager shut down for volume '{}'", self.volume_id);
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
    log::debug!("Live event processing started");

    // Open a read connection for path resolution (integer-keyed lookups)
    let db_path = writer.db_path();
    let conn = match IndexStore::open_write_connection(&db_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Live event loop: failed to open read connection: {e}");
            return;
        }
    };

    let mut event_count = 0u64;
    let mut pending_paths = std::collections::HashSet::<String>::new();
    let mut flush_interval = tokio::time::interval(Duration::from_millis(300));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        reconciler.process_live_event(&event, &conn, &writer, &mut pending_paths);
                        event_count += 1;
                        if event_count.is_multiple_of(10_000) {
                            log::debug!("Live event processing: {event_count} events processed so far");
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

    log::debug!("Live event processing stopped after {event_count} events");
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
    log::debug!("Replay event processing started (since_event_id={since_event_id})");

    // Open a read connection for path resolution (integer-keyed lookups)
    let db_path = writer.db_path();
    let conn = match IndexStore::open_write_connection(&db_path) {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("Failed to open read connection for replay: {e}"));
        }
    };

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
            log::debug!(
                "Replay: first event_id={}, gap from stored={}, journal appears available",
                event.event_id,
                event.event_id.saturating_sub(since_event_id),
            );
        }

        // HistoryDone marks end of replay phase
        if event.flags.history_done {
            log::debug!("Replay: HistoryDone received after {event_count} events");

            // Process the HistoryDone event itself (it may carry other flags)
            if let Some(paths) = reconciler::process_fs_event(&event, &conn, &writer) {
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
        if let Some(paths) = reconciler::process_fs_event(&event, &conn, &writer) {
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
            log::debug!("Replay: {event_count} events processed so far");
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
            log::debug!(
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

    log::debug!("Replay: switching to live mode");
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
                        reconciler.process_live_event(&event, &conn, &writer, &mut live_pending_paths);
                        live_count += 1;
                        if live_count.is_multiple_of(10_000) {
                            log::debug!("Live event processing (post-replay): {live_count} events");
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

    log::debug!("Replay event loop stopped ({event_count} replay + {live_count} live events)");
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
    log::debug!(
        "Background verification started ({} affected dirs)",
        affected_paths.len(),
    );

    // Verify affected directories: FSEvents journal replay coalesces events,
    // so child deletions may only show as "parent dir modified," and new
    // children may not get individual creation events. Readdir each affected
    // parent and reconcile with DB.
    let verify_result = verify_affected_dirs(&affected_paths, &writer);

    // Scan newly discovered directories (inserts children + computes subtree aggregates).
    // Skip excluded paths (system dirs like /System, /dev) that aren't in the index.
    if !verify_result.new_dir_paths.is_empty() {
        // Flush first: verify_affected_dirs sent UpsertEntryV2 for each new dir, but those
        // writes are still queued. scan_subtree opens a read connection to resolve the dir's
        // path → entry_id, which fails if the entry isn't committed yet.
        if let Err(e) = writer.flush().await {
            log::warn!("Background verification pre-scan flush failed: {e}");
        }

        let cancelled = AtomicBool::new(false);
        for dir_path in &verify_result.new_dir_paths {
            if scanner::should_exclude(dir_path) {
                continue;
            }
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

    let has_changes =
        verify_result.stale_count > 0 || verify_result.new_file_count > 0 || !verify_result.new_dir_paths.is_empty();

    if has_changes {
        log::debug!(
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
        // upward. Resolve each new dir path to its entry ID, read the computed
        // dir_stats, and send PropagateDeltaById to the parent.
        if !verify_result.new_dir_paths.is_empty() {
            // Brief lock: resolve paths → IDs and batch-read dir_stats
            let dir_deltas: Vec<(i64, store::DirStatsById)> = {
                let guard = GLOBAL_INDEX_STORE.lock();
                if let Ok(ref guard) = guard
                    && let Some(store) = guard.as_ref()
                {
                    let conn = store.read_conn();
                    let mut deltas = Vec::new();
                    for dir_path in &verify_result.new_dir_paths {
                        let entry_id = match store::resolve_path(conn, dir_path) {
                            Ok(Some(id)) => id,
                            _ => continue,
                        };
                        let parent_id = match IndexStore::get_parent_id(conn, entry_id) {
                            Ok(Some(pid)) => pid,
                            _ => continue,
                        };
                        let stats = IndexStore::get_dir_stats_by_id(conn, entry_id)
                            .ok()
                            .flatten()
                            .unwrap_or(store::DirStatsById {
                                entry_id,
                                recursive_size: 0,
                                recursive_file_count: 0,
                                recursive_dir_count: 0,
                            });
                        deltas.push((parent_id, stats));
                    }
                    deltas
                } else {
                    Vec::new()
                }
                // guard dropped here
            };

            for (parent_id, stats) in &dir_deltas {
                let _ = writer.send(WriteMessage::PropagateDeltaById {
                    entry_id: *parent_id,
                    size_delta: stats.recursive_size as i64,
                    file_count_delta: stats.recursive_file_count as i32,
                    dir_count_delta: (stats.recursive_dir_count as i32) + 1,
                });
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

    log::debug!(
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
/// Two-phase approach to minimize `GLOBAL_INDEX_STORE` lock hold time:
///
/// **Phase 1 (lock held briefly):** Resolve each affected path to its entry ID,
/// list children as `EntryRow` (integer-keyed), and snapshot into a `HashMap`.
/// Then drop the lock. Only SQLite reads, no disk I/O.
///
/// **Phase 2 (no lock):** Walk the snapshot, check the filesystem
/// (`Path::exists`, `read_dir`, `symlink_metadata`), and send corrections to
/// the writer channel using integer-keyed write messages:
/// 1. **Stale entries**: DB children that no longer exist on disk get
///    `DeleteEntryById`/`DeleteSubtreeById` (auto-propagates deltas).
/// 2. **Missing entries**: Disk children not in DB get `UpsertEntryV2`.
///    New files also get `PropagateDeltaById`. New directories are collected
///    in `new_dir_paths` for the caller to scan via `scan_subtree`.
fn verify_affected_dirs(affected_paths: &std::collections::HashSet<String>, writer: &IndexWriter) -> VerifyResult {
    // ── Phase 1: Bulk-read DB state under the lock ───────────────────
    // Snapshot: parent_path → (parent_id, Vec<EntryRow>)
    let db_snapshot: std::collections::HashMap<String, (i64, Vec<store::EntryRow>)> = {
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

        let conn = store.read_conn();
        let mut snapshot = std::collections::HashMap::with_capacity(affected_paths.len());
        for parent_path in affected_paths {
            let parent_id = match store::resolve_path(conn, parent_path) {
                Ok(Some(id)) => id,
                _ => continue, // Path not in index, skip
            };
            match store.list_children(parent_id) {
                Ok(entries) => {
                    snapshot.insert(parent_path.clone(), (parent_id, entries));
                }
                Err(_) => {
                    // Insert empty vec so Phase 2 still checks disk for new entries
                    snapshot.insert(parent_path.clone(), (parent_id, Vec::new()));
                }
            }
        }
        snapshot
        // guard dropped here — lock released before any disk I/O
    };

    // ── Phase 2: Filesystem checks without the lock ──────────────────
    let mut stale_count = 0u64;
    let mut new_file_count = 0u64;
    let mut new_dir_paths = Vec::<String>::new();

    for (parent_path, (parent_id, db_children)) in &db_snapshot {
        // Build a set of normalized DB child names for fast lookup
        let db_child_names: std::collections::HashSet<String> = db_children
            .iter()
            .map(|c| store::normalize_for_comparison(&c.name))
            .collect();

        // Build child path from parent_path + name for filesystem checks
        let parent_prefix = if parent_path == "/" {
            String::new()
        } else {
            parent_path.clone()
        };

        // Detect stale entries (in DB but not on disk)
        for child in db_children {
            let child_path = format!("{}/{}", parent_prefix, child.name);
            if !Path::new(&child_path).exists() {
                if child.is_directory {
                    let _ = writer.send(WriteMessage::DeleteSubtreeById(child.id));
                } else {
                    let _ = writer.send(WriteMessage::DeleteEntryById(child.id));
                }
                stale_count += 1;
            }
        }

        // Detect missing entries (on disk but not in DB)
        let read_dir = match std::fs::read_dir(parent_path) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        for dir_entry in read_dir.flatten() {
            let child_path = dir_entry.path();
            let child_path_str = child_path.to_string_lossy().to_string();
            let normalized = firmlinks::normalize_path(&child_path_str);

            let name = dir_entry.file_name().to_string_lossy().to_string();
            if db_child_names.contains(&store::normalize_for_comparison(&name)) {
                continue;
            }

            let metadata = match std::fs::symlink_metadata(&child_path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_dir = metadata.is_dir();
            let is_symlink = metadata.is_symlink();

            let (size, modified_at) = if is_dir || is_symlink {
                (None, reconciler::entry_modified_at(&metadata))
            } else {
                reconciler::entry_size_and_mtime(&metadata)
            };

            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: *parent_id,
                name,
                is_directory: is_dir,
                is_symlink,
                size,
                modified_at,
            });

            if is_dir {
                log::debug!("verify_affected_dirs: new dir on disk: {normalized} (parent_id={parent_id})");
                new_dir_paths.push(normalized);
            } else if let Some(sz) = size {
                // UpsertEntryV2 inserts the entry; propagate its size delta up the
                // ancestor chain starting from the parent.
                let _ = writer.send(WriteMessage::PropagateDeltaById {
                    entry_id: *parent_id,
                    size_delta: sz as i64,
                    file_count_delta: 1,
                    dir_count_delta: 0,
                });
                new_file_count += 1;
            }
        }
    }

    if stale_count > 0 || new_file_count > 0 || !new_dir_paths.is_empty() {
        log::debug!(
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
/// `start_indexing()`).
pub fn init(app: &AppHandle) {
    app.manage(IndexManagerState(std::sync::Mutex::new(None)));
    log::debug!("Indexing state registered");
}

/// Whether indexing should auto-start on launch.
///
/// - If settings say disabled (`indexing_enabled == Some(false)`): never auto-start.
/// - Otherwise: auto-start by default (both dev and release builds).
pub fn should_auto_start(indexing_enabled: Option<bool>) -> bool {
    // User explicitly disabled indexing in settings
    if indexing_enabled == Some(false) {
        return false;
    }

    // Default true (setting not yet stored means first launch, enabled by default)
    true
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

    log::debug!("Indexing system initialized for root volume");
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

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::listing::FileEntry;
    use store::{DirStatsById, EntryRow, IndexStore, ROOT_ID};

    /// Helper: open a temp store and write connection for testing.
    fn open_temp_store() -> (IndexStore, rusqlite::Connection, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test-index.db");
        let store = IndexStore::open(&db_path).expect("open store");
        let conn = IndexStore::open_write_connection(&db_path).expect("open write conn");
        (store, conn, dir)
    }

    /// Helper: create a FileEntry for testing enrichment.
    fn make_file_entry(name: &str, path: &str, is_directory: bool) -> FileEntry {
        FileEntry {
            name: name.to_string(),
            path: path.to_string(),
            is_directory,
            is_symlink: false,
            size: if is_directory { None } else { Some(100) },
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o755,
            owner: String::new(),
            group: String::new(),
            icon_id: String::new(),
            extended_metadata_loaded: false,
            recursive_size: None,
            recursive_file_count: None,
            recursive_dir_count: None,
        }
    }

    /// End-to-end test: insert entries, compute aggregates, enrich FileEntry objects, verify stats.
    #[test]
    fn enrich_entries_via_parent_id_end_to_end() {
        let (store, conn, _dir) = open_temp_store();

        // Build a tree:
        //   / (ROOT_ID=1)
        //   /projects (dir, id=2)
        //   /projects/alpha (dir, id=3)
        //   /projects/alpha/file1.txt (100 bytes, id=4)
        //   /projects/alpha/file2.txt (200 bytes, id=5)
        //   /projects/beta (dir, id=6)
        //   /projects/beta/file3.txt (300 bytes, id=7)
        //   /projects/readme.txt (file, 50 bytes, id=8)
        let entries = vec![
            EntryRow {
                id: 2,
                parent_id: ROOT_ID,
                name: "projects".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 3,
                parent_id: 2,
                name: "alpha".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 4,
                parent_id: 3,
                name: "file1.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(100),
                modified_at: None,
            },
            EntryRow {
                id: 5,
                parent_id: 3,
                name: "file2.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(200),
                modified_at: None,
            },
            EntryRow {
                id: 6,
                parent_id: 2,
                name: "beta".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 7,
                parent_id: 6,
                name: "file3.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(300),
                modified_at: None,
            },
            EntryRow {
                id: 8,
                parent_id: 2,
                name: "readme.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(50),
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert entries");

        // Compute aggregates
        aggregator::compute_all_aggregates(&conn).expect("compute aggregates");

        // Verify aggregates were computed correctly
        let alpha_stats = IndexStore::get_dir_stats_by_id(&conn, 3).expect("get alpha stats");
        assert!(alpha_stats.is_some(), "alpha should have dir_stats");
        let alpha = alpha_stats.unwrap();
        assert_eq!(alpha.recursive_size, 300, "alpha: 100+200=300");
        assert_eq!(alpha.recursive_file_count, 2, "alpha: 2 files");
        assert_eq!(alpha.recursive_dir_count, 0, "alpha: 0 subdirs");

        let beta_stats = IndexStore::get_dir_stats_by_id(&conn, 6).expect("get beta stats");
        assert!(beta_stats.is_some(), "beta should have dir_stats");
        let beta = beta_stats.unwrap();
        assert_eq!(beta.recursive_size, 300, "beta: 300");
        assert_eq!(beta.recursive_file_count, 1, "beta: 1 file");
        assert_eq!(beta.recursive_dir_count, 0, "beta: 0 subdirs");

        let projects_stats = IndexStore::get_dir_stats_by_id(&conn, 2).expect("get projects stats");
        assert!(projects_stats.is_some(), "projects should have dir_stats");
        let proj = projects_stats.unwrap();
        assert_eq!(proj.recursive_size, 650, "projects: 100+200+300+50=650");
        assert_eq!(
            proj.recursive_file_count, 4,
            "projects: 4 files (file1, file2, file3, readme)"
        );
        assert_eq!(proj.recursive_dir_count, 2, "projects: 2 subdirs (alpha, beta)");

        // Now test enrichment: simulate a listing of /projects children
        let mut file_entries = vec![
            make_file_entry("alpha", "/projects/alpha", true),
            make_file_entry("beta", "/projects/beta", true),
            make_file_entry("readme.txt", "/projects/readme.txt", false),
        ];

        // Use the integer-keyed fast path
        let result = enrich_via_parent_id(&mut file_entries, &store, "/projects");
        assert!(result.is_ok(), "enrich_via_parent_id should succeed: {result:?}");

        // Verify enrichment results
        let alpha_entry = &file_entries[0];
        assert_eq!(alpha_entry.recursive_size, Some(300));
        assert_eq!(alpha_entry.recursive_file_count, Some(2));
        assert_eq!(alpha_entry.recursive_dir_count, Some(0));

        let beta_entry = &file_entries[1];
        assert_eq!(beta_entry.recursive_size, Some(300));
        assert_eq!(beta_entry.recursive_file_count, Some(1));
        assert_eq!(beta_entry.recursive_dir_count, Some(0));

        // Non-directory entries should be unaffected
        let readme_entry = &file_entries[2];
        assert_eq!(readme_entry.recursive_size, None);
    }

    /// Test enrichment fallback for individual path resolution.
    #[test]
    fn enrich_entries_fallback_individual_paths() {
        let (store, conn, _dir) = open_temp_store();

        // Simple tree: /docs (dir) with one file
        let entries = vec![
            EntryRow {
                id: 2,
                parent_id: ROOT_ID,
                name: "docs".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 3,
                parent_id: 2,
                name: "guide.md".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(500),
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");
        aggregator::compute_all_aggregates(&conn).expect("aggregates");

        let mut file_entries = vec![make_file_entry("docs", "/docs", true)];

        // Use the individual path fallback
        enrich_via_individual_paths(&mut file_entries, &store);

        let docs = &file_entries[0];
        assert_eq!(docs.recursive_size, Some(500));
        assert_eq!(docs.recursive_file_count, Some(1));
        assert_eq!(docs.recursive_dir_count, Some(0));
    }

    /// Test that enrichment handles empty directory listing.
    #[test]
    fn enrich_entries_empty_list() {
        let (store, _conn, _dir) = open_temp_store();
        let mut entries: Vec<FileEntry> = Vec::new();
        enrich_via_individual_paths(&mut entries, &store);
    }

    /// Test that enrichment handles entries with no matching index data.
    #[test]
    fn enrich_entries_no_matching_index() {
        let (store, _conn, _dir) = open_temp_store();
        let mut entries = vec![make_file_entry("nonexistent", "/nonexistent", true)];
        enrich_via_individual_paths(&mut entries, &store);
        assert_eq!(entries[0].recursive_size, None, "unindexed dir should remain None");
    }

    /// Test that `list_child_dir_ids_and_names` returns only directories.
    #[test]
    fn list_child_dir_ids_and_names_filters_files() {
        let (_store, conn, _dir) = open_temp_store();

        let entries = vec![
            EntryRow {
                id: 2,
                parent_id: ROOT_ID,
                name: "dir_a".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 3,
                parent_id: ROOT_ID,
                name: "dir_b".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 4,
                parent_id: ROOT_ID,
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(10),
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");

        let child_dirs = IndexStore::list_child_dir_ids_and_names(&conn, ROOT_ID).expect("list");
        assert_eq!(child_dirs.len(), 2, "should only return directories, not files");

        let names: std::collections::HashSet<&str> = child_dirs.iter().map(|(_, n)| n.as_str()).collect();
        assert!(names.contains("dir_a"));
        assert!(names.contains("dir_b"));
    }

    /// Test the PathResolver integration for dir stats lookups.
    #[test]
    fn path_resolver_for_dir_stats() {
        let (store, conn, _dir) = open_temp_store();

        let entries = vec![
            EntryRow {
                id: 2,
                parent_id: ROOT_ID,
                name: "src".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 3,
                parent_id: 2,
                name: "main.rs".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(1000),
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");
        aggregator::compute_all_aggregates(&conn).expect("aggregates");

        let mut resolver = PathResolver::new();
        let entry_id = resolver.resolve(store.read_conn(), "/src").expect("resolve");
        assert_eq!(entry_id, Some(2));

        let stats = IndexStore::get_dir_stats_by_id(store.read_conn(), 2).expect("stats");
        assert!(stats.is_some());
        assert_eq!(stats.unwrap().recursive_size, 1000);

        // Second resolve should hit LRU cache (no DB access)
        let cached_id = resolver.resolve(store.read_conn(), "/src").expect("cached");
        assert_eq!(cached_id, Some(2));
    }

    /// End-to-end: scan -> aggregate -> enrich -> simulate watcher event -> re-enrich -> verify.
    #[test]
    fn end_to_end_scan_enrich_watcher_update() {
        let (store, conn, _dir) = open_temp_store();

        // Phase 1: Initial scan
        let entries = vec![
            EntryRow {
                id: 2,
                parent_id: ROOT_ID,
                name: "home".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 3,
                parent_id: 2,
                name: "user".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 4,
                parent_id: 3,
                name: "doc.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(1000),
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");
        aggregator::compute_all_aggregates(&conn).expect("aggregates");

        // Verify initial aggregates
        let home_stats = IndexStore::get_dir_stats_by_id(&conn, 2).unwrap().unwrap();
        assert_eq!(home_stats.recursive_size, 1000);
        assert_eq!(home_stats.recursive_file_count, 1);
        assert_eq!(home_stats.recursive_dir_count, 1);

        // Phase 2: Enrich a listing of /home children
        let mut listing = vec![make_file_entry("user", "/home/user", true)];
        let result = enrich_via_parent_id(&mut listing, &store, "/home");
        assert!(result.is_ok());
        assert_eq!(listing[0].recursive_size, Some(1000));
        assert_eq!(listing[0].recursive_file_count, Some(1));
        assert_eq!(listing[0].recursive_dir_count, Some(0));

        // Phase 3: Simulate a watcher event (new file added via reconciler)
        IndexStore::insert_entry_v2(&conn, 3, "notes.txt", false, false, Some(500), None).expect("insert new file");

        // Simulate delta propagation (as the writer would do)
        let updated_user = DirStatsById {
            entry_id: 3,
            recursive_size: 1500,
            recursive_file_count: 2,
            recursive_dir_count: 0,
        };
        IndexStore::upsert_dir_stats_by_id(&conn, &[updated_user]).expect("update user stats");

        let updated_home = DirStatsById {
            entry_id: 2,
            recursive_size: 1500,
            recursive_file_count: 2,
            recursive_dir_count: 1,
        };
        IndexStore::upsert_dir_stats_by_id(&conn, &[updated_home]).expect("update home stats");

        // Phase 4: Re-enrich after watcher event
        let mut listing2 = vec![make_file_entry("user", "/home/user", true)];
        let result2 = enrich_via_parent_id(&mut listing2, &store, "/home");
        assert!(result2.is_ok());
        assert_eq!(listing2[0].recursive_size, Some(1500), "should reflect new file");
        assert_eq!(listing2[0].recursive_file_count, Some(2));

        // Phase 5: Verify integer-keyed lookup works
        let user_id = store::resolve_path(&conn, "/home/user").unwrap().unwrap();
        let user_stats = IndexStore::get_dir_stats_by_id(&conn, user_id).unwrap();
        assert!(user_stats.is_some());
        let user = user_stats.unwrap();
        assert_eq!(user.recursive_size, 1500);
    }

    /// Test enrichment of entries at the root level (parent = /).
    #[test]
    fn enrich_entries_at_root_level() {
        let (store, conn, _dir) = open_temp_store();

        let entries = vec![
            EntryRow {
                id: 2,
                parent_id: ROOT_ID,
                name: "Applications".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 3,
                parent_id: 2,
                name: "app.exe".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(5000),
                modified_at: None,
            },
            EntryRow {
                id: 4,
                parent_id: ROOT_ID,
                name: "Users".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 5,
                parent_id: 4,
                name: "someone".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");
        aggregator::compute_all_aggregates(&conn).expect("aggregates");

        // Listing at /: children are /Applications and /Users
        let mut listing = vec![
            make_file_entry("Applications", "/Applications", true),
            make_file_entry("Users", "/Users", true),
        ];

        let result = enrich_via_parent_id(&mut listing, &store, "/");
        assert!(result.is_ok());

        assert_eq!(listing[0].recursive_size, Some(5000));
        assert_eq!(listing[0].recursive_file_count, Some(1));

        assert_eq!(listing[1].recursive_size, Some(0));
        assert_eq!(listing[1].recursive_dir_count, Some(1));
    }
}
