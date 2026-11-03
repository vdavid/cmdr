//! Restic routing for the [`VolumeManager`]: the part of
//! [`resolve`](VolumeManager::resolve) that sends a path at or below a restic
//! repo's `Browse snapshots` row to a read-only
//! [`ResticVolume`](cmdr_restic::ResticVolume), plus the LRU that caps how many
//! stay registered.
//!
//! The candidate is lexical (`cmdr_fs::restic_view`); the verdict is the hub's
//! memo, seeded for free by the listing overlay. The dispatcher that orders this
//! route against git and archives is `routing.rs`.

use super::VolumeManager;
use crate::file_system::restic::wiring;
use crate::file_system::volume::Volume;
use crate::file_system::volume::{ResolvedVolume, RoutedKind};
use crate::ignore_poison::IgnorePoison;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

/// Max number of `ResticVolume`s kept registered at once. A view is cheap (the
/// hub holds every password, opened repo, and index), so this bounds
/// registrations only: eviction never drops an index or forgets a password,
/// and the next navigation re-registers.
const RESTIC_LRU_CAP: usize = 4;

/// The registered restic views: their recency (front = oldest), and the parent
/// each was minted over.
///
/// The parent is remembered because one can be REPLACED under the same volume
/// id (an SMB share reconnecting, an OS mount upgraded to a direct session),
/// and a view minted over the retired one would read through it forever.
/// `Weak`, so remembering a parent never keeps a retired volume alive.
// DEFAULT-OK: empty is the truthful start: no view is registered yet.
#[derive(Default)]
pub(super) struct ResticRoutes {
    lru: VecDeque<String>,
    parents: HashMap<String, Weak<dyn Volume>>,
}

/// What the restic route answers for one candidate boundary.
pub(super) enum ResticAnswer {
    /// A confirmed repo: its view serves the path.
    Routed(ResolvedVolume),
    /// Not a repo (or not browsable here): the next route decides.
    PassOn,
    /// The parent couldn't say: the next route decides, and a write refuses.
    Undecided,
}

impl VolumeManager {
    /// Routes `path` to the view of the repo at `repo_root` when the hub
    /// confirms it's a repo; [`ResticAnswer::PassOn`] hands over to the next
    /// route, and [`ResticAnswer::Undecided`] does too but marks the answer so a
    /// write refuses (the parent failed or outran the confirm's deadline).
    ///
    /// A miss in the hub's memo costs ONE bounded listing of `repo_root` on the
    /// parent, remembered both ways, so a real folder named `Browse snapshots`
    /// outside a repo pays it once, not per call.
    pub(super) async fn resolve_restic(&self, volume_id: &str, repo_root: &Path, path: &Path) -> ResticAnswer {
        let Some(parent) = self.get(volume_id) else {
            return ResticAnswer::PassOn;
        };
        if parent.routes_over_a_parent() {
            // A repo backed up inside a snapshot, or zipped in an archive, isn't
            // browsable a second level down; the overlay never offers it either.
            return ResticAnswer::PassOn;
        }
        match wiring::repos().confirm_repo(volume_id, &parent, repo_root).await {
            Some(true) => ResticAnswer::Routed(self.register_restic(volume_id, parent, repo_root, path)),
            Some(false) => ResticAnswer::PassOn,
            None => ResticAnswer::Undecided,
        }
    }

    /// [`resolve_restic`](Self::resolve_restic) from the memo alone, for the
    /// synchronous `resolve_local_only`: ❌ no confirm listing here, since "local"
    /// includes an OS-mounted share, where a `read_dir` on a wedged mount blocks a
    /// scan walker for minutes. A miss passes through; the fresh-listing oracle
    /// declines an unrouted path under the view, so that only costs a re-read.
    pub(super) fn resolve_restic_from_memo(
        &self,
        volume_id: &str,
        repo_root: &Path,
        path: &Path,
    ) -> Option<ResolvedVolume> {
        if wiring::repos().serves_confirmed(volume_id, repo_root) != Some(true) {
            return None;
        }
        let parent = self.get(volume_id)?;
        if parent.routes_over_a_parent() {
            return None;
        }
        Some(self.register_restic(volume_id, parent, repo_root, path))
    }

    /// Registers (or reuses) the view over `repo_root`, bumps the LRU, and
    /// returns it resolved. A view minted over a parent that's since been
    /// replaced is minted again over the current one.
    fn register_restic(
        &self,
        volume_id: &str,
        parent: Arc<dyn Volume>,
        repo_root: &Path,
        path: &Path,
    ) -> ResolvedVolume {
        let view_id = restic_volume_id(volume_id, repo_root);
        let current = {
            let routes = self.restic_routes.lock_ignore_poison();
            routes
                .parents
                .get(&view_id)
                .is_some_and(|minted_over| Weak::ptr_eq(minted_over, &Arc::downgrade(&parent)))
        };
        if !current || self.get(&view_id).is_none() {
            let view = Arc::new(cmdr_restic::ResticVolume::new(
                Arc::clone(wiring::repos()),
                volume_id,
                Arc::clone(&parent),
                repo_root.to_path_buf(),
            ));
            // `register`, not `register_if_absent`: same root, so it swaps the
            // volume in place when the parent changed.
            self.register(&view_id, view);
            self.restic_routes
                .lock_ignore_poison()
                .parents
                .insert(view_id.clone(), Arc::downgrade(&parent));
        }
        self.touch_restic_lru(&view_id);

        match self.get(&view_id) {
            Some(volume) => ResolvedVolume {
                volume: Some(volume),
                path: path.to_path_buf(),
                routed: Some(RoutedKind::Restic),
                unconfirmed: None,
            },
            // Registered then evicted before we could read it back (only
            // reachable under a pathologically small cap).
            None => ResolvedVolume::passthrough(Some(parent), path),
        }
    }

    /// Records `id` as the most recently resolved view and unregisters the
    /// least recently resolved ones past [`RESTIC_LRU_CAP`], OUTSIDE the LRU
    /// lock, so the LRU and volumes locks are never held at once.
    fn touch_restic_lru(&self, id: &str) {
        let evicted = {
            let mut routes = self.restic_routes.lock_ignore_poison();
            let evicted = super::routing::touch_routed_lru(&mut routes.lru, id, RESTIC_LRU_CAP);
            for old in &evicted {
                routes.parents.remove(old);
            }
            evicted
        };
        for old in evicted {
            self.unregister(&old);
        }
    }
}

/// The registry id for the view over `repo_root` on `volume_id`:
/// `restic-{hash(volume id, repo root)}`. Backend-internal only (the frontend
/// holds the parent drive id), and lexical, ❌ no `canonicalize`: it's a `stat`,
/// on a network parent a slow one, and the hub keys its repos the same way.
pub(super) fn restic_volume_id(volume_id: &str, repo_root: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    volume_id.hash(&mut hasher);
    repo_root.hash(&mut hasher);
    format!("restic-{:016x}", hasher.finish())
}

/// The repo roots `path` could browse into, left to right. Re-exported so the
/// dispatcher reads one name.
pub(super) fn restic_candidates(path: &Path) -> Vec<PathBuf> {
    cmdr_fs::restic_view::restic_route_candidates(path).collect()
}

#[cfg(test)]
#[path = "restic_routing_tests.rs"]
mod restic_routing_tests;
