//! `IndexManager` — central coordinator for the drive indexing system.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use super::event_loop::{
    JOURNAL_GAP_THRESHOLD, ReplayConfig, WATCHER_CHANNEL_CAPACITY, run_live_event_loop, run_replay_event_loop,
};
use super::events::{
    ActivityPhase, DEBUG_STATS, IndexDebugStatusResponse, IndexDirUpdatedEvent, IndexScanCompleteEvent,
    IndexScanProgressEvent, IndexScanStartedEvent, IndexStatusResponse, PhaseRecord, RescanReason,
    emit_rescan_notification,
};
use super::reconciler::{self, EventReconciler};
use super::scanner::{self, ScanConfig};
use super::store::IndexStore;
use super::watcher::{self, DriveWatcher};
use super::writer::{IndexWriter, WriteMessage};
use super::{INDEXING, IndexPhase};

// ── IndexManager ─────────────────────────────────────────────────────

/// Central coordinator for the drive indexing system.
///
/// Owns the SQLite store (reads), the writer thread (writes), and the scanner handle.
/// Accessed by module-level functions that lock the `INDEXING` static.
pub(crate) struct IndexManager {
    /// Volume ID (for example, "root" for /)
    pub(super) volume_id: String,
    /// Volume root path
    volume_root: PathBuf,
    /// SQLite store for reads
    pub(super) store: IndexStore,
    /// Writer handle for sending writes
    pub(super) writer: IndexWriter,
    /// Handle to the active full scan (if running)
    scan_handle: Option<scanner::ScanHandle>,
    /// FSEvents watcher (started alongside scan, persists after scan completes)
    drive_watcher: Option<DriveWatcher>,
    /// Live event processing task (runs after reconciliation completes).
    /// Shared with spawned async tasks so they can store the handle.
    live_event_task: Arc<std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    /// Tauri app handle for emitting events
    pub(super) app: AppHandle,
    /// Whether a full scan is currently running. Shared with the completion handler.
    pub(super) scanning: Arc<AtomicBool>,
}

impl IndexManager {
    /// Create a new IndexManager for a volume.
    ///
    /// Opens (or creates) the SQLite database, spawns the writer thread,
    /// and initializes the path resolver.
    pub fn new(volume_id: String, volume_root: PathBuf, app: AppHandle) -> Result<Self, String> {
        let data_dir = crate::config::resolved_app_data_dir(&app)?;

        let db_path = data_dir.join(format!("index-{volume_id}.db"));

        let store = IndexStore::open(&db_path).map_err(|e| format!("Failed to open index store: {e}"))?;

        let writer = IndexWriter::spawn(&db_path, Some(app.clone()))
            .map_err(|e| format!("Failed to spawn index writer: {e}"))?;

        log::debug!(
            "IndexManager created for volume '{volume_id}' at {}",
            volume_root.display()
        );

        Ok(Self {
            volume_id,
            volume_root,
            store,
            writer,
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
                    // If the gap is too large, skip replay entirely — replaying tens of
                    // millions of events is slower than a fresh scan. The watcher channel
                    // (32K capacity) has overflow detection as a secondary safety net.
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
                        return self.start_scan("stale index — journal gap too large");
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

        // Determine the trigger string for the scan phase
        let trigger = if status.last_event_id.is_some() && status.scan_completed_at.is_none() {
            "incomplete previous scan"
        } else if status.scan_completed_at.is_some() {
            "full rescan (no event replay on this platform)"
        } else {
            "fresh scan"
        };
        self.start_scan(trigger)
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
                let gap = current_id.saturating_sub(since_event_id);
                DEBUG_STATS.set_phase(
                    ActivityPhase::Replaying,
                    &format!("app launch, ~{gap} pending FSEvents"),
                );
                log::info!("Replay: watcher started (since_event_id={since_event_id}, current={current_id})");
            }
            Err(e) => {
                emit_rescan_notification(
                    &self.app,
                    &self.volume_id,
                    RescanReason::WatcherStartFailed,
                    format!("DriveWatcher failed to start for replay: {e}"),
                );
                return self.start_scan("watcher failed to start for replay");
            }
        }

        // Estimated total events for progress reporting (approximate: not all IDs
        // in the range belong to our volume)
        let estimated_total = if current_id > since_event_id {
            Some(current_id - since_event_id)
        } else {
            None
        };

        // Suppress verifier until replay completes. The spawned task resets
        // this to false when replay is done (or on fallback to full scan).
        self.scanning.store(true, Ordering::Relaxed);

        // Spawn the replay event processing loop
        let writer = self.writer.clone();
        let app = self.app.clone();
        let volume_id = self.volume_id.clone();
        let live_event_task_slot = Arc::clone(&self.live_event_task);
        let scanning = Arc::clone(&self.scanning);

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
                watcher_overflow,
                Arc::clone(&scanning),
            )
            .await;

            // Live event loop ended (shutdown). Clear scanning as a safety net
            // (normally cleared inside run_replay_event_loop after replay phase).
            scanning.store(false, Ordering::Relaxed);

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

                    if let Err(e) = mgr.start_scan("journal replay detected gap") {
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
    pub fn start_scan(&mut self, scan_trigger: &str) -> Result<(), String> {
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

        DEBUG_STATS.set_phase(ActivityPhase::Scanning, scan_trigger);

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

                    DEBUG_STATS.close_phase_with_stats(vec![
                        ("entries", summary.total_entries.to_string()),
                        ("dirs", summary.total_dirs.to_string()),
                        ("duration_s", format!("{:.1}", summary.duration_ms as f64 / 1000.0)),
                    ]);
                    DEBUG_STATS.set_phase(ActivityPhase::Aggregating, "post-scan");

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
                    if let Some(ref flag) = watcher_overflow_flag
                        && flag.load(Ordering::Relaxed)
                    {
                        log::info!(
                            "FSEvents channel overflowed during scan — some watcher \
                                 events were dropped. Live event loop will trigger a rescan."
                        );
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

                    DEBUG_STATS.close_phase_with_stats(vec![]);
                    DEBUG_STATS.set_phase(ActivityPhase::Reconciling, "post-scan");

                    // Tell the frontend to refresh all visible listings — directory
                    // sizes are now available for the first time after a full scan.
                    let _ = app.emit(
                        "index-dir-updated",
                        IndexDirUpdatedEvent {
                            paths: vec!["/".to_string()],
                        },
                    );

                    // Store scan metadata now, before the reconciler replay which
                    // can fail (e.g. "database is locked") and cause an early return.
                    // Without this, scan_completed_at is never persisted and the next
                    // startup triggers a full rescan of the entire volume.
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

                    // Open a read connection for path resolution during replay
                    let replay_conn = match IndexStore::open_read_connection(&writer.db_path()) {
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
                    match reconciler.replay(scan_start_event_id, &replay_conn, &writer, &mut |paths| {
                        reconciler::emit_dir_updated(&app, paths)
                    }) {
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

                    DEBUG_STATS.close_phase_with_stats(vec![("buffered_events", buffered_count.to_string())]);
                    DEBUG_STATS.set_phase(ActivityPhase::Live, "post-scan reconciliation complete");

                    // Step 5: Start live event processing loop
                    let writer_live = writer.clone();
                    let app_live = app.clone();
                    let volume_id_live = volume_id.clone();
                    let overflow_live = watcher_overflow_flag.clone();
                    let handle = tauri::async_runtime::spawn(async move {
                        run_live_event_loop(
                            event_rx,
                            reconciler,
                            writer_live,
                            app_live,
                            volume_id_live,
                            overflow_live,
                        )
                        .await;
                    });

                    // Store the handle so shutdown() can wait for it to drain
                    {
                        let mut guard = live_event_task_slot.lock().unwrap();
                        *guard = Some(handle);
                    }
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

    /// Stop the active full scan and watcher.
    pub fn stop_scan(&mut self) {
        DEBUG_STATS.set_phase(ActivityPhase::Idle, "stopped");

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

        let (activity_phase, phase_started_at, phase_duration_ms, phase_history) = Self::read_phase_timeline();

        let db_main_size = self.store.db_main_size().ok();
        let db_wal_size = self.store.db_wal_size().ok();
        let (db_page_count, db_freelist_count) = IndexStore::db_page_stats(conn)
            .map(|(p, f)| (Some(p), Some(f)))
            .unwrap_or((None, None));

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
            activity_phase,
            phase_started_at,
            phase_duration_ms,
            phase_history,
            verifying: DEBUG_STATS.verifying.load(Ordering::Relaxed),
            db_main_size,
            db_wal_size,
            db_page_count,
            db_freelist_count,
        })
    }

    /// Read the current phase timeline from DebugStats.
    pub(super) fn read_phase_timeline() -> (ActivityPhase, String, u64, Vec<PhaseRecord>) {
        let history = DEBUG_STATS.phase_history.lock().map(|h| h.clone()).unwrap_or_default();

        let (activity_phase, phase_started_at) = history
            .last()
            .map(|r| (r.phase.clone(), r.started_at.clone()))
            .unwrap_or((ActivityPhase::Idle, String::new()));

        let phase_duration_ms = DEBUG_STATS
            .phase_started
            .lock()
            .ok()
            .and_then(|s| s.map(|i| i.elapsed().as_millis() as u64))
            .unwrap_or(0);

        (activity_phase, phase_started_at, phase_duration_ms, history)
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
        DEBUG_STATS.set_phase(ActivityPhase::Idle, "shutdown");

        // 1. Cancel active scan (but don't abort event loop)
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

        log::info!("IndexManager: shut down for volume '{}'", self.volume_id);
    }
}
