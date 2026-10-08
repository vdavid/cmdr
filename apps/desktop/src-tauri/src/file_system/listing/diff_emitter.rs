//! Coalesces transport, never coordinate spaces or committed revisions.
//! Producers stamp and enqueue each transition under the listing-cache write lock.
//! Lock order is cache then queue; draining the queue never reads the cache.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use tauri_specta::Event as _;

#[cfg(test)]
use crate::file_system::listing::diff::DiffChange;
use crate::file_system::listing::diff::{DirectoryDiff, DirectoryDiffBatch};
use crate::file_system::watcher::WATCHER_MANAGER;
use crate::ignore_poison::{IgnorePoison, RwLockIgnorePoison};

const FLUSH_WINDOW_MS: u64 = 50;

// DEFAULT-OK: an empty queue has no flush scheduled.
#[derive(Default)]
struct PendingDiff {
    batches: Vec<DirectoryDiffBatch>,
    flush_scheduled: bool,
}

static PENDING_DIFFS: LazyLock<Mutex<HashMap<String, PendingDiff>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Takes an already stamped transition while its producer holds the cache write lock.
pub(crate) fn enqueue_diff(listing_id: &str, batch: DirectoryDiffBatch) {
    // Projection changes can advance a revision with no rows affected (a scratch
    // dotfile while hidden files are off). Clients still need that revision link.
    let needs_schedule = {
        let mut pending = PENDING_DIFFS.lock_ignore_poison();
        let entry = pending.entry(listing_id.to_string()).or_default();
        entry.batches.push(batch);
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

/// Call under the cache write lock when replacing a row space or ending a listing.
pub(crate) fn drop_pending(listing_id: &str) {
    PENDING_DIFFS.lock_ignore_poison().remove(listing_id);
}

fn take_pending(listing_id: &str) -> Vec<DirectoryDiffBatch> {
    let mut pending = PENDING_DIFFS.lock_ignore_poison();
    let Some(entry) = pending.get_mut(listing_id) else {
        return Vec::new();
    };
    entry.flush_scheduled = false;
    std::mem::take(&mut entry.batches)
}

fn flush(listing_id: &str) {
    let batches = take_pending(listing_id);
    if batches.is_empty() {
        return;
    }
    let app_handle = WATCHER_MANAGER.read_ignore_poison().app_handle.clone();
    let Some(app) = app_handle else { return };
    let diff = DirectoryDiff {
        listing_id: listing_id.to_string(),
        batches,
    };
    if let Err(e) = diff.emit(&app) {
        log::warn!("diff_emitter: couldn't emit batched event: {}", e);
    }
}

#[cfg(feature = "playwright-e2e")]
pub(crate) fn flush_all_pending() {
    let ids: Vec<String> = PENDING_DIFFS.lock_ignore_poison().keys().cloned().collect();
    for id in ids {
        flush(&id);
    }
}

#[cfg(test)]
pub(crate) fn pending_count(listing_id: &str) -> usize {
    pending_changes_for_test(listing_id).len()
}

#[cfg(test)]
pub(crate) fn flush_now_for_test(listing_id: &str) {
    flush(listing_id);
}

#[cfg(test)]
pub(crate) fn take_batches_for_test(listing_id: &str) -> Vec<DirectoryDiffBatch> {
    take_pending(listing_id)
}

#[cfg(test)]
pub(crate) fn hold_for_test(listing_id: &str) {
    PENDING_DIFFS
        .lock_ignore_poison()
        .entry(listing_id.to_string())
        .or_default()
        .flush_scheduled = true;
}

#[cfg(test)]
pub(crate) fn pending_batches_for_test(listing_id: &str) -> Vec<DirectoryDiffBatch> {
    PENDING_DIFFS
        .lock_ignore_poison()
        .get(listing_id)
        .map(|e| e.batches.clone())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn pending_changes_for_test(listing_id: &str) -> Vec<DiffChange> {
    pending_batches_for_test(listing_id)
        .into_iter()
        .flat_map(|b| b.changes)
        .collect()
}
