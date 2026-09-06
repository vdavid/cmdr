//! Which directories a PANE is showing, for the subsystems that keep something
//! alive while one is open.
//!
//! A [`ListingLifecycle`] observer hears one call when a listing enters the
//! listing cache and one when it leaves. Today's one observer is the virtual
//! `.git` portal, which keeps a repository's `.git/*` watcher armed for as long
//! as a pane is standing in that repository's virtual trees.
//!
//! **Arming belongs to the BACKEND, ❌ never to a frontend subscription.** The
//! breadcrumb chip's `subscribe_git_state` used to be the only thing starting a
//! per-repo watcher, so a pane on `branches/` with nothing else open went stale,
//! and turning both git features off stopped every watcher in the app. An
//! observer here holds for a pane the MCP server drove, for a window that never
//! rendered a chip, and for whatever asks next.
//!
//! ❗ Both calls run on the listing-open and listing-close paths, so an observer
//! does cheap, non-blocking work here and detaches anything else. `git/arming.rs`
//! is the worked example: it decides from the path alone and hands the actual
//! subscribe to the blocking pool.
//!
//! Registration mirrors `listing_overlays.rs`: a subsystem registers one
//! observer at startup, and the listing pipeline calls whatever is registered.

use std::path::Path;
use std::sync::{Arc, LazyLock, RwLock};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::file_system::volume::Volume;

/// A subsystem that cares which directories a pane has open.
pub(crate) trait ListingLifecycle: Send + Sync + 'static {
    /// The observer's stable name (`"git-portal"`), the key the registry dedupes
    /// on.
    fn id(&self) -> &'static str;

    /// A pane opened `path` on `volume`, and the listing cache now holds it
    /// under `listing_id`.
    fn listing_opened(&self, listing_id: &str, volume: &dyn Volume, path: &Path);

    /// The listing `listing_id` left the cache. ❗ The path isn't passed back:
    /// an observer that armed something remembers what, so a close releases
    /// exactly what its own open took, whatever has changed in between.
    fn listing_closed(&self, listing_id: &str);
}

/// The registered observers, in registration order.
static OBSERVERS: LazyLock<RwLock<Vec<Arc<dyn ListingLifecycle>>>> = LazyLock::new(|| RwLock::new(Vec::new()));

/// Files an observer under its `id()`. The first registration per id wins; a
/// duplicate is logged and dropped, so a double setup can't arm twice.
pub(crate) fn register_listing_lifecycle(observer: Arc<dyn ListingLifecycle>) {
    let mut observers = OBSERVERS.write_ignore_poison();
    if observers.iter().any(|o| o.id() == observer.id()) {
        return;
    }
    log::debug!(target: "listing", "registered listing lifecycle observer {}", observer.id());
    observers.push(observer);
}

/// Tells every observer that `listing_id` is now showing `path` on `volume`.
pub(crate) fn listing_opened(listing_id: &str, volume: &dyn Volume, path: &Path) {
    for observer in snapshot() {
        observer.listing_opened(listing_id, volume, path);
    }
}

/// Tells every observer that `listing_id` is gone.
pub(crate) fn listing_closed(listing_id: &str) {
    for observer in snapshot() {
        observer.listing_closed(listing_id);
    }
}

/// A copy of the registered observers, so no observer runs under the lock.
fn snapshot() -> Vec<Arc<dyn ListingLifecycle>> {
    OBSERVERS.read_ignore_poison().clone()
}
