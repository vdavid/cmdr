//! Which repositories the open listings keep watched.
//!
//! A pane standing in a repo's virtual `.git` trees is the reason that repo's
//! `.git/*` watcher exists, so arming follows the LISTING. The observer takes a
//! subscriber on the repository when such a listing opens and gives it back when
//! it closes, sharing the refcount the breadcrumb chip's `subscribe_git_state`
//! already uses: a working-tree pane and a `.git/` pane on one repo cost one
//! watcher between them.
//!
//! Why here and not in the frontend: `src/lib/file-explorer/pane/git-browser-sync.svelte.ts`
//! subscribes only while the chip or the status column is switched on, and only
//! for the repo it looked up. So a lone `branches/` pane, an MCP-driven pane, and
//! a window with both git features off all had nothing arming the watcher, and
//! the pane sat on whatever the refs said when it opened.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;
use crate::listing_lifecycle::{ListingLifecycle, register_listing_lifecycle};

use super::wiring;
use crate::file_system::volume::Volume;

/// The repository each open listing holds a subscriber on.
///
/// Keyed by listing id rather than derived from the path again at close time, so
/// a release gives back exactly what its own open took: the portal toggle, or
/// the repository itself, may have moved in between.
static ARMED: LazyLock<Mutex<HashMap<String, PathBuf>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Keeps a repo's watcher armed for as long as a pane is showing one of its
/// virtual `.git` listings.
pub struct GitPortalListings;

/// Wires the observer into the listing pipeline. Idempotent, so a test can call
/// it beside the app's startup call.
pub fn register() {
    register_listing_lifecycle(Arc::new(GitPortalListings));
}

impl ListingLifecycle for GitPortalListings {
    fn id(&self) -> &'static str {
        "git-portal"
    }

    fn listing_opened(&self, listing_id: &str, volume: &dyn Volume, path: &Path) {
        let Some(worktree_root) = repo_a_listing_watches(volume, path) else {
            return;
        };
        arm_detached(listing_id.to_string(), worktree_root);
    }

    fn listing_closed(&self, listing_id: &str) {
        disarm(listing_id);
    }
}

/// The repository a listing on `path` keeps watched, or nothing when the listing
/// isn't the portal's.
///
/// ❗ Pure string and flag work, no `stat` and no repository open: this runs on
/// every listing in the app, the same rule the overlay's predicate keeps.
/// Whether that `.git` belongs to a repository `gix` can open is answered by the
/// subscribe itself, off this thread.
///
/// Two shapes qualify, and they're the two the portal serves:
///
/// - **A virtual tree** (`<repo>/.git/branches/…`). The route only sends a path
///   here when the parent volume holds real repos, so the volume needs no second
///   look.
/// - **The repo's own `.git/`**, whose six category rows carry live counts
///   ("12 branches"), so a `git branch` has to reach it. That listing is the
///   local volume's, so the volume DOES get asked: `gix` can't open a path only
///   a protocol can reach.
pub(crate) fn repo_a_listing_watches(volume: &dyn Volume, path: &Path) -> Option<PathBuf> {
    if !wiring::is_virtual_portal_enabled() {
        return None;
    }
    let worktree_root = wiring::repo_a_listing_shows(path)?;
    // A path inside one of the six trees only got here through the route, which
    // already asked whether the parent volume holds real repos. The `.git/`
    // landing listing is the local volume's, so that one still gets asked.
    if cmdr_git::portal_route(path).is_none() && !wiring::volume_holds_real_repos(volume) {
        return None;
    }
    Some(worktree_root)
}

/// Takes the subscriber on the blocking pool, then hands it back if the listing
/// ended while we were taking it.
///
/// **Detached for the same reason `watcher::start_watching_detached` is**:
/// arming is a chain of blocking syscalls (a repository open, then one FSEvents
/// stream per watched `.git/*` path), and a listing open must not sit on a
/// runtime worker waiting for them. `tauri::async_runtime` rather than `tokio`
/// because the sync listing-start path reaches this from the IPC handler thread,
/// which has no Tokio runtime context.
///
/// **The reconcile half is not optional.** `list_directory_end` runs
/// [`disarm`] against a map this arm may not have written yet, so without it the
/// arm lands on a listing nobody will ever close again and the repo keeps a
/// watcher, an open `gix` handle, and a status snapshot for the life of the
/// process. Listing-cache membership is the liveness signal, exactly as it is
/// for the FSEvents arm.
fn arm_detached(listing_id: String, worktree_root: PathBuf) {
    tauri::async_runtime::spawn_blocking(move || {
        let root = match wiring::portal().watch_repo(&worktree_root) {
            Ok(root) => root,
            Err(e) => {
                // Not a repository this listing's path belongs to after all (a
                // directory merely named `.git`, a gitdir we can't open). The
                // pane still lists; there's nothing to keep fresh.
                log::debug!(target: "git", "no watcher for {}: {e}", worktree_root.display());
                return;
            }
        };
        ARMED.lock_ignore_poison().insert(listing_id.clone(), root);

        if crate::file_system::listing::caching::get_listing_path(&listing_id).is_none() {
            log::debug!(target: "git", "listing {listing_id} ended while arming its repo watcher, releasing it");
            disarm(&listing_id);
        }
    });
}

/// Gives back whatever `listing_id` armed. Idempotent: the reconcile above and
/// the close IPC both call it, and the map removal decides which one releases.
fn disarm(listing_id: &str) {
    let armed = ARMED.lock_ignore_poison().remove(listing_id);
    if let Some(root) = armed {
        wiring::portal().unsubscribe_state(&root);
    }
}
