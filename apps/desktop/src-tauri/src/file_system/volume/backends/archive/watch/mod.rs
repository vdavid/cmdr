//! Live content watch on the backing `.zip` file.
//!
//! When an `ArchiveVolume` registers, it starts a watch so an external edit to
//! the archive (an editor rewriting it, a `cp` over it, an in-app mutation's
//! final rename) refreshes any open listing inside the zip.
//!
//! ## Why watch the parent directory, not the file
//!
//! macOS editors and every safe-overwrite (including this app's own temp+rename)
//! replace the file's inode: they write `foo.zip.tmp`, then atomically rename it
//! over `foo.zip`. A `notify` watch pinned to the OLD inode goes silent after
//! such a swap. Watching the archive's PARENT DIRECTORY (non-recursive) tracks
//! the stable directory inode instead, so it keeps firing across inode swaps with
//! no re-arming; we filter the directory's events down to the archive path. This
//! mirrors the local listing watcher (`file_system::watcher::start_watching`),
//! which watches a directory non-recursively for the same reason.
//!
//! ## Lifecycle (leak-free by construction)
//!
//! The [`ArchiveContentWatch`] handle lives in the `ArchiveVolume`. When the
//! archive LRU evicts the volume (or the app tears down), the volume's `Arc`
//! drops, this handle drops, and the `Debouncer`'s own `Drop` stops the OS watch.
//! [`active_watch_count`] observes the live-handle count so tests can prove no
//! watcher leaks past eviction.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use notify_debouncer_full::{
    DebounceEventResult, Debouncer, RecommendedCache, new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
};

use super::ArchiveIndexCache;
use crate::indexing::paths::firmlinks;

/// Debounce window for archive content events. Matches the listing watcher's
/// default so a burst of writes during a rewrite collapses into one refresh.
const ARCHIVE_WATCH_DEBOUNCE: Duration = Duration::from_millis(200);

/// Count of live archive content watches. Incremented when a watch starts,
/// decremented when its handle drops (eviction or teardown). Tests assert it
/// returns to zero to prove eviction leaks no watchers.
static ACTIVE_ARCHIVE_WATCHES: AtomicUsize = AtomicUsize::new(0);

/// Number of archive content watches currently live. For tests and diagnostics.
pub fn active_watch_count() -> usize {
    ACTIVE_ARCHIVE_WATCHES.load(Ordering::SeqCst)
}

/// Handle for a live archive content watch. Holding it keeps the watch running;
/// dropping it stops the OS watch (via the `Debouncer`'s `Drop`) and releases the
/// live-count slot.
pub struct ArchiveContentWatch {
    #[allow(dead_code, reason = "held only to keep the debouncer (and its OS watch) alive")]
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl Drop for ArchiveContentWatch {
    fn drop(&mut self) {
        ACTIVE_ARCHIVE_WATCHES.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Starts a content watch on `archive_path`'s parent directory, filtering events
/// down to the archive file. On a matching change it drops the stale parsed index
/// (`cache.clear()`, releasing the old `Arc`) and refreshes every open listing
/// inside the zip through `refresh_archive_listings`.
///
/// Returns `None` when the watch can't be established (no parent directory, or
/// `notify` refuses the path) — the caller then leaves `listing_is_watched`
/// reporting `false`, so a listing never claims freshness it can't back.
///
/// `parent_volume_id` is the archive's PARENT DRIVE id (the id the listing cache
/// keys on); the refresh re-resolves `(parent_volume_id, inner_path)` back to this
/// archive. `cache` is the volume's own `ArchiveIndexCache`, shared so the
/// callback can invalidate it.
pub fn start_watch(
    archive_path: PathBuf,
    parent_volume_id: String,
    cache: Arc<ArchiveIndexCache>,
) -> Option<ArchiveContentWatch> {
    let watch_dir = archive_path.parent()?.to_path_buf();

    let mut debouncer = new_debouncer(ARCHIVE_WATCH_DEBOUNCE, None, move |result: DebounceEventResult| {
        // The watched directory reports events for ALL its children; act only
        // when the batch touches our archive file. An `Err` typically means
        // the watched directory itself went away — nothing to refresh.
        let Ok(events) = result else { return };
        let touched = events
            .iter()
            .flat_map(|event| event.paths.iter())
            .any(|path| event_path_targets_archive(path, &archive_path));
        if !touched {
            return;
        }

        // Drop the stale parsed index so the re-read re-parses the new bytes
        // (the `(path, size, mtime)` key would miss anyway, but clearing also
        // releases the old `Arc` instead of leaking one index per edit).
        cache.clear();

        // The debouncer callback runs on notify-rs's own thread, which has no
        // Tokio runtime — `tokio::spawn` would panic. `tauri::async_runtime`
        // works from any thread (same rule as `file_system::watcher`).
        let volume_id = parent_volume_id.clone();
        let archive_path = archive_path.clone();
        tauri::async_runtime::spawn(async move {
            crate::file_system::listing::caching::refresh_archive_listings(&volume_id, &archive_path).await;
        });
    })
    .map_err(|e| log::warn!("archive watch: failed to create debouncer: {e}"))
    .ok()?;

    debouncer
        .watch(&watch_dir, RecursiveMode::NonRecursive)
        .map_err(|e| log::warn!("archive watch: failed to watch {}: {e}", watch_dir.display()))
        .ok()?;

    ACTIVE_ARCHIVE_WATCHES.fetch_add(1, Ordering::SeqCst);
    Some(ArchiveContentWatch { debouncer })
}

/// Whether a directory-watch event path refers to the archive file.
///
/// Compares on the firmlink/symlink-normalized forms, because on macOS FSEvents
/// reports canonical paths (`/private/tmp/…`) while the archive path is the
/// user-navigated form (`/tmp/…`) — the same rebasing the listing watcher does in
/// `rebase_event_path`. A raw comparison would miss every event for archives
/// under `/tmp`, `/var`, or `/etc`.
fn event_path_targets_archive(event_path: &Path, archive_path: &Path) -> bool {
    if event_path == archive_path {
        return true;
    }
    let event_normalized = firmlinks::normalize_path(&event_path.to_string_lossy());
    let archive_normalized = firmlinks::normalize_path(&archive_path.to_string_lossy());
    event_normalized == archive_normalized
}

#[cfg(test)]
mod watch_integration_test;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_matches_the_exact_archive_path() {
        assert!(event_path_targets_archive(
            Path::new("/Users/jane/docs/bundle.zip"),
            Path::new("/Users/jane/docs/bundle.zip"),
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn event_matches_across_the_private_firmlink() {
        // FSEvents reports the canonical `/private/tmp/…`; the archive was
        // navigated as `/tmp/…`. Without normalization the watch never fires.
        assert!(event_path_targets_archive(
            Path::new("/private/tmp/work/bundle.zip"),
            Path::new("/tmp/work/bundle.zip"),
        ));
    }

    #[test]
    fn event_ignores_a_sibling_file_in_the_watched_directory() {
        // The parent-directory watch reports every child; a sibling must not
        // trigger an archive refresh.
        assert!(!event_path_targets_archive(
            Path::new("/Users/jane/docs/other.txt"),
            Path::new("/Users/jane/docs/bundle.zip"),
        ));
    }

    #[test]
    fn event_ignores_a_prefix_similar_sibling() {
        // `bundle.zip.tmp` (an editor's temp) and `bundle.zipper` share a prefix
        // but aren't the archive; only the exact file counts.
        assert!(!event_path_targets_archive(
            Path::new("/Users/jane/docs/bundle.zip.tmp"),
            Path::new("/Users/jane/docs/bundle.zip"),
        ));
        assert!(!event_path_targets_archive(
            Path::new("/Users/jane/docs/bundle.zipper"),
            Path::new("/Users/jane/docs/bundle.zip"),
        ));
    }
}
