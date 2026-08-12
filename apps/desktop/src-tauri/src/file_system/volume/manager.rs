//! Volume manager for registering and accessing volumes.
//!
//! The VolumeManager is the central registry for all mounted volumes.
//! It tracks both the available volumes and which one is the current default.
//! The one instance the app runs on lives here too, behind [`get_volume_manager`].

use super::Volume;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, RwLock};

/// Archive routing (`resolve`, `.zip`-boundary predicates, the archive LRU, and
/// [`ResolvedVolume`]) lives in a second `impl VolumeManager` block here.
mod archive_routing;

/// The mount-root set an ID owns, and the promotion rules over it.
mod roots;

use roots::Registration;
pub use roots::{RootRemoval, StaleRootOutcome, is_stale_mount_errno};

#[cfg(test)]
pub(crate) mod test_support;

/// Manages registered volumes and provides access to them.
///
/// Thread-safe registry storing volumes by ID, with support for a default volume.
///
/// An entry is a [`Registration`]: the volume plus EVERY mount root known to
/// carry its ID, one of which is active (`volume.root()`). See `roots.rs`.
pub struct VolumeManager {
    volumes: RwLock<HashMap<String, Registration>>,
    default_volume_id: RwLock<Option<String>>,
    /// Registration recency of the on-demand `ArchiveVolume`s (front = oldest).
    /// A value store: recovering on poison is safe (a lost reorder at worst
    /// evicts slightly early). See [`Self::touch_archive_lru`].
    archive_lru: Mutex<VecDeque<String>>,
}

/// How [`VolumeManager::register`] and [`VolumeManager::register_if_absent`]
/// resolve an identity conflict, for the log line.
const ROOT_RECORDED_RESOLUTION: &str =
    "the existing registration stays active and the new root is recorded as a fallback";

/// Complain loudly when one ID is about to cover two different mount roots.
///
/// A volume ID is identity: it keys the index DB, `lastUsedPaths`, tab state, and
/// operation routing, so two volumes sharing one would send reads (and writes) to
/// the wrong disk. `cmdr_fs::volume::ids` makes that unreachable for every
/// derived ID, which leaves exactly two ways here: a byte-for-byte volume clone
/// (two disks really do report one UUID) and a filesystem mounted twice. Both are
/// genuinely ambiguous, so the registry picks the deterministic answer (keep the
/// incumbent, remember the other root) and says so rather than resolving the
/// ambiguity quietly. What it must never be again is silent.
///
/// The fallback root inherits the same ambiguity: for a double mount it's the
/// same filesystem and promoting to it is exactly right, while for a genuine
/// clone it's a second disk. Neither case is silent, and for the clone the two
/// already share every per-volume store, so which one the ID points at is not
/// the wound.
fn report_identity_conflict(id: &str, existing: &Arc<dyn Volume>, incoming: &Arc<dyn Volume>, resolution: &str) {
    if !is_identity_conflict(existing, incoming) {
        return;
    }
    crate::log_error!(
        target: "cmdr_lib::file_system::volume",
        "Two different mount roots ({} and {}) claim volume ID {id}; {resolution}, so the two share per-volume state. Expected only for a cloned volume or a doubly-mounted filesystem.",
        existing.root().display(),
        incoming.root().display(),
    );
}

/// Whether re-registering `id` means two DIFFERENT mounts, rather than the same
/// mount changing backends (which is routine: that's the SMB upgrade).
fn is_identity_conflict(existing: &Arc<dyn Volume>, incoming: &Arc<dyn Volume>) -> bool {
    existing.root() != incoming.root()
}

impl VolumeManager {
    /// Creates a new empty volume manager.
    pub fn new() -> Self {
        Self {
            volumes: RwLock::new(HashMap::new()),
            default_volume_id: RwLock::new(None),
            archive_lru: Mutex::new(VecDeque::new()),
        }
    }

    /// Registers a volume with the given ID.
    ///
    /// Replacing the volume at the SAME root is routine, and this is how it's
    /// done: that's an OS-mounted SMB share being upgraded in place to a direct
    /// `smb2` session. An IDENTITY CONFLICT (a different root under an ID that's
    /// already taken) keeps the INCUMBENT instead, so registration order stops
    /// deciding where a doubly-mounted filesystem is rooted. Discovery collapses
    /// those mounts before they get here (`volumes::mounts::collapse_by_volume_id`);
    /// this is the registry's own guard, and it stays loud either way.
    pub fn register(&self, id: &str, volume: Arc<dyn Volume>) {
        if let Ok(mut volumes) = self.volumes.write() {
            if let Some(existing) = volumes.get_mut(id) {
                report_identity_conflict(id, &existing.volume, &volume, ROOT_RECORDED_RESOLUTION);
                if is_identity_conflict(&existing.volume, &volume) {
                    existing.record_root(volume.root());
                    return;
                }
                existing.replace_volume(volume);
                return;
            }
            volumes.insert(id.to_string(), Registration::new(volume));
        }
    }

    /// Registers a volume under `id`, replacing whatever is there even across a
    /// root change.
    ///
    /// Only for restoring a registration a test remembered
    /// (`test_support::TestVolumeRegistration`): putting back the previous value
    /// has to be unconditional, since [`register`]'s conflict guard would
    /// otherwise strand the test's own volume in the process-global registry.
    ///
    /// [`register`]: Self::register
    #[cfg(test)]
    pub(crate) fn force_register(&self, id: &str, volume: Arc<dyn Volume>) {
        if let Ok(mut volumes) = self.volumes.write() {
            volumes.insert(id.to_string(), Registration::new(volume));
        }
    }

    /// Registers a volume only if no volume with this ID exists yet.
    ///
    /// Returns `true` if the volume was registered, `false` if a volume
    /// with this ID already exists (the existing volume is kept).
    ///
    /// This is the mount watcher's entry point, so a second mount of an
    /// already-registered filesystem arrives here: the incumbent keeps the ID
    /// and the new mount point is recorded as a fallback root.
    pub fn register_if_absent(&self, id: &str, volume: Arc<dyn Volume>) -> bool {
        if let Ok(mut volumes) = self.volumes.write() {
            use std::collections::hash_map::Entry;
            match volumes.entry(id.to_string()) {
                Entry::Occupied(mut existing) => {
                    let entry = existing.get_mut();
                    report_identity_conflict(id, &entry.volume, &volume, ROOT_RECORDED_RESOLUTION);
                    entry.record_root(volume.root());
                    false
                }
                Entry::Vacant(e) => {
                    e.insert(Registration::new(volume));
                    true
                }
            }
        } else {
            false
        }
    }

    /// Unregisters a volume by ID, dropping every mount root it owned.
    ///
    /// If this was the default volume, the default is cleared.
    pub fn unregister(&self, id: &str) {
        if let Ok(mut volumes) = self.volumes.write() {
            volumes.remove(id);
        }
        self.clear_default_if(id);
    }

    /// Clears the default volume when it was `id`. Touches only
    /// `default_volume_id`, so it's safe to call while holding `volumes`.
    fn clear_default_if(&self, id: &str) {
        if let Ok(default) = self.default_volume_id.read()
            && default.as_deref() == Some(id)
        {
            drop(default); // Release read lock
            if let Ok(mut default) = self.default_volume_id.write() {
                *default = None;
            }
        }
    }

    /// Gets a volume by ID.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Volume>> {
        Some(self.volumes.read().ok()?.get(id)?.volume.clone())
    }

    /// Finds a registered volume by a mount path, matching ANY known root of an
    /// entry, not only the active one.
    ///
    /// Used by the unmount path: when `NSWorkspaceDidUnmount` (macOS) or the
    /// `/proc/mounts` watcher (Linux) fires, `statfs` on the now-gone path can no
    /// longer recover the SMB mount info, so we can't rederive the volume ID from
    /// the path. Looking up by root instead lets us find the entry we registered,
    /// whatever ID it was keyed under. It has to see the fallback roots too: a
    /// second mount of one share is registered under the SAME ID at a DIFFERENT
    /// path, and a sibling the lookup can't see is a sibling nothing can act on.
    ///
    /// The returned volume is the entry's ACTIVE one, which for a fallback-root
    /// hit is rooted somewhere else. Callers that care compare `volume.root()`.
    pub fn find_by_root(&self, root: &Path) -> Option<(String, Arc<dyn Volume>)> {
        self.volumes
            .read()
            .ok()?
            .iter()
            .find(|(_, entry)| entry.knows_root(root))
            .map(|(id, entry)| (id.clone(), Arc::clone(&entry.volume)))
    }

    /// Every mount root known to reach `id`, active one first.
    pub fn known_roots(&self, id: &str) -> Vec<PathBuf> {
        let Ok(volumes) = self.volumes.read() else {
            return Vec::new();
        };
        let Some(entry) = volumes.get(id) else {
            return Vec::new();
        };
        let active = entry.volume.root();
        let mut roots: Vec<PathBuf> = vec![active.to_path_buf()];
        roots.extend(entry.roots.iter().filter(|r| r.path != active).map(|r| r.path.clone()));
        roots
    }

    /// Drop a mount root that has gone away, promoting a survivor when it was
    /// the active one and unregistering only when it was the LAST one.
    ///
    /// This is the unmount path's entry point, and the reason a share mounted
    /// twice survives an eject of either mount. Pure registry work under the
    /// write lock: teardown (`on_unmount`, stopping an index) belongs to the
    /// caller, which is why [`RootRemoval::Unregistered`] hands the volume back.
    pub fn remove_root(&self, root: &Path) -> RootRemoval {
        let Ok(mut volumes) = self.volumes.write() else {
            return RootRemoval::Unknown;
        };
        let Some((id, entry)) = volumes.iter_mut().find(|(_, entry)| entry.knows_root(root)) else {
            return RootRemoval::Unknown;
        };
        let id = id.clone();

        let was_active = entry.volume.root() == root;
        entry.roots.retain(|r| r.path != root);

        if !was_active {
            return RootRemoval::SiblingDropped { id };
        }
        match entry.promote_to_best_root() {
            roots::Promotion::Promoted(new_root) => RootRemoval::Promoted { id, new_root },
            roots::Promotion::BackendCantReroot => RootRemoval::ActiveRootStranded { id },
            // `AlreadyBest` can't follow removing the active root (it's gone from
            // the set), so both remaining arms mean the entry has no roots left.
            roots::Promotion::AlreadyBest | roots::Promotion::NoRootsLeft => {
                let entry = volumes.remove(&id).expect("found under this key a moment ago");
                self.clear_default_if(&id);
                RootRemoval::Unregistered {
                    id,
                    volume: entry.volume,
                }
            }
        }
    }

    /// Record that `root` answered with an errno proving its mount is gone, and
    /// move the ID to a live sibling if there is one.
    ///
    /// The lazy half of "liveness outranks path shape": nothing probes a mount to
    /// find out whether it's alive (that blocks for 30–120 s on a wedged one),
    /// so the evidence arrives as a failed operation and the promotion rides on
    /// it. A root already known stale re-reports cheaply and changes nothing.
    pub fn mark_root_stale(&self, id: &str, root: &Path) -> StaleRootOutcome {
        let Ok(mut volumes) = self.volumes.write() else {
            return StaleRootOutcome::Unchanged;
        };
        let Some(entry) = volumes.get_mut(id) else {
            return StaleRootOutcome::Unchanged;
        };
        let Some(marked) = entry.roots.iter_mut().find(|r| r.path == root) else {
            return StaleRootOutcome::Unchanged;
        };
        marked.proven_stale = true;

        match entry.promote_to_best_root() {
            roots::Promotion::Promoted(new_root) => StaleRootOutcome::Promoted { new_root },
            _ => StaleRootOutcome::Unchanged,
        }
    }

    /// Find the registered non-root volume whose mount root is the longest
    /// ancestor (or equal) of `path`, returning its registry id.
    ///
    /// Used by index read routing to map a `/Volumes/X/…` path to the per-mount
    /// index it belongs to. `root` (`/`) is skipped: it prefixes every path and is
    /// the fallback the router uses when nothing more specific matches. Component-
    /// wise `starts_with` avoids a `/Volumes/XY`-matches-`/Volumes/X` false hit, and
    /// the longest-root wins so a nested mount (`/Volumes/X/Y`) beats its parent.
    ///
    /// In-memory (one `RwLock<HashMap>` read, no syscall), so it's safe on the
    /// enrichment / dir-stats hot path.
    pub fn mount_id_for_path(&self, path: &str) -> Option<String> {
        let target = Path::new(path);
        self.volumes
            .read()
            .ok()?
            .iter()
            .map(|(id, entry)| (id, &entry.volume))
            .filter(|(_, v)| v.root() != Path::new("/"))
            .filter(|(_, v)| target.starts_with(v.root()))
            .max_by_key(|(_, v)| v.root().as_os_str().len())
            .map(|(id, _)| id.clone())
    }

    /// Gets the default volume.
    pub fn default_volume(&self) -> Option<Arc<dyn Volume>> {
        let default_id = self.default_volume_id.read().ok()?.clone()?;
        self.get(&default_id)
    }

    /// Gets the default volume ID.
    pub fn default_volume_id(&self) -> Option<String> {
        self.default_volume_id.read().ok()?.clone()
    }

    /// Sets the default volume by ID.
    ///
    /// Returns true if the volume exists and was set as default.
    pub fn set_default(&self, id: &str) -> bool {
        // Verify the volume exists
        let exists = self.volumes.read().map(|v| v.contains_key(id)).unwrap_or(false);

        if exists && let Ok(mut default) = self.default_volume_id.write() {
            *default = Some(id.to_string());
            return true;
        }
        false
    }

    /// Lists all registered volumes as (id, name) pairs.
    pub fn list_volumes(&self) -> Vec<(String, String)> {
        self.volumes
            .read()
            .map(|volumes| {
                volumes
                    .iter()
                    .map(|(id, entry)| (id.clone(), entry.volume.name().to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns all registered volumes as (id, handle) pairs. Unlike [`list_volumes`]
    /// (which returns display names), this hands back the `Volume` handles so callers
    /// can inspect capabilities (`root`, `supports_local_fs_access`,
    /// `smb_connection_state`). Used by the file viewer's locality check.
    ///
    /// [`list_volumes`]: Self::list_volumes
    pub fn list_volumes_with_handles(&self) -> Vec<(String, Arc<dyn Volume>)> {
        self.volumes
            .read()
            .map(|volumes| {
                volumes
                    .iter()
                    .map(|(id, entry)| (id.clone(), entry.volume.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns the number of registered volumes.
    pub fn count(&self) -> usize {
        self.volumes.read().map(|v| v.len()).unwrap_or(0)
    }
}

impl Default for VolumeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// The process-wide volume registry, created on first access.
static VOLUME_MANAGER: LazyLock<VolumeManager> = LazyLock::new(VolumeManager::new);

/// Returns a reference to the global volume manager.
///
/// It lives beside the type, not in the `file_system` facade, so that reaching
/// the registry never means importing a module that knows every backend. Which
/// volumes get registered at startup is the facade's job (`init_volume_manager`).
pub(crate) fn get_volume_manager() -> &'static VolumeManager {
    &VOLUME_MANAGER
}

#[cfg(test)]
mod tests {
    use super::super::InMemoryVolume;
    use super::*;

    #[test]
    fn test_new_creates_empty_manager() {
        let manager = VolumeManager::new();
        assert_eq!(manager.count(), 0);
        assert!(manager.default_volume().is_none());
    }

    #[test]
    fn test_register_and_get() {
        let manager = VolumeManager::new();
        let volume = Arc::new(InMemoryVolume::new("Test Volume"));

        manager.register("test", volume.clone());

        let retrieved = manager.get("test").unwrap();
        assert_eq!(retrieved.name(), "Test Volume");
    }

    #[test]
    fn test_unregister() {
        let manager = VolumeManager::new();
        let volume = Arc::new(InMemoryVolume::new("Test Volume"));

        manager.register("test", volume);
        assert_eq!(manager.count(), 1);

        manager.unregister("test");
        assert_eq!(manager.count(), 0);
        assert!(manager.get("test").is_none());
    }

    #[test]
    fn test_set_default() {
        let manager = VolumeManager::new();
        let volume = Arc::new(InMemoryVolume::new("Test Volume"));

        manager.register("test", volume);
        assert!(manager.set_default("test"));

        let default = manager.default_volume().unwrap();
        assert_eq!(default.name(), "Test Volume");
    }

    #[test]
    fn test_set_default_nonexistent_returns_false() {
        let manager = VolumeManager::new();
        assert!(!manager.set_default("nonexistent"));
    }

    #[test]
    fn test_unregister_clears_default() {
        let manager = VolumeManager::new();
        let volume = Arc::new(InMemoryVolume::new("Test Volume"));

        manager.register("test", volume);
        manager.set_default("test");
        assert!(manager.default_volume().is_some());

        manager.unregister("test");
        assert!(manager.default_volume().is_none());
    }

    #[test]
    fn mount_id_for_path_returns_longest_non_root_ancestor() {
        use crate::file_system::LocalPosixVolume;

        let manager = VolumeManager::new();
        manager.register("root", Arc::new(LocalPosixVolume::new("Root", "/")));
        manager.register("ext", Arc::new(LocalPosixVolume::new("Ext", "/Volumes/X")));
        manager.register("nested", Arc::new(LocalPosixVolume::new("Nested", "/Volumes/X/Y")));

        // A path under the external mount routes to it, never to `root`.
        assert_eq!(manager.mount_id_for_path("/Volumes/X/sub").as_deref(), Some("ext"));
        // A nested mount wins over its parent (longest ancestor).
        assert_eq!(
            manager.mount_id_for_path("/Volumes/X/Y/deep").as_deref(),
            Some("nested")
        );
        // The mount root itself matches.
        assert_eq!(manager.mount_id_for_path("/Volumes/X").as_deref(), Some("ext"));
        // A component-boundary sibling is NOT a false prefix hit.
        assert_eq!(manager.mount_id_for_path("/Volumes/XY/z"), None);
        // A boot-disk path matches only `root` (skipped) → None.
        assert_eq!(manager.mount_id_for_path("/Users/me"), None);
    }

    #[test]
    fn test_find_by_root_returns_registered_entry() {
        let manager = VolumeManager::new();
        let volume = Arc::new(InMemoryVolume::new("Test Volume"));
        manager.register("test-id", volume);

        let (id, v) = manager.find_by_root(Path::new("/")).expect("InMemoryVolume root is /");
        assert_eq!(id, "test-id");
        assert_eq!(v.name(), "Test Volume");
    }

    #[test]
    fn test_find_by_root_returns_none_for_unknown_root() {
        let manager = VolumeManager::new();
        manager.register("test-id", Arc::new(InMemoryVolume::new("Test")));
        assert!(manager.find_by_root(Path::new("/nonexistent/path")).is_none());
    }

    #[test]
    fn test_list_volumes() {
        let manager = VolumeManager::new();
        manager.register("vol1", Arc::new(InMemoryVolume::new("Volume One")));
        manager.register("vol2", Arc::new(InMemoryVolume::new("Volume Two")));

        let list = manager.list_volumes();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|(id, name)| id == "vol1" && name == "Volume One"));
        assert!(list.iter().any(|(id, name)| id == "vol2" && name == "Volume Two"));
    }

    #[test]
    fn test_register_if_absent_new_volume() {
        let manager = VolumeManager::new();
        let volume = Arc::new(InMemoryVolume::new("Test Volume"));

        assert!(manager.register_if_absent("test", volume));
        assert_eq!(manager.count(), 1);
        assert_eq!(manager.get("test").unwrap().name(), "Test Volume");
    }

    #[test]
    fn test_register_if_absent_existing_volume_keeps_original() {
        let manager = VolumeManager::new();
        let original = Arc::new(InMemoryVolume::new("Original"));
        let replacement = Arc::new(InMemoryVolume::new("Replacement"));

        manager.register("test", original);
        assert!(!manager.register_if_absent("test", replacement));

        // Original should be kept
        assert_eq!(manager.get("test").unwrap().name(), "Original");
    }

    #[test]
    fn re_registering_the_same_mount_is_not_an_identity_conflict() {
        // The SMB upgrade swaps an OS-mounted `LocalPosixVolume` for a direct
        // `SmbVolume` at the same root. Routine, and it must stay quiet.
        let existing: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("OS mount").with_root("/Volumes/naspi"));
        let incoming: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Direct SMB").with_root("/Volumes/naspi"));
        assert!(!is_identity_conflict(&existing, &incoming));
    }

    #[test]
    fn one_id_over_two_roots_is_an_identity_conflict() {
        // Two different disks under one ID would cross-wire their index, saved
        // paths, and operation routing. IDs are built so this can't happen from a
        // derivation; if it ever does (a cloned volume, a double mount), it has to
        // be loud rather than silent.
        let existing: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Disk A").with_root("/Volumes/A"));
        let incoming: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Disk B").with_root("/Volumes/B"));
        assert!(is_identity_conflict(&existing, &incoming));
    }

    #[test]
    fn an_identity_conflict_keeps_the_incumbent_whatever_the_order() {
        // One share mounted at two paths registers twice under one ID. Letting
        // the last writer win made registration ORDER decide where the volume
        // was rooted, so a saved path under the first mount reached a backend
        // rooted at the second and every listing under it failed.
        for (first_root, second_root) in [
            ("/Volumes/naspi", "/Volumes/naspi-1"),
            ("/Volumes/naspi-1", "/Volumes/naspi"),
        ] {
            let manager = VolumeManager::new();
            manager.register(
                "smb-share",
                Arc::new(InMemoryVolume::new("First").with_root(first_root)),
            );
            manager.register(
                "smb-share",
                Arc::new(InMemoryVolume::new("Second").with_root(second_root)),
            );

            let registered = manager.get("smb-share").expect("registered above");
            assert_eq!(registered.name(), "First", "the incumbent keeps the ID");
            assert_eq!(registered.root(), Path::new(first_root));
        }
    }

    #[test]
    fn a_second_mount_of_one_share_is_recorded_as_a_sibling_root() {
        // macOS mounts the same share twice and both mounts derive one ID. The
        // incumbent stays active (that's what keeps a saved `/Volumes/naspi/…`
        // path working), but the second root has to stay FINDABLE: the unmount
        // path looks a gone mount up by root, and a sibling it can't see is a
        // sibling it can't promote to.
        let manager = VolumeManager::new();
        manager.register(
            "smb-share",
            Arc::new(InMemoryVolume::new("First").with_root("/Volumes/naspi")),
        );
        manager.register(
            "smb-share",
            Arc::new(InMemoryVolume::new("Second").with_root("/Volumes/naspi-1")),
        );

        let active = manager.get("smb-share").expect("registered above");
        assert_eq!(active.root(), Path::new("/Volumes/naspi"), "the incumbent stays active");

        let (id, _) = manager
            .find_by_root(Path::new("/Volumes/naspi-1"))
            .expect("the sibling root is a known root of this volume");
        assert_eq!(id, "smb-share");
    }

    #[test]
    fn replacing_the_volume_at_the_same_root_still_wins() {
        // The SMB upgrade swaps an OS-mounted `LocalPosixVolume` for a direct
        // `SmbVolume` at the same root, and the manual "Connect directly" and
        // reconnect paths do the same. Not an identity conflict, so it replaces.
        let manager = VolumeManager::new();
        manager.register(
            "smb-share",
            Arc::new(InMemoryVolume::new("OS mount").with_root("/Volumes/naspi")),
        );
        manager.register(
            "smb-share",
            Arc::new(InMemoryVolume::new("Direct SMB").with_root("/Volumes/naspi")),
        );

        assert_eq!(manager.get("smb-share").expect("registered above").name(), "Direct SMB");
    }

    /// A registry holding one share reached through two mount points, the
    /// shortest one active. The shape every multi-root test starts from.
    fn doubly_mounted_share() -> VolumeManager {
        use crate::file_system::LocalPosixVolume;

        let manager = VolumeManager::new();
        manager.register("share", Arc::new(LocalPosixVolume::new("naspi", "/Volumes/naspi")));
        manager.register("share", Arc::new(LocalPosixVolume::new("naspi", "/Volumes/naspi-1")));
        manager
    }

    #[test]
    fn losing_the_active_mount_promotes_a_sibling_instead_of_unregistering() {
        // Ejecting one of two mounts of a share used to take the whole share
        // away until the app restarted (discovery runs at launch only), because
        // the unmount path unregistered the ID it found by root.
        let manager = doubly_mounted_share();

        let outcome = manager.remove_root(Path::new("/Volumes/naspi"));
        assert!(matches!(outcome, RootRemoval::Promoted { .. }), "a sibling survives");
        assert_eq!(
            manager.get("share").expect("still registered").root(),
            Path::new("/Volumes/naspi-1")
        );
    }

    #[test]
    fn losing_the_last_mount_unregisters_the_volume() {
        let manager = doubly_mounted_share();
        manager.remove_root(Path::new("/Volumes/naspi"));

        let outcome = manager.remove_root(Path::new("/Volumes/naspi-1"));
        assert!(
            matches!(outcome, RootRemoval::Unregistered { .. }),
            "the last root gone means gone"
        );
        assert!(manager.get("share").is_none());
    }

    #[test]
    fn losing_a_fallback_mount_leaves_the_active_one_alone() {
        let manager = doubly_mounted_share();

        let outcome = manager.remove_root(Path::new("/Volumes/naspi-1"));
        assert!(matches!(outcome, RootRemoval::SiblingDropped { .. }));
        assert_eq!(
            manager.get("share").expect("still registered").root(),
            Path::new("/Volumes/naspi")
        );
        assert_eq!(manager.known_roots("share"), vec![PathBuf::from("/Volumes/naspi")]);
    }

    #[test]
    fn a_backend_that_cant_reroot_keeps_its_registration() {
        // `InMemoryVolume` takes the conservative `rerooted` default, standing in
        // for a backend whose transport is anchored to its root. Losing the
        // active mount must not unregister it while another mount reaches the
        // same filesystem: a direct `SmbVolume` rides smb2, not the mount, so
        // dropping it would kill a session that still works.
        let manager = VolumeManager::new();
        manager.register(
            "share",
            Arc::new(InMemoryVolume::new("Direct SMB").with_root("/Volumes/naspi")),
        );
        manager.register(
            "share",
            Arc::new(InMemoryVolume::new("Second mount").with_root("/Volumes/naspi-1")),
        );

        let outcome = manager.remove_root(Path::new("/Volumes/naspi"));
        assert!(matches!(outcome, RootRemoval::ActiveRootStranded { .. }));
        assert_eq!(
            manager.get("share").expect("still registered").root(),
            Path::new("/Volumes/naspi"),
            "it stays where its transport is anchored"
        );
    }

    #[test]
    fn a_stale_mount_errno_hands_the_id_to_a_live_sibling() {
        // Liveness outranks path shape. macOS leaves a wedged `/Volumes/naspi`
        // in place and lands the reconnect at `/Volumes/naspi-1`; picking the
        // shortest path then picks the corpse, on every launch, forever.
        let manager = doubly_mounted_share();
        assert_eq!(
            manager.get("share").expect("registered").root(),
            Path::new("/Volumes/naspi"),
            "while both look alive, the shortest path is active"
        );

        let outcome = manager.mark_root_stale("share", Path::new("/Volumes/naspi"));
        assert!(matches!(outcome, StaleRootOutcome::Promoted { .. }));
        assert_eq!(
            manager.get("share").expect("still registered").root(),
            Path::new("/Volumes/naspi-1")
        );
    }

    #[test]
    fn a_failed_operation_promotes_only_on_an_errno_that_proves_the_mount_is_gone() {
        use crate::file_system::LocalPosixVolume;
        use crate::file_system::volume::{VolumeError, note_root_failure};

        // Drives the global registry (that's what `note_root_failure` reads), so
        // it uses ids no other test touches.
        let id = "cmdr-test-stale-errno-share";
        let manager = get_volume_manager();
        manager.unregister(id);
        manager.register(id, Arc::new(LocalPosixVolume::new("naspi", "/Volumes/cmdr-test-stale")));
        manager.register(
            id,
            Arc::new(LocalPosixVolume::new("naspi", "/Volumes/cmdr-test-stale-1")),
        );

        // A missing file says nothing about the mount.
        note_root_failure(
            id,
            &VolumeError::IoError {
                message: "no such file".to_string(),
                raw_os_error: Some(libc::ENOENT),
            },
        );
        assert_eq!(
            manager.get(id).expect("registered").root(),
            Path::new("/Volumes/cmdr-test-stale"),
            "an ordinary file error must not rotate a healthy volume's root"
        );

        note_root_failure(
            id,
            &VolumeError::IoError {
                message: "socket is not connected".to_string(),
                raw_os_error: Some(libc::ENOTCONN),
            },
        );
        assert_eq!(
            manager.get(id).expect("still registered").root(),
            Path::new("/Volumes/cmdr-test-stale-1"),
            "a stale-mount errno moves the volume to the mount that still answers"
        );

        manager.unregister(id);
    }

    #[test]
    fn test_multiple_volumes() {
        let manager = VolumeManager::new();

        manager.register("root", Arc::new(InMemoryVolume::new("Macintosh HD")));
        manager.register("dropbox", Arc::new(InMemoryVolume::new("Dropbox")));
        manager.register("gdrive", Arc::new(InMemoryVolume::new("Google Drive")));

        assert_eq!(manager.count(), 3);

        manager.set_default("root");
        assert_eq!(manager.default_volume().unwrap().name(), "Macintosh HD");

        // Switch default
        manager.set_default("dropbox");
        assert_eq!(manager.default_volume().unwrap().name(), "Dropbox");
    }

    #[test]
    fn test_concurrent_registration() {
        use std::thread;

        let manager = Arc::new(VolumeManager::new());
        let mut handles = vec![];

        // Spawn 10 threads that each register a volume
        for i in 0..10 {
            let manager_clone = Arc::clone(&manager);
            handles.push(thread::spawn(move || {
                let volume = Arc::new(InMemoryVolume::new(format!("Volume {}", i)));
                manager_clone.register(&format!("vol_{}", i), volume);
            }));
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // All 10 should be registered
        assert_eq!(manager.count(), 10);
    }

    #[test]
    fn test_concurrent_reads() {
        use std::thread;

        let manager = Arc::new(VolumeManager::new());

        // Pre-register volumes
        for i in 0..5 {
            manager.register(
                &format!("vol_{}", i),
                Arc::new(InMemoryVolume::new(format!("Volume {}", i))),
            );
        }
        manager.set_default("vol_0");

        let mut handles = vec![];

        // Spawn 20 threads that concurrently read
        for _ in 0..20 {
            let manager_clone = Arc::clone(&manager);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let _ = manager_clone.get("vol_0");
                    let _ = manager_clone.default_volume();
                    let _ = manager_clone.list_volumes();
                    let _ = manager_clone.count();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should still have correct state
        assert_eq!(manager.count(), 5);
        assert_eq!(manager.default_volume().unwrap().name(), "Volume 0");
    }

    #[test]
    fn test_concurrent_read_write() {
        use std::thread;

        let manager = Arc::new(VolumeManager::new());
        manager.register("permanent", Arc::new(InMemoryVolume::new("Permanent")));

        let mut handles = vec![];

        // Readers
        for _ in 0..5 {
            let manager_clone = Arc::clone(&manager);
            handles.push(thread::spawn(move || {
                for _ in 0..50 {
                    let _ = manager_clone.get("permanent");
                    let _ = manager_clone.list_volumes();
                    thread::yield_now();
                }
            }));
        }

        // Writers
        for i in 0..5 {
            let manager_clone = Arc::clone(&manager);
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    let vol_id = format!("temp_{}_{}", i, j);
                    manager_clone.register(&vol_id, Arc::new(InMemoryVolume::new(&vol_id)));
                    thread::yield_now();
                    manager_clone.unregister(&vol_id);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Permanent volume should still exist
        assert!(manager.get("permanent").is_some());
    }
}
