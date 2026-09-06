//! Rows a PANE shows that no volume holds.
//!
//! A [`ListingOverlay`] contributes extra [`FileEntry`]s to one directory's
//! listing, folded in by the listing pipeline after the volume's own entries
//! arrive. Today's one contributor is the virtual `.git` portal, which puts the
//! six category rows (`branches/`, `tags/`, …) into a repo's `.git/` listing
//! while every real entry beside them stays the local volume's, editable and
//! deletable as always.
//!
//! ❌ **Never move this into a `Volume` impl or into the volume manager.** The
//! whole point is that a contributed row reaches a pane and NOTHING else: a copy
//! scan, a delete walker, and the drive indexer all list through `Volume`, and
//! the moment one of them can see a row with no inode behind it, a repo delete
//! stops half-way through with `.git/` still on disk. That was a real bug, and
//! this seam is the shape that makes it unrepresentable.
//!
//! The same rule is why a listing that carries contributed rows is never
//! authoritative for a walker: `CachedListing::has_overlay_rows` says so and the
//! fresh-listing oracle declines it (`listing/caching.rs`).
//!
//! Registration mirrors `device_volumes.rs`: a subsystem registers one
//! contributor at startup, and the consumer folds over whatever is registered.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::Volume;

/// A source of extra rows for one kind of directory.
pub(crate) trait ListingOverlay: Send + Sync + 'static {
    /// The contributor's stable name (`"git-portal"`), the key the registry
    /// dedupes on.
    fn id(&self) -> &'static str;

    /// Whether this overlay has anything to say about `path` on `volume`.
    ///
    /// ❗ Cheap and zero-I/O: this runs on EVERY directory listing in the app,
    /// so it has to be a look at the path and the volume's capability flags.
    /// The expensive half is [`extra_entries`](Self::extra_entries), which only
    /// runs when this answered `true`.
    fn applies_to(&self, volume: &dyn Volume, path: &Path) -> bool;

    /// The rows to fold in. Runs on the blocking pool, so it may open a
    /// repository or hit the disk.
    fn extra_entries(&self, volume: &dyn Volume, path: &Path) -> Vec<FileEntry>;
}

/// The registered contributors, in registration order.
static OVERLAYS: LazyLock<RwLock<Vec<Arc<dyn ListingOverlay>>>> = LazyLock::new(|| RwLock::new(Vec::new()));

/// Files a contributor under its `id()`. The first registration per id wins; a
/// duplicate is logged and dropped, so a double setup can't contribute the same
/// rows twice.
pub(crate) fn register_listing_overlay(overlay: Arc<dyn ListingOverlay>) {
    let mut overlays = OVERLAYS.write_ignore_poison();
    if overlays.iter().any(|o| o.id() == overlay.id()) {
        log::warn!(target: "listing", "listing overlay {} registered twice; keeping the first", overlay.id());
        return;
    }
    log::debug!(target: "listing", "registered listing overlay {}", overlay.id());
    overlays.push(overlay);
}

/// A snapshot of the registered contributors.
pub(crate) fn listing_overlays() -> Vec<Arc<dyn ListingOverlay>> {
    OVERLAYS.read_ignore_poison().clone()
}

/// Folds every applicable contributor's rows into `entries`, and reports how
/// many rows they added.
///
/// **A contributed row SHADOWS a real one of the same name.** A repo created by
/// an older git carries a real (deprecated) `.git/branches/` directory, and the
/// portal's `branches` row is the one the user came for; showing two rows called
/// `branches` would be worse than either.
///
/// Sorting happens after this returns, so a contributor hands rows over in any
/// order.
pub(crate) async fn decorate(volume: &Arc<dyn Volume>, path: &Path, entries: &mut Vec<FileEntry>) -> usize {
    let applicable: Vec<Arc<dyn ListingOverlay>> = listing_overlays()
        .into_iter()
        .filter(|overlay| overlay.applies_to(volume.as_ref(), path))
        .collect();
    if applicable.is_empty() {
        return 0;
    }

    let contributed = gather(applicable, Arc::clone(volume), path.to_path_buf()).await;
    if contributed.is_empty() {
        return 0;
    }

    let names: HashSet<&str> = contributed.iter().map(|entry| entry.name.as_str()).collect();
    entries.retain(|entry| !names.contains(entry.name.as_str()));
    let added = contributed.len();
    entries.extend(contributed);
    added
}

/// Runs the applicable contributors on the blocking pool: one of them opens a
/// repository and counts refs, which is not work for an async worker.
///
/// A contributor that panics costs its rows and nothing else. The volume's own
/// entries are already in hand by the time this runs, and a pane showing a
/// repo's real `.git/` contents beats a pane showing a listing error because
/// `gix` tripped over a damaged object.
async fn gather(overlays: Vec<Arc<dyn ListingOverlay>>, volume: Arc<dyn Volume>, path: PathBuf) -> Vec<FileEntry> {
    tokio::task::spawn_blocking(move || {
        overlays
            .iter()
            .flat_map(|overlay| overlay.extra_entries(volume.as_ref(), &path))
            .collect()
    })
    .await
    .unwrap_or_else(|e| {
        log::warn!(target: "listing", "a listing overlay panicked, listing without its rows: {e}");
        Vec::new()
    })
}

#[cfg(test)]
mod tests;
