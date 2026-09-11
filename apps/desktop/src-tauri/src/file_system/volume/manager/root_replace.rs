//! Swapping the volume under an id across a ROOT change, deliberately.
//!
//! [`VolumeManager::register`] treats a different root under a taken id as an
//! identity conflict and keeps the incumbent, which is right for a filesystem
//! mounted twice and wrong for a saved place whose root a person just edited.
//! This is the one door for the second case. Rationale:
//! `../DETAILS.md` § "Replacing a root in place".

use super::{Volume, VolumeManager};
use crate::ignore_poison::RwLockIgnorePoison;
use std::sync::Arc;

/// What [`VolumeManager::replace_root_in_place`] did.
#[must_use]
pub enum RootReplacement {
    /// `volume` now serves the id at its own root, and the old root is gone from
    /// the entry's root set.
    Replaced {
        /// The instance that served the id until now. ❗ NOT retired: it may
        /// share its connection with the one that replaced it.
        previous: Arc<dyn Volume>,
    },
    /// Nothing is registered under the id, so nothing was replaced and nothing
    /// was registered either.
    NotRegistered,
    /// The id is registered, but to an instance other than the one the caller
    /// checked against: the place disconnected and reconnected meanwhile, so a
    /// fresh volume over a new connection serves it. Nothing was replaced.
    Superseded,
}

impl VolumeManager {
    /// Makes `volume` the one serving `id`, across a root change, retiring nobody,
    /// as long as `expected` still serves it.
    ///
    /// For a successor that shares the incumbent's connection: a saved SFTP or
    /// WebDAV place renamed or re-rooted while it's connected
    /// (`network/DETAILS.md` § "Editing a connected place"). The old active root
    /// leaves the entry's root set, so `find_by_root` and `mount_id_for_path`
    /// stop matching paths only it covered.
    ///
    /// ❗ A compare-and-swap. The successor was built over `expected`'s
    /// connection, so it replaces only that instance: over a volume a reconnect
    /// registered since, it would serve the place from a connection that's
    /// closed, and every request would answer `DeviceDisconnected`.
    ///
    /// ❗ Nothing is retired and nothing is superseded: the incumbent's
    /// `Retirement` is typically the successor's too, and retiring it would stand
    /// the live place's reconnect loop down. A caller replacing a volume over a
    /// FRESH connection wants `connect_wiring::install_retiring_incumbent`
    /// instead, and a filesystem mounted twice wants [`register`].
    ///
    /// Announced to the arrival listeners like any `register`, after the guard
    /// drops. An id nobody registered, or one another instance serves now, is
    /// refused and nothing is added.
    ///
    /// [`register`]: VolumeManager::register
    pub fn replace_root_in_place(
        &self,
        id: &str,
        expected: &Arc<dyn Volume>,
        volume: Arc<dyn Volume>,
    ) -> RootReplacement {
        let previous = {
            let mut volumes = self.volumes.write_ignore_poison();
            let Some(entry) = volumes.get_mut(id) else {
                return RootReplacement::NotRegistered;
            };
            // `Arc::ptr_eq` compares the data pointers only, so two `dyn` handles
            // to one instance match whichever vtable each carries.
            if !Arc::ptr_eq(&entry.volume, expected) {
                return RootReplacement::Superseded;
            }
            entry.replace_root(volume)
        };
        self.announce_arrival(id);
        RootReplacement::Replaced { previous }
    }
}

#[cfg(test)]
#[path = "root_replace_tests.rs"]
mod root_replace_tests;
