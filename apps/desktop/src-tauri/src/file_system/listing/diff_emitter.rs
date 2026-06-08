//! Per-listing `directory-diff` event coalescer.
//!
//! Bulk operations (large delete, copy into a watched dir, MTP event burst) used
//! to emit one `directory-diff` per file. The frontend handler runs ~5 IPC calls
//! per event (`getTotalCount`, `refetchColumnWidths`, `fetchEntryUnderCursor`,
//! `fetchListingStats`, plus a virtual-list re-fetch), so a 5k-file delete drove
//! ~25k IPC calls and made the source pane flicker. This module accumulates
//! changes per listing and flushes one batched event after a short window.
//!
//! Producers call `enqueue_diff(listing_id, changes)`. The cache mutation must
//! still happen synchronously at the call site so `get_file_range` sees the
//! latest state; only the IPC emit is deferred.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use tauri_specta::Event as _;

use crate::file_system::listing::increment_sequence;
use crate::file_system::watcher::{DiffChange, DirectoryDiff, WATCHER_MANAGER};

/// Trailing flush window. Below human perception for single events; at high
/// event rates collapses bursts into at most 1000 / `FLUSH_WINDOW_MS` emits per
/// listing per second.
const FLUSH_WINDOW_MS: u64 = 50;

#[derive(Default)]
struct PendingDiff {
    changes: Vec<DiffChange>,
    flush_scheduled: bool,
}

static PENDING_DIFFS: LazyLock<Mutex<HashMap<String, PendingDiff>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Queues `changes` for `listing_id`. If no flush is pending for this listing,
/// schedules one after `FLUSH_WINDOW_MS`. No-op when `changes` is empty.
///
/// Safe to call from any thread, including the FSEvents debouncer callback
/// (uses `tauri::async_runtime::spawn` for the timer task).
pub(crate) fn enqueue_diff(listing_id: &str, changes: Vec<DiffChange>) {
    if changes.is_empty() {
        return;
    }

    let needs_schedule = {
        let mut pending = match PENDING_DIFFS.lock() {
            Ok(p) => p,
            Err(_) => return,
        };
        let entry = pending.entry(listing_id.to_string()).or_default();
        entry.changes.extend(changes);
        if entry.flush_scheduled {
            false
        } else {
            entry.flush_scheduled = true;
            true
        }
    };

    if needs_schedule {
        let lid = listing_id.to_string();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(FLUSH_WINDOW_MS)).await;
            flush(&lid);
        });
    }
}

/// Drops any pending changes for `listing_id` without emitting. Called when a
/// listing ends (`list_directory_end`) so a no-longer-watched listing doesn't
/// fire a trailing event.
pub(crate) fn drop_pending(listing_id: &str) {
    if let Ok(mut pending) = PENDING_DIFFS.lock() {
        pending.remove(listing_id);
    }
}

fn flush(listing_id: &str) {
    let changes = {
        let mut pending = match PENDING_DIFFS.lock() {
            Ok(p) => p,
            Err(_) => return,
        };
        let Some(entry) = pending.get_mut(listing_id) else {
            return;
        };
        entry.flush_scheduled = false;
        std::mem::take(&mut entry.changes)
    };

    if changes.is_empty() {
        return;
    }

    let Some(sequence) = increment_sequence(listing_id) else {
        return; // listing gone
    };

    let app_handle = match WATCHER_MANAGER.read() {
        Ok(m) => m.app_handle.clone(),
        Err(_) => return,
    };
    let Some(app) = app_handle else { return };

    let diff = DirectoryDiff {
        listing_id: listing_id.to_string(),
        sequence,
        changes,
    };
    if let Err(e) = diff.emit(&app) {
        log::warn!("diff_emitter: couldn't emit batched event: {}", e);
    }
}

/// Synchronously flush every pending diff. Used by the E2E `flush_all_watchers`
/// helper so tests don't have to wait out the trailing window. Production code
/// must never call this.
#[cfg(feature = "playwright-e2e")]
pub(crate) fn flush_all_pending() {
    let ids: Vec<String> = match PENDING_DIFFS.lock() {
        Ok(p) => p.keys().cloned().collect(),
        Err(_) => return,
    };
    for id in ids {
        flush(&id);
    }
}

#[cfg(test)]
pub(crate) fn pending_count(listing_id: &str) -> usize {
    PENDING_DIFFS
        .lock()
        .ok()
        .and_then(|p| p.get(listing_id).map(|e| e.changes.len()))
        .unwrap_or(0)
}

#[cfg(test)]
pub(crate) fn flush_now_for_test(listing_id: &str) {
    flush(listing_id);
}
