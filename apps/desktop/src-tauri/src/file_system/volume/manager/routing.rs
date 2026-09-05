//! Which volume serves a path: [`VolumeManager::resolve`] and what it answers.
//!
//! The registry proper (hold volumes by ID) is the parent `manager` module.
//! This is the dispatcher above the two ROUTES that can swap the volume out
//! from under a path: the git portal (`git_routing.rs`) and archives
//! (`archive_routing.rs`). Both mint a read-only volume on demand, register it,
//! and cap how many stay registered; both hand the caller's path back verbatim.
//!
//! It's another inherent `impl VolumeManager` block (a type's impl can span
//! files within a crate), so every method stays at `VolumeManager::…`.

use super::super::Volume;
use super::VolumeManager;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Which route swapped the volume in, when one did.
///
/// Both variants mean the same thing to most readers — "this isn't the drive
/// the caller named, it's a read-only volume mapping a namespace onto it" — so
/// most sites ask [`ResolvedVolume::is_routed`]. Match a variant only where the
/// answer is genuinely about that ONE backend (the archive-edit driver, the
/// archive preview, the viewer's extract path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutedKind {
    /// The path crossed a `.zip` boundary into an [`ArchiveVolume`](cmdr_archive::ArchiveVolume).
    Archive,
    /// The path reached into a repo's virtual `.git` trees, so a
    /// [`GitPortalVolume`](cmdr_git::GitPortalVolume)
    /// serves it.
    GitPortal,
}

/// Outcome of [`VolumeManager::resolve`]: the volume that should serve `path`.
///
/// `path` is ALWAYS the caller's input path unchanged. A routed resolve only
/// swaps in the read-only volume — which maps the full path into its own
/// namespace itself — and a passthrough returns the requested volume untouched.
/// Adoption sites read `resolved.path` so the "full path, unchanged" contract
/// lives in exactly one place.
pub struct ResolvedVolume {
    /// The volume to call, or `None` when `volume_id` isn't registered (an
    /// unmount race). Sites keep their existing `.ok_or_else(...)?` handling.
    pub volume: Option<Arc<dyn Volume>>,
    /// The path to pass to `volume`'s methods — the input path, verbatim.
    pub path: PathBuf,
    /// Which route swapped `volume` in, or `None` for a passthrough. Sites use
    /// it to skip drive-index enrich/verify and the read-only write guards.
    pub routed: Option<RoutedKind>,
}

/// Whether `path` could be served by a ROUTE rather than by the volume that
/// physically holds it. Pure string work over path segments: no `stat`, no
/// network, and deliberately permissive, because
/// [`VolumeManager::resolve`] is the authoritative answer.
///
/// True for exactly the paths with no file of their own on the parent volume: a
/// non-empty archive-inner path, and anything inside a repo's virtual `.git`
/// trees while the portal is switched on. ❗ The `.zip` FILE itself is NOT one of
/// them — it's an ordinary file, and a copy, a move, and the viewer all have to
/// treat it as bytes on disk.
///
/// Call sites use it as the cheap gate in front of an `await`ed `resolve`, so an
/// ordinary local or remote path pays nothing; whoever then acts on the answer
/// reads [`ResolvedVolume::routed`], never this.
pub fn path_routes_over_its_parent(path: &Path) -> bool {
    let inside_an_archive =
        cmdr_archive::archive_boundary_candidate(path).is_some_and(|(_zip, inner)| !inner.as_os_str().is_empty());
    inside_an_archive || crate::file_system::git::wiring::portal_serves(path)
}

impl ResolvedVolume {
    /// An unrouted resolve: the requested volume (if any), path unchanged.
    pub(super) fn passthrough(volume: Option<Arc<dyn Volume>>, path: &Path) -> Self {
        Self {
            volume,
            path: path.to_path_buf(),
            routed: None,
        }
    }

    /// Whether a route swapped the volume in. True for every read-only routed
    /// volume, whichever kind: a routed path has no drive-index entry and takes
    /// no writes, and both of those follow from being routed at all.
    pub fn is_routed(&self) -> bool {
        self.routed.is_some()
    }
}

impl VolumeManager {
    /// Path-aware volume lookup: routes a path that reaches into a repo's
    /// virtual `.git` trees to its read-only `GitPortalVolume`, and one that
    /// crosses a `.zip` boundary to its read-only `ArchiveVolume`, registering
    /// either on demand. Everything else is a plain [`get`](Self::get) with the
    /// path unchanged.
    ///
    /// The git check runs FIRST and is pure string work, so a `.zip` sitting
    /// inside a snapshot (`…/.git/branches/main/bundle.zip`) belongs to the
    /// portal rather than to an archive route that would look for a file that
    /// isn't on disk. A `.zip` in the repo's WORKING tree is untouched by it
    /// and routes to the archive as always.
    ///
    /// This is `async` for the archive half alone: confirming a `.zip` boundary
    /// on a REMOTE parent (direct SMB / MTP) costs a `get_metadata` plus a
    /// four-byte `read_range` over the network. See
    /// [`resolve_archive`](Self::resolve_archive).
    ///
    /// Adopt this at every site that did `get(volume_id)` then
    /// `volume.method(path)`. The sync-only
    /// [`resolve_local_only`](Self::resolve_local_only) exists for the one
    /// caller that can't `.await` (the write-op fresh-listing oracle).
    pub async fn resolve(&self, volume_id: &str, path: &Path) -> ResolvedVolume {
        if let Some(routed) = self.resolve_git_portal(volume_id, path) {
            return routed;
        }
        self.resolve_archive(volume_id, path).await
    }

    /// Sync sibling of [`resolve`](Self::resolve) that confirms **only local**
    /// archive boundaries. A remote (direct SMB / MTP) `.zip` path returns a
    /// passthrough (its parent volume, path unchanged), because a remote
    /// confirm needs async I/O this method can't do. Git routing is lexical, so
    /// it's identical here.
    ///
    /// The ONE caller is the write-op fresh-listing oracle
    /// (`listing::caching::try_get_authoritative_listing`), which runs on sync
    /// recursive scan walkers. That oracle guards remote archives separately (a
    /// non-local parent's volume-level `listing_watch_coverage` would falsely
    /// claim freshness), so the local-only routing here is sufficient there.
    pub fn resolve_local_only(&self, volume_id: &str, path: &Path) -> ResolvedVolume {
        if let Some(routed) = self.resolve_git_portal(volume_id, path) {
            return routed;
        }
        self.resolve_local_archive(volume_id, path)
    }
}

/// Records `id` as the most recently resolved entry in a routed-volume LRU and
/// hands back the IDs that fell off the end, for the caller to unregister
/// OUTSIDE the LRU lock (so the LRU and volumes locks are never held at once).
///
/// Shared by the archive and git-portal routes: both mint volumes on demand and
/// both must stay bounded, and a second copy of this would drift.
pub(super) fn touch_routed_lru(lru: &mut VecDeque<String>, id: &str, cap: usize) -> Vec<String> {
    lru.retain(|existing| existing != id);
    lru.push_back(id.to_string());
    let mut evicted = Vec::new();
    while lru.len() > cap {
        if let Some(old) = lru.pop_front() {
            evicted.push(old);
        }
    }
    evicted
}
