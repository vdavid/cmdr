//! Drive indexing module.
//!
//! Background-indexes local volumes into a per-volume SQLite database,
//! tracking every file and directory with recursive size aggregates.
//! See `docs/specs/drive-indexing/plan.md` for the full design.

pub mod aggregator;
mod enrichment;
mod event_loop;
mod events;
pub mod firmlinks;
pub mod path_resolver;
pub mod store;
pub mod writer;

mod memory_watchdog;
mod micro_scan;
mod reconciler;
pub(crate) mod scanner;
mod verifier; // Placeholder: per-navigation background readdir diff (future milestone)
pub(crate) mod watcher;

pub use enrichment::enrich_entries_with_index;
pub use events::*;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use enrichment::{ReadPool, READ_POOL};
use event_loop::{
    JOURNAL_GAP_THRESHOLD, WATCHER_CHANNEL_CAPACITY,
    ReplayConfig, run_live_event_loop, run_replay_event_loop,
};
use micro_scan::{MicroScanManager, ScanPriority};
use path_resolver::PathResolver;
use reconciler::EventReconciler;
use scanner::ScanConfig;
use store::{DirStats, IndexStore};
use tauri::{AppHandle, Emitter};
use watcher::DriveWatcher;
use writer::{IndexWriter, WriteMessage};

// ── Re-exports for commands ──────────────────────────────────────────

pub use micro_scan::ScanPriority as PubScanPriority;

// ── Indexing state machine ────────────────────────────────────────────

/// Lifecycle phases of the indexing system. Single source of truth for
/// whether indexing is active and what capabilities are available.
pub(crate) enum IndexPhase {
    /// Indexing is not active (disabled by user, not yet started, or shut down).
    Disabled,
    /// IndexManager created, `resume_or_scan()` is running. A temporary read
    /// store is available for enrichment and status queries while initialization
    /// completes.
    Initializing { store: IndexStore },
    /// Fully operational: scanning, watching, enrichment, IPC all work.
    Running(Box<IndexManager>),
    /// Shutdown in progress (transitional, cleanup running).
    ShuttingDown,
}

static INDEXING: LazyLock<std::sync::Mutex<IndexPhase>> = LazyLock::new(|| std::sync::Mutex::new(IndexPhase::Disabled));

// ── Debug stats (shared atomics for the debug window) ────────────────

/// Shared counters for MustScanSubDirs events and live FS events.
/// Updated by event loops, read by the debug status IPC command.
struct DebugStats {
    must_scan_sub_dirs_count: AtomicU64,
    must_scan_rescans_completed: AtomicU64,
    live_event_count: AtomicU64,
    watcher_active: AtomicBool,
    /// Recent MustScanSubDirs paths: (timestamp, path). Ring buffer.
    recent_must_scan_paths: std::sync::Mutex<Vec<(String, String)>>,
}

impl DebugStats {
    fn new() -> Self {
        Self {
            must_scan_sub_dirs_count: AtomicU64::new(0),
            must_scan_rescans_completed: AtomicU64::new(0),
            live_event_count: AtomicU64::new(0),
            watcher_active: AtomicBool::new(false),
            recent_must_scan_paths: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn record_must_scan(&self, path: &str) {
        self.must_scan_sub_dirs_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut paths) = self.recent_must_scan_paths.lock() {
            let now = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
            paths.push((now, path.to_string()));
            if paths.len() > 50 {
                let excess = paths.len() - 50;
                paths.drain(..excess);
            }
        }
    }

    fn record_rescan_completed(&self) {
        self.must_scan_rescans_completed.fetch_add(1, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.must_scan_sub_dirs_count.store(0, Ordering::Relaxed);
        self.must_scan_rescans_completed.store(0, Ordering::Relaxed);
        self.live_event_count.store(0, Ordering::Relaxed);
        self.watcher_active.store(false, Ordering::Relaxed);
        if let Ok(mut paths) = self.recent_must_scan_paths.lock() {
            paths.clear();
        }
    }
}

static DEBUG_STATS: LazyLock<DebugStats> = LazyLock::new(DebugStats::new);

use events::emit_rescan_notification;

// ── IndexManager ─────────────────────────────────────────────────────

/// Central coordinator for the drive indexing system.
///
/// Owns the SQLite store (reads), the writer thread (writes), the path resolver
/// (LRU-cached path→ID mapping), the scanner handle, and the micro-scan manager.
/// Accessed by module-level functions that lock the `INDEXING` static.
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
    /// Live event processing task (runs after reconciliation completes).
    /// Shared with spawned async tasks so they can store the handle.
    live_event_task: Arc<std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
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
        let data_dir = crate::config::resolved_app_data_dir(&app)?;

        let db_path = data_dir.join(format!("index-{volume_id}.db"));

        let store = IndexStore::open(&db_path).map_err(|e| format!("Failed to open index store: {e}"))?;

        let writer = IndexWriter::spawn(&db_path, Some(app.clone()))
            .map_err(|e| format!("Failed to spawn index writer: {e}"))?;

        let micro_scans = MicroScanManager::new(writer.clone(), 3);

        let path_resolver = PathResolver::new();

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
            live_event_task: Arc::new(std::sync::Mutex::new(None)),
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
                    // Pre-check: compare stored event ID with current system event ID.
                    // If the gap is too large, skip replay entirely — the cmdr-fsevent-stream
                    // channel (1024 capacity, try_send) would silently drop most events,
                    // and replaying millions of events is slower than a fresh scan anyway.
                    let current_id = watcher::current_event_id();
                    if current_id > 0 && current_id > last_event_id + JOURNAL_GAP_THRESHOLD {
                        let gap = current_id - last_event_id;
                        emit_rescan_notification(
                            &self.app,
                            &self.volume_id,
                            RescanReason::StaleIndex,
                            format!(
                                "Stored last_event_id={last_event_id}, current system \
                                 event_id={current_id}, gap={gap} \
                                 (threshold={JOURNAL_GAP_THRESHOLD}). \
                                 The app likely hasn't run for a long time."
                            ),
                        );
                        return self.start_scan();
                    }

                    let current_id = watcher::current_event_id();
                    let gap = current_id.saturating_sub(last_event_id);
                    log::info!(
                        "Startup: cold-start replay (last_event_id={last_event_id}, current={current_id}, gap={gap})",
                    );
                    return self.start_replay(last_event_id);
                }
            }
            log::info!("Startup: fresh scan (existing index has no last_event_id)");
        } else if status.scan_completed_at.is_some() {
            log::info!("Startup: full rescan (no event replay on this platform)");
        } else if status.last_event_id.is_some() {
            emit_rescan_notification(
                &self.app,
                &self.volume_id,
                RescanReason::IncompletePreviousScan,
                "Index DB exists but scan_completed_at is not set. Previous scan likely didn't \
                 finish."
                    .to_string(),
            );
        } else {
            log::info!("Startup: fresh scan (no existing index)");
        }

        self.start_scan()
    }

    /// Resume from an existing index by replaying FSEvents journal since `since_event_id`.
    ///
    /// Starts the watcher with `sinceWhen = since_event_id`. The watcher replays
    /// journal events which are processed as live events. If the journal is
    /// unavailable (gap detected), falls back to a full scan.
    fn start_replay(&mut self, since_event_id: u64) -> Result<(), String> {
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(WATCHER_CHANNEL_CAPACITY);
        let current_id = watcher::current_event_id();

        let watcher_overflow: Option<Arc<AtomicBool>>;
        match DriveWatcher::start(&self.volume_root, since_event_id, event_tx) {
            Ok(watcher) => {
                watcher_overflow = Some(watcher.overflow_flag());
                self.drive_watcher = Some(watcher);
                DEBUG_STATS.watcher_active.store(true, Ordering::Relaxed);
                log::info!("Replay: watcher started (since_event_id={since_event_id}, current={current_id})");
            }
            Err(e) => {
                emit_rescan_notification(
                    &self.app,
                    &self.volume_id,
                    RescanReason::WatcherStartFailed,
                    format!("DriveWatcher failed to start for replay: {e}"),
                );
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
        let live_event_task_slot = Arc::clone(&self.live_event_task);

        // We need a way for the replay loop to signal "journal unavailable, need full scan".
        // Use a oneshot channel: if the replay detects a gap, it sends a signal.
        let (fallback_tx, fallback_rx) = tokio::sync::oneshot::channel::<()>();

        // Use tauri::async_runtime::spawn because indexing can start from the
        // synchronous Tauri setup() hook where no Tokio runtime context exists.
        // Store the handle so shutdown() can wait for it to drain.
        let handle = tauri::async_runtime::spawn(async move {
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
                watcher_overflow,
            )
            .await;

            if let Err(e) = result {
                log::warn!("Replay event loop error: {e}");
            }
        });
        {
            let mut guard = live_event_task_slot.lock().unwrap();
            *guard = Some(handle);
        }

        // Spawn a task that watches for the fallback signal and triggers a full scan if needed.
        tauri::async_runtime::spawn(async move {
            if fallback_rx.await.is_ok() {
                log::warn!("Journal replay detected gap, initiating full scan fallback");
                let mut guard = match INDEXING.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        log::warn!("Failed to lock state for fallback scan: {e}");
                        return;
                    }
                };
                if let IndexPhase::Running(mgr) = &mut *guard {
                    // Stop the current watcher (replay detected it's useless)
                    if let Some(ref mut watcher) = mgr.drive_watcher {
                        watcher.stop();
                    }
                    mgr.drive_watcher = None;
                    {
                        let mut task_guard = mgr.live_event_task.lock().unwrap();
                        if let Some(task) = task_guard.take() {
                            task.abort();
                        }
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

        // Step 0: Truncate entries + dir_stats so the scan inserts into an empty DB.
        // Without this, INSERT OR REPLACE on a populated table with the `platform_case`
        // collation is ~12x slower (30 min vs 2.5 min), and old rows with stale IDs
        // accumulate as orphaned subtrees, bloating the DB 3-4x per scan cycle.
        if let Err(e) = self.writer.send(WriteMessage::TruncateData) {
            log::warn!("Failed to send TruncateData: {e}");
        }
        if let Err(e) = tokio::task::block_in_place(|| self.writer.flush_blocking()) {
            log::warn!("Failed to flush after TruncateData: {e}");
        }

        // Step 1: Start the FSEvents watcher BEFORE the scan so we don't miss events
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(WATCHER_CHANNEL_CAPACITY);
        let scan_start_event_id = watcher::current_event_id();

        // watcher_overflow is None if the watcher failed to start (non-fatal).
        let watcher_overflow: Option<Arc<AtomicBool>>;
        match DriveWatcher::start(&self.volume_root, 0, event_tx) {
            Ok(watcher) => {
                watcher_overflow = Some(watcher.overflow_flag());
                self.drive_watcher = Some(watcher);
                DEBUG_STATS.watcher_active.store(true, Ordering::Relaxed);
                log::info!("Scan: watcher started (scan_start_event_id={scan_start_event_id})");
            }
            Err(e) => {
                watcher_overflow = None;
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
        let live_event_task_slot = Arc::clone(&self.live_event_task);
        let watcher_overflow_flag = watcher_overflow;
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
                        "Scan: complete ({} entries, {} dirs, {:.1}s)",
                        summary.total_entries,
                        summary.total_dirs,
                        summary.duration_ms as f64 / 1000.0,
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
                    log::info!("Reconciler: {buffered_count} events buffered during scan");

                    if reconciler.did_buffer_overflow() {
                        emit_rescan_notification(
                            &app,
                            &volume_id,
                            RescanReason::ReconcilerBufferOverflow,
                            "The filesystem watcher buffered over 500,000 events during the \
                             scan, exceeding the reconciler's capacity. A lot of filesystem \
                             activity was happening during the scan."
                                .to_string(),
                        );
                    }

                    // Check if the FSEvents channel overflowed (events dropped
                    // before reaching the forward task). If so, our buffered events
                    // are incomplete — the reconciler replay will miss changes.
                    // We still proceed (the scan data itself is fine), but log a
                    // warning. The live event loop will detect the overflow flag
                    // and trigger a rescan at that point, since a fresh scan is
                    // the only way to recover from dropped events.
                    if let Some(ref flag) = watcher_overflow_flag {
                        if flag.load(Ordering::Relaxed) {
                            log::info!(
                                "FSEvents channel overflowed during scan — some watcher \
                                 events were dropped. Live event loop will trigger a rescan."
                            );
                        }
                    }

                    // Emit scan-complete first, then start the flushing phase.
                    // Order matters: the frontend's scan-complete handler calls
                    // resetAggregation(), so the saving_entries event must come
                    // after to avoid being immediately cleared.
                    let _ = app.emit(
                        "index-scan-complete",
                        IndexScanCompleteEvent {
                            volume_id: volume_id.clone(),
                            total_entries: summary.total_entries,
                            total_dirs: summary.total_dirs,
                            duration_ms: summary.duration_ms,
                        },
                    );

                    // Tell the writer how many entries the scan produced, so it
                    // can report flushing progress as it drains remaining
                    // InsertEntriesV2 batches from the channel.
                    writer.set_expected_total_entries(summary.total_entries);

                    // Emit an initial saving_entries event so the UI shows
                    // progress immediately (the writer may still be processing
                    // a backlog of InsertEntriesV2 messages).
                    let _ = app.emit(
                        "index-aggregation-progress",
                        serde_json::json!({
                            "phase": "saving_entries",
                            "current": 0,
                            "total": summary.total_entries,
                        }),
                    );

                    // Flush the writer to ensure all scan batches are committed
                    // before opening the read connection. Without this, the WAL
                    // snapshot may not include the latest InsertEntriesV2 batches,
                    // causing resolve_path to fail for recently-scanned parents.
                    if let Err(e) = writer.flush().await {
                        log::warn!("Reconciler: writer flush before replay failed: {e}");
                    }

                    // Signal that aggregation (and entry flushing) is complete.
                    // The flush above drains all queued writes including
                    // ComputeAllAggregates, so by this point the UI can dismiss
                    // the progress overlay.
                    let _ = app.emit("index-aggregation-complete", ());

                    // Open a read connection for path resolution during replay
                    let replay_conn = match IndexStore::open_write_connection(&writer.db_path()) {
                        Ok(c) => c,
                        Err(e) => {
                            log::warn!("Reconciler: failed to open read connection for replay: {e}");
                            return;
                        }
                    };

                    // Set a baseline last_event_id so there's always a valid
                    // event ID even if no live events were buffered during the scan.
                    // The reconciler will overwrite this with a higher ID if any
                    // post-scan events exist.
                    if scan_start_event_id > 0 {
                        let _ = writer.send(WriteMessage::UpdateLastEventId(scan_start_event_id));
                    }

                    // Replay events that arrived after the scan read their paths
                    match reconciler.replay(scan_start_event_id, &replay_conn, &writer, &app) {
                        Ok(last_id) => {
                            log::info!("Reconciler: post-scan replay complete (last_event_id={last_id})");
                        }
                        Err(e) => {
                            log::warn!("Reconciler: replay failed: {e}");
                        }
                    }

                    // Backfill dir_stats for any directories created by the replay
                    // that didn't go through the full aggregation pass.
                    let _ = writer.send(WriteMessage::BackfillMissingDirStats);

                    // Switch to live mode
                    reconciler.switch_to_live();

                    // Step 5: Start live event processing loop
                    let writer_live = writer.clone();
                    let app_live = app.clone();
                    let volume_id_live = volume_id.clone();
                    let overflow_live = watcher_overflow_flag.clone();
                    let handle = tauri::async_runtime::spawn(async move {
                        run_live_event_loop(
                            event_rx, reconciler, writer_live, app_live,
                            volume_id_live, overflow_live,
                        ).await;
                    });

                    // Store the handle so shutdown() can wait for it to drain
                    {
                        let mut guard = live_event_task_slot.lock().unwrap();
                        *guard = Some(handle);
                    }

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

        DEBUG_STATS.reset();

        // Abort the live event processing task
        {
            let mut guard = self.live_event_task.lock().unwrap();
            if let Some(task) = guard.take() {
                task.abort();
            }
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

    /// Get extended debug status including live DB counts and event stats.
    pub fn get_debug_status(&self) -> Result<IndexDebugStatusResponse, String> {
        let base = self.get_status()?;
        let conn = self.store.read_conn();

        let live_entry_count = IndexStore::get_entry_count(conn).ok();
        let live_dir_count = IndexStore::get_dir_count(conn).ok();
        let dirs_with_stats = IndexStore::get_dirs_with_stats_count(conn).ok();

        let recent_must_scan_paths = DEBUG_STATS
            .recent_must_scan_paths
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default();

        Ok(IndexDebugStatusResponse {
            base,
            watcher_active: DEBUG_STATS.watcher_active.load(Ordering::Relaxed),
            live_event_count: DEBUG_STATS.live_event_count.load(Ordering::Relaxed),
            must_scan_count: DEBUG_STATS.must_scan_sub_dirs_count.load(Ordering::Relaxed),
            must_scan_rescans_completed: DEBUG_STATS.must_scan_rescans_completed.load(Ordering::Relaxed),
            live_entry_count,
            live_dir_count,
            dirs_with_stats,
            recent_must_scan_paths,
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

    /// Shut down the indexing system gracefully.
    ///
    /// Sequence: stop watcher (closes the channel sender) → wait for the event
    /// loop to drain its final batch and send `UpdateLastEventId` → shut down
    /// the writer. This ensures `last_event_id` is up-to-date on next restart.
    pub fn shutdown(&mut self) {
        // 1. Cancel active scan and stop micro-scans (but don't abort event loop)
        if let Some(ref handle) = self.scan_handle {
            handle.cancel();
        }
        self.scan_handle = None;
        self.scanning.store(false, Ordering::Relaxed);

        // 2. Stop the watcher. Dropping the sender closes the channel, which
        //    causes event_rx.recv() to return None in the event loop.
        if let Some(ref mut watcher) = self.drive_watcher {
            watcher.stop();
        }
        self.drive_watcher = None;

        // 3. Wait for the event loop to drain (process final batch + UpdateLastEventId).
        //    Use block_in_place so we can .await the join handle without blocking the
        //    tokio runtime thread pool.
        let task = self.live_event_task.lock().unwrap().take();
        if let Some(task) = task {
            tokio::task::block_in_place(|| {
                tauri::async_runtime::block_on(async {
                    match tokio::time::timeout(Duration::from_secs(5), task).await {
                        Ok(Ok(())) => log::debug!("Live event loop drained successfully"),
                        Ok(Err(e)) => log::debug!("Live event loop task error: {e}"),
                        Err(_) => log::warn!("Live event loop drain timed out after 5s"),
                    }
                });
            });
        }

        // 4. Now shut down the writer (all final writes have been queued)
        self.writer.shutdown();

        let micro_scans = self.micro_scans.clone();
        tauri::async_runtime::spawn(async move {
            micro_scans.cancel_all().await;
        });

        log::info!("IndexManager: shut down for volume '{}'", self.volume_id);
    }
}

// ── Initialization ───────────────────────────────────────────────────

/// Force-initialize the INDEXING static. Called during app setup so the
/// LazyLock is ready before any async tasks access it.
pub fn init() {
    drop(INDEXING.lock());
    log::debug!("Indexing state initialized");
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
pub fn stop_indexing() -> Result<(), String> {
    // Invalidate ReadPool before shutdown so thread-local connections are discarded.
    if let Some(pool) = READ_POOL.lock().unwrap().take() {
        pool.invalidate();
    }

    let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;

    match std::mem::replace(&mut *guard, IndexPhase::ShuttingDown) {
        IndexPhase::Running(mut mgr) => {
            mgr.shutdown();
            *guard = IndexPhase::Disabled;
            log::info!("Indexing stopped (DB preserved on disk)");
        }
        IndexPhase::Initializing { .. } => {
            *guard = IndexPhase::Disabled;
            log::info!("Indexing stopped during initialization");
        }
        other => {
            *guard = other; // put it back, wasn't running
        }
    }

    Ok(())
}

/// Create the IndexManager for the root volume and auto-start indexing
/// (resume from existing index or fresh scan).
///
/// Call after `init()`. On startup this checks for an existing index: if found,
/// it replays the FSEvents journal from the stored `last_event_id`; otherwise
/// it starts a fresh full scan.
pub fn start_indexing(app: &AppHandle) -> Result<(), String> {
    log::info!("start_indexing: begin");
    memory_watchdog::start(app.clone());

    let mut manager = IndexManager::new("root".to_string(), PathBuf::from("/"), app.clone())?;

    // Install ReadPool early so enrichment works during the Initializing phase.
    let pool = Arc::new(
        ReadPool::new(manager.db_path().to_path_buf())
            .map_err(|e| format!("Failed to create read pool: {e}"))?,
    );
    *READ_POOL.lock().unwrap() = Some(pool);

    // Transition to Initializing: open a temporary store so enrichment
    // and status queries work while resume_or_scan() runs.
    {
        let init_store = IndexStore::open(manager.db_path()).map_err(|e| format!("Failed to open init store: {e}"))?;
        let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;
        *guard = IndexPhase::Initializing { store: init_store };
    }

    let scan_result = manager.resume_or_scan();

    // Clone the writer before moving manager into the state machine, so we
    // can hand it to the vacuum timer if startup succeeds.
    let writer_for_vacuum = manager.writer.clone();

    // Re-lock and check: if someone called stop_indexing() while we were
    // inside resume_or_scan(), the phase is now Disabled. Respect that —
    // shut down the manager instead of overwriting with Running.
    let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;
    match (&*guard, scan_result) {
        (IndexPhase::Initializing { .. }, Ok(())) => {
            *guard = IndexPhase::Running(Box::new(manager));
            log::info!("start_indexing: done — IndexManager is Running");

            // Periodic incremental vacuum: reclaim free pages from deletes/rescans
            // every 30s. Stops automatically when the writer channel closes.
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    if writer_for_vacuum.send(WriteMessage::IncrementalVacuum).is_err() {
                        break;
                    }
                }
            });
        }
        (IndexPhase::Initializing { .. }, Err(e)) => {
            *guard = IndexPhase::Disabled;
            if let Some(pool) = READ_POOL.lock().unwrap().take() {
                pool.invalidate();
            }
            return Err(e);
        }
        (_, Ok(())) => {
            // Phase changed (e.g. stop_indexing set Disabled). Don't override.
            log::info!("start_indexing: phase changed during init, shutting down manager");
            manager.shutdown();
        }
        (_, Err(e)) => {
            log::warn!("start_indexing: resume_or_scan failed and phase changed: {e}");
            manager.shutdown();
        }
    }

    Ok(())
}

/// Stop all scans, shut down the writer, delete the DB file, and reset state.
///
/// Call `start_indexing()` to create a fresh index afterward.
pub fn clear_index() -> Result<(), String> {
    // Invalidate ReadPool before deleting DB files so thread-local connections are discarded.
    if let Some(pool) = READ_POOL.lock().unwrap().take() {
        pool.invalidate();
    }

    let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;

    match std::mem::replace(&mut *guard, IndexPhase::ShuttingDown) {
        IndexPhase::Running(mut mgr) => {
            let db_path = mgr.db_path().to_path_buf();
            mgr.shutdown();
            *guard = IndexPhase::Disabled;

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
        }
        other => {
            *guard = other;
            log::info!("Drive index clear requested but indexing was not active");
        }
    }

    Ok(())
}

// ── Module-level public API (called by IPC commands) ─────────────────

/// Get the current indexing status.
pub fn get_status() -> Result<IndexStatusResponse, String> {
    let guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &*guard {
        IndexPhase::Disabled | IndexPhase::ShuttingDown => Ok(IndexStatusResponse {
            initialized: false,
            scanning: false,
            entries_scanned: 0,
            dirs_found: 0,
            index_status: None,
            db_file_size: None,
        }),
        IndexPhase::Initializing { store, .. } => {
            let db_file_size = store.db_file_size().ok();
            let index_status = store.get_index_status().ok();
            Ok(IndexStatusResponse {
                initialized: true,
                scanning: true,
                entries_scanned: 0,
                dirs_found: 0,
                index_status,
                db_file_size,
            })
        }
        IndexPhase::Running(mgr) => mgr.get_status(),
    }
}

/// Get extended debug status for the debug window.
pub fn get_debug_status() -> Result<IndexDebugStatusResponse, String> {
    let guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &*guard {
        IndexPhase::Disabled | IndexPhase::ShuttingDown => {
            let base = IndexStatusResponse {
                initialized: false,
                scanning: false,
                entries_scanned: 0,
                dirs_found: 0,
                index_status: None,
                db_file_size: None,
            };
            Ok(IndexDebugStatusResponse {
                base,
                watcher_active: false,
                live_event_count: 0,
                must_scan_count: 0,
                must_scan_rescans_completed: 0,
                live_entry_count: None,
                live_dir_count: None,
                dirs_with_stats: None,
                recent_must_scan_paths: Vec::new(),
            })
        }
        IndexPhase::Initializing { store, .. } => {
            let db_file_size = store.db_file_size().ok();
            let index_status = store.get_index_status().ok();
            let base = IndexStatusResponse {
                initialized: true,
                scanning: true,
                entries_scanned: 0,
                dirs_found: 0,
                index_status,
                db_file_size,
            };
            Ok(IndexDebugStatusResponse {
                base,
                watcher_active: DEBUG_STATS.watcher_active.load(Ordering::Relaxed),
                live_event_count: 0,
                must_scan_count: 0,
                must_scan_rescans_completed: 0,
                live_entry_count: None,
                live_dir_count: None,
                dirs_with_stats: None,
                recent_must_scan_paths: Vec::new(),
            })
        }
        IndexPhase::Running(mgr) => mgr.get_debug_status(),
    }
}

/// Look up recursive stats for a single directory.
pub fn get_dir_stats(path: &str) -> Result<Option<DirStats>, String> {
    let mut guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &mut *guard {
        IndexPhase::Running(mgr) => mgr.get_dir_stats(path),
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Batch lookup of dir_stats for multiple paths.
pub fn get_dir_stats_batch(paths: &[String]) -> Result<Vec<Option<DirStats>>, String> {
    let mut guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &mut *guard {
        IndexPhase::Running(mgr) => mgr.get_dir_stats_batch(paths),
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Request a priority micro-scan for a directory.
pub fn prioritize_dir(path: &str, priority: ScanPriority) -> Result<(), String> {
    let guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &*guard {
        IndexPhase::Running(mgr) => {
            mgr.prioritize_dir(path, priority);
            Ok(())
        }
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Cancel current-directory micro-scans.
pub fn cancel_nav_priority(path: &str) -> Result<(), String> {
    let guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &*guard {
        IndexPhase::Running(mgr) => {
            mgr.cancel_nav_priority(path);
            Ok(())
        }
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Force a fresh full scan (for debug/manual trigger).
pub fn force_scan() -> Result<(), String> {
    let mut guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &mut *guard {
        IndexPhase::Running(mgr) => mgr.start_scan(),
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Stop the active scan without shutting down the manager.
pub fn stop_scan() -> Result<(), String> {
    let mut guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &mut *guard {
        IndexPhase::Running(mgr) => {
            mgr.stop_scan();
            Ok(())
        }
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Check whether indexing is active (initializing or running).
pub fn is_active() -> bool {
    INDEXING
        .lock()
        .map(|g| matches!(&*g, IndexPhase::Initializing { .. } | IndexPhase::Running(_)))
        .unwrap_or(false)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::listing::FileEntry;
    use enrichment::{THREAD_CONN, enrich_via_parent_id_on, enrich_via_individual_paths_on};
    use rusqlite::Connection;
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
        let result = enrich_via_parent_id_on(&mut file_entries, store.read_conn(), "/projects");
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
        enrich_via_individual_paths_on(&mut file_entries, store.read_conn());

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
        enrich_via_individual_paths_on(&mut entries, store.read_conn());
    }

    /// Test that enrichment handles entries with no matching index data.
    #[test]
    fn enrich_entries_no_matching_index() {
        let (store, _conn, _dir) = open_temp_store();
        let mut entries = vec![make_file_entry("nonexistent", "/nonexistent", true)];
        enrich_via_individual_paths_on(&mut entries, store.read_conn());
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
        let result = enrich_via_parent_id_on(&mut listing, store.read_conn(), "/home");
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
        let result2 = enrich_via_parent_id_on(&mut listing2, store.read_conn(), "/home");
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

        let result = enrich_via_parent_id_on(&mut listing, store.read_conn(), "/");
        assert!(result.is_ok());

        assert_eq!(listing[0].recursive_size, Some(5000));
        assert_eq!(listing[0].recursive_file_count, Some(1));

        assert_eq!(listing[1].recursive_size, Some(0));
        assert_eq!(listing[1].recursive_dir_count, Some(1));
    }

    // ── ReadPool and contention tests ────────────────────────────────

    /// Helper: populate a temp DB with a small tree and aggregates for ReadPool tests.
    /// Returns (db_path, TempDir). The TempDir must be kept alive to prevent cleanup.
    fn setup_db_for_pool() -> (PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("pool-test.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
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
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(42),
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");
        aggregator::compute_all_aggregates(&conn).expect("aggregates");
        (db_path, dir)
    }

    /// Key regression test: enrichment succeeds even while INDEXING is locked.
    /// Before the ReadPool fix, `enrich_entries_with_index` used `try_lock()` on
    /// INDEXING and silently skipped when the lock was held.
    #[test]
    fn enrichment_under_contention() {
        let (db_path, _dir) = setup_db_for_pool();
        let pool = Arc::new(ReadPool::new(db_path).expect("create pool"));

        // Install pool into READ_POOL so `enrich_entries_with_index` can find it
        *READ_POOL.lock().unwrap() = Some(Arc::clone(&pool));

        // Hold INDEXING.lock() on a background thread for 2 seconds
        let lock_handle = std::thread::spawn(|| {
            let guard = INDEXING.lock().unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(guard);
        });

        // Give the locker thread time to acquire
        std::thread::sleep(Duration::from_millis(50));

        // Enrich on this thread — must succeed despite INDEXING being locked
        let mut entries = vec![make_file_entry("projects", "/projects", true)];
        enrich_entries_with_index(&mut entries);

        assert_eq!(entries[0].recursive_size, Some(42), "enrichment should work under contention");
        assert_eq!(entries[0].recursive_file_count, Some(1));

        lock_handle.join().unwrap();

        // Clean up global state
        *READ_POOL.lock().unwrap() = None;
    }

    /// Thread-local connection reuse: calling `with_conn` twice from the same
    /// thread should reuse the cached connection (same raw pointer).
    #[test]
    fn read_pool_connection_reuse() {
        let (db_path, _dir) = setup_db_for_pool();
        let pool = ReadPool::new(db_path).expect("create pool");

        let ptr1 = pool
            .with_conn(|conn| conn as *const Connection as usize)
            .expect("first call");
        let ptr2 = pool
            .with_conn(|conn| conn as *const Connection as usize)
            .expect("second call");

        assert_eq!(ptr1, ptr2, "same thread should reuse the cached connection");
    }

    /// After `invalidate()`, the next `with_conn` opens a fresh connection.
    #[test]
    fn read_pool_generation_invalidation() {
        let (db_path, _dir) = setup_db_for_pool();
        let pool = ReadPool::new(db_path.clone()).expect("create pool");

        // Warm up the thread-local connection
        pool.with_conn(|_| ()).expect("before invalidation");

        // Verify the cached generation is 0
        let gen_before = THREAD_CONN.with(|cell| {
            cell.borrow().as_ref().map(|(_, g, _)| *g).unwrap()
        });
        assert_eq!(gen_before, 0);

        pool.invalidate();

        // After invalidation, the pool generation is 1 but the cached
        // thread-local still holds generation 0. The next with_conn must
        // detect the mismatch and reopen.
        pool.with_conn(|_| ()).expect("after invalidation");

        let gen_after = THREAD_CONN.with(|cell| {
            cell.borrow().as_ref().map(|(_, g, _)| *g).unwrap()
        });
        assert_eq!(gen_after, 1, "invalidation should force a new connection with bumped generation");
    }

    /// Multiple threads can call `with_conn` concurrently without errors.
    #[test]
    fn read_pool_cross_thread_reads() {
        let (db_path, _dir) = setup_db_for_pool();
        let pool = Arc::new(ReadPool::new(db_path).expect("create pool"));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let p = Arc::clone(&pool);
                std::thread::spawn(move || {
                    p.with_conn(|conn| {
                        let stats = IndexStore::get_dir_stats_by_id(conn, 2).expect("query");
                        assert!(stats.is_some(), "each thread should read the data");
                        assert_eq!(stats.unwrap().recursive_size, 42);
                    })
                    .expect("with_conn should succeed");
                })
            })
            .collect();

        for h in handles {
            h.join().expect("thread should not panic");
        }
    }

    /// After clearing READ_POOL, `enrich_entries_with_index` returns early
    /// without panic and leaves entries unenriched.
    #[test]
    fn shutdown_enrichment_returns_early() {
        // Ensure READ_POOL is empty (simulate post-shutdown state)
        *READ_POOL.lock().unwrap() = None;

        let mut entries = vec![make_file_entry("stuff", "/stuff", true)];
        enrich_entries_with_index(&mut entries);

        assert_eq!(entries[0].recursive_size, None, "unenriched after shutdown");
    }
}
