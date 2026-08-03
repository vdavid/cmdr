//! The per-volume read-handle table the read path routes through.
//!
//! Both read handles a volume owns — its [`ReadPool`](super::enrichment::ReadPool)
//! and its [`PendingSizes`](super::pending_sizes::PendingSizes) — live in one of
//! these, keyed by volume id. Lifecycle PUSHES a handle in when it reserves a
//! volume's slot and takes it back out when the volume stops; the read path only
//! ever looks one up.
//!
//! ## Why a table here rather than a lookup into the registry
//!
//! Reading a handle out of `lifecycle::state`'s `INDEX_REGISTRY` would make the
//! read path — the hottest path in the subsystem, run on every listing — depend
//! on the lifecycle module, and lifecycle already depends on these handles to
//! hand them out. That is a two-way dependency between a hot read path and a
//! mutex that teardown holds while it works. Pushing instead of pulling keeps
//! the read side underneath lifecycle, so `INDEX_REGISTRY` guards lifecycle only.
//!
//! ## Lock discipline (the part that must not regress)
//!
//! This `RwLock` is a LEAF: every operation below is a hash lookup plus an `Arc`
//! clone, and NOTHING is called while the guard is alive. ❌ Never add a callback
//! parameter, a `log::` call that can format a handle, or any other reach-out
//! here. That is what lets lifecycle install a handle while holding
//! `INDEX_REGISTRY` (registry → table) without ever creating the reverse order,
//! and it's why a reader can never be parked behind a teardown.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

/// A volume-id-keyed table of one kind of read handle.
///
/// Declare one as a `LazyLock` static, then `install` / `uninstall` from the
/// lifecycle side and `get` from the read side.
pub(super) struct VolumeHandles<T> {
    by_volume: RwLock<HashMap<String, Arc<T>>>,
}

impl<T> VolumeHandles<T> {
    /// An empty table. Declare it in a `LazyLock` static.
    pub(super) fn new() -> Self {
        Self {
            by_volume: RwLock::new(HashMap::new()),
        }
    }

    /// Clone a volume's handle. `None` means "no index registered for this
    /// volume", which is the read path's skip signal.
    pub(super) fn get(&self, volume_id: &str) -> Option<Arc<T>> {
        self.by_volume.read_ignore_poison().get(volume_id).cloned()
    }

    /// Publish a volume's handle, replacing any previous one for that id.
    pub(super) fn install(&self, volume_id: &str, handle: Arc<T>) {
        // Bind whatever `insert` displaces so it drops HERE, after the guard is gone
        // at the end of this statement. Running a replaced handle's destructor under
        // the write lock would break the leaf property this module rests on.
        let replaced = self
            .by_volume
            .write_ignore_poison()
            .insert(volume_id.to_string(), handle);
        drop(replaced);
    }

    /// Withdraw a volume's handle and return it, so the caller can invalidate
    /// what it owns. Once this returns, the read path routes `None` for that
    /// volume: it is the point where reads start skipping.
    pub(super) fn uninstall(&self, volume_id: &str) -> Option<Arc<T>> {
        self.by_volume.write_ignore_poison().remove(volume_id)
    }
}
