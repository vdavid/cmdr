//! Volume manager for registering and accessing volumes.
//!
//! The VolumeManager is the central registry for all mounted volumes.
//! It tracks both the available volumes and which one is the current default.
//! The one instance the app runs on lives here too, behind [`get_volume_manager`].

use super::Volume;
use crate::ignore_poison::RwLockIgnorePoison;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, RwLock};

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
    /// Who wants to hear that a volume has become available. See
    /// [`Self::on_volume_arrival`].
    arrival_listeners: RwLock<Vec<VolumeArrivalListener>>,
}

/// Notified with the ID of a volume the registry has just taken on.
///
/// Deliberately only the ID: a listener that needs the handle asks the registry
/// for it, so it can never act on a volume a racing registration has already
/// replaced. It also keeps the dependency pointing one way — the registry knows
/// nothing about who listens, which is what lets the in-flight temp ledger
/// (`write_operations::in_flight_temps`) subscribe without welding the two
/// subtrees into a cycle.
type VolumeArrivalListener = Box<dyn Fn(&str) + Send + Sync>;

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
///
/// **Warn, not error, and deliberately so.** Error level is the auto-report
/// threshold (`error_reporter/CLAUDE.md`), and the common case here is a share
/// macOS mounted twice: recoverable, expected (the message says so), and not
/// user-impacting, which is exactly the rule's definition of a warning. At error
/// level one machine's remount loop uploaded 98 bundles in a week and drowned the
/// channel it was supposed to alert. The rarer clone deserves louder, but nothing
/// here can tell the two apart, so this stays a warning that names both.
fn report_identity_conflict(id: &str, existing: &Arc<dyn Volume>, incoming: &Arc<dyn Volume>, resolution: &str) {
    if !is_identity_conflict(existing.root(), incoming.root()) {
        return;
    }
    log::warn!(
        target: "cmdr_lib::file_system::volume",
        "Two different mount roots ({} and {}) claim volume ID {id}; {resolution}, so the two share per-volume state. Expected only for a cloned volume or a doubly-mounted filesystem.",
        existing.root().display(),
        incoming.root().display(),
    );
}

/// Whether re-registering `id` means two DIFFERENT mounts, rather than the same
/// mount changing backends (which is routine: that's the SMB upgrade).
fn is_identity_conflict(existing_root: &Path, incoming_root: &Path) -> bool {
    existing_root != incoming_root
}

impl VolumeManager {
    /// Creates a new empty volume manager.
    pub fn new() -> Self {
        Self {
            volumes: RwLock::new(HashMap::new()),
            default_volume_id: RwLock::new(None),
            archive_lru: Mutex::new(VecDeque::new()),
            arrival_listeners: RwLock::new(Vec::new()),
        }
    }

    /// Asks to be told, by ID, whenever a volume becomes available here.
    ///
    /// For work that has to wait for a volume rather than go looking for one:
    /// the in-flight temp ledger holds the partials an interrupted transfer left
    /// on a share and can only remove them once that share is back. A listener
    /// runs INSIDE the registration, so it must return immediately — hand real
    /// work to a task.
    pub fn on_volume_arrival(&self, listener: impl Fn(&str) + Send + Sync + 'static) {
        self.arrival_listeners.write_ignore_poison().push(Box::new(listener));
    }

    /// Tells the listeners `id` is now served.
    ///
    /// ❌ Never call this while holding the `volumes` lock: a listener may ask
    /// the registry for the volume it was just told about.
    fn announce_arrival(&self, id: &str) {
        for listener in self.arrival_listeners.read_ignore_poison().iter() {
            listener(id);
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
        self.register_locked(id, volume);
        // Outside the `volumes` guard, and unconditional: every arm above leaves
        // a usable volume serving `id`, whether it's the newcomer or a kept
        // incumbent.
        self.announce_arrival(id);
    }

    /// [`register`](Self::register)'s body, so the announcement happens after the
    /// `volumes` guard is gone.
    fn register_locked(&self, id: &str, volume: Arc<dyn Volume>) {
        let mut volumes = self.volumes.write_ignore_poison();
        if let Some(existing) = volumes.get_mut(id) {
            report_identity_conflict(id, &existing.volume, &volume, ROOT_RECORDED_RESOLUTION);
            if is_identity_conflict(existing.volume.root(), volume.root()) {
                existing.record_root(volume.root());
                return;
            }
            existing.replace_volume(volume);
            return;
        }
        volumes.insert(id.to_string(), Registration::new(volume));
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
        self.volumes
            .write_ignore_poison()
            .insert(id.to_string(), Registration::new(volume));
        self.announce_arrival(id);
    }

    /// Whether [`register`] would REFUSE a volume rooted at `root` under `id`,
    /// keeping the incumbent active and recording `root` as a fallback.
    ///
    /// A caller that retires the incumbent before handing over (the SMB upgrade's
    /// `register_replacing_predecessor`) has to ask FIRST. Retiring stops the
    /// volume's watcher, and on a refused registration the registry keeps that
    /// same volume serving: the ID ends up pointing at a live share that no longer
    /// sees its own changes.
    ///
    /// [`register`]: Self::register
    pub fn would_keep_incumbent(&self, id: &str, root: &Path) -> bool {
        self.volumes
            .read_ignore_poison()
            .get(id)
            .is_some_and(|entry| is_identity_conflict(entry.volume.root(), root))
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
        use std::collections::hash_map::Entry;
        let registered = match self.volumes.write_ignore_poison().entry(id.to_string()) {
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
        };
        if registered {
            self.announce_arrival(id);
        }
        registered
    }

    /// Unregisters a volume by ID, dropping every mount root it owned.
    ///
    /// If this was the default volume, the default is cleared.
    pub fn unregister(&self, id: &str) {
        let removed = self.volumes.write_ignore_poison().remove(id);
        if let Some(entry) = removed {
            retire(&entry.volume);
        }
        self.clear_default_if(id);
    }

    /// Clears the default volume when it was `id`. Touches only
    /// `default_volume_id`, so it's safe to call while holding `volumes`.
    fn clear_default_if(&self, id: &str) {
        // The read guard is a temporary of this statement, so it's released
        // before the write below asks for the same lock.
        let is_default = self.default_volume_id.read_ignore_poison().as_deref() == Some(id);
        if is_default {
            *self.default_volume_id.write_ignore_poison() = None;
        }
    }

    /// Gets a volume by ID.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Volume>> {
        Some(self.volumes.read_ignore_poison().get(id)?.volume.clone())
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
            .read_ignore_poison()
            .iter()
            .find(|(_, entry)| entry.knows_root(root))
            .map(|(id, entry)| (id.clone(), Arc::clone(&entry.volume)))
    }

    /// Gets the default volume.
    pub fn default_volume(&self) -> Option<Arc<dyn Volume>> {
        let default_id = self.default_volume_id.read_ignore_poison().clone()?;
        self.get(&default_id)
    }

    /// Gets the default volume ID.
    pub fn default_volume_id(&self) -> Option<String> {
        self.default_volume_id.read_ignore_poison().clone()
    }

    /// Sets the default volume by ID.
    ///
    /// Returns true if the volume exists and was set as default.
    pub fn set_default(&self, id: &str) -> bool {
        // Verify the volume exists
        if !self.volumes.read_ignore_poison().contains_key(id) {
            return false;
        }
        *self.default_volume_id.write_ignore_poison() = Some(id.to_string());
        true
    }

    /// Lists all registered volumes as (id, name) pairs.
    pub fn list_volumes(&self) -> Vec<(String, String)> {
        self.volumes
            .read_ignore_poison()
            .iter()
            .map(|(id, entry)| (id.clone(), entry.volume.name().to_string()))
            .collect()
    }

    /// Returns all registered volumes as (id, handle) pairs. Unlike [`list_volumes`]
    /// (which returns display names), this hands back the `Volume` handles so callers
    /// can inspect capabilities (`root`, `supports_local_fs_access`,
    /// `smb_connection_state`). Used by the file viewer's locality check.
    ///
    /// [`list_volumes`]: Self::list_volumes
    pub fn list_volumes_with_handles(&self) -> Vec<(String, Arc<dyn Volume>)> {
        self.volumes
            .read_ignore_poison()
            .iter()
            .map(|(id, entry)| (id.clone(), entry.volume.clone()))
            .collect()
    }

    /// Returns the number of registered volumes.
    pub fn count(&self) -> usize {
        self.volumes.read_ignore_poison().len()
    }
}

/// Tell a volume the registry no longer serves it, so whatever it runs in the
/// background (a watcher, a reconnect backoff loop) stands down.
///
/// Called from the two ways out of the registry, [`VolumeManager::unregister`]
/// and `roots::remove_root`'s last-mount arm, and from nowhere else. ❌ Not on a
/// REPLACE: a re-root hands the ID to another instance of a share that is still
/// live and still watching, and retiring there stops a healthy volume. An
/// upgrade's predecessor is retired by the hand-over itself, through
/// `Volume::on_superseded`.
///
/// A backend with nothing running between calls keeps no flag and this is a
/// no-op. See `cmdr_fs::volume::Retirement`.
pub(super) fn retire(volume: &Arc<dyn Volume>) {
    if let Some(retirement) = volume.retirement() {
        retirement.retire();
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
    use test_support::RetiringVolume;

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
        assert!(!is_identity_conflict(existing.root(), incoming.root()));
    }

    #[test]
    fn one_id_over_two_roots_is_an_identity_conflict() {
        // Two different disks under one ID would cross-wire their index, saved
        // paths, and operation routing. IDs are built so this can't happen from a
        // derivation; if it ever does (a cloned volume, a double mount), it has to
        // be loud rather than silent.
        let existing: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Disk A").with_root("/Volumes/A"));
        let incoming: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Disk B").with_root("/Volumes/B"));
        assert!(is_identity_conflict(existing.root(), incoming.root()));
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

    // ── Retirement ──────────────────────────────────────────────
    //
    // A volume's background work (a watcher, a reconnect backoff loop) outlives
    // any one call, so it has to keep asking whether the registry still serves
    // the volume it was spawned for. Only the registry knows, so the registry
    // writes the answer down. See `cmdr_fs::volume::Retirement`.

    #[test]
    fn unregistering_a_volume_retires_it() {
        let manager = VolumeManager::new();
        let (volume, is_retired) = RetiringVolume::at("/Volumes/naspi");
        manager.register("naspi", volume);
        assert!(!is_retired(), "a registered volume is live");

        manager.unregister("naspi");

        assert!(
            is_retired(),
            "nothing routes to this volume any more, so its background work must stand down"
        );
    }

    /// Replacing a volume is NOT removing it, and the difference decides whether
    /// a healthy share keeps its watcher. The registry replaces an entry for two
    /// reasons: an upgrade (a fresh volume over a fresh session, whose
    /// predecessor is retired by the caller through `Volume::on_superseded`), and
    /// a re-root onto a surviving mount (the SAME share, deliberately still
    /// live). Retiring on every replace would stand the re-rooted share down.
    #[test]
    fn replacing_a_volume_at_its_own_root_retires_nobody() {
        let manager = VolumeManager::new();
        let (incumbent, incumbent_retired) = RetiringVolume::at("/Volumes/naspi");
        let (successor, successor_retired) = RetiringVolume::at("/Volumes/naspi");
        manager.register("naspi", incumbent);

        manager.register("naspi", successor);

        assert!(
            !incumbent_retired() && !successor_retired(),
            "who retires on a hand-over is the hand-over's business, not the registry's"
        );
    }

    /// A backend with nothing running between calls answers `None`, and the
    /// registry has to cope rather than assume every volume keeps a flag.
    #[test]
    fn a_volume_that_keeps_no_flag_unregisters_fine() {
        let manager = VolumeManager::new();
        manager.register("test", Arc::new(InMemoryVolume::new("Test Volume")));

        manager.unregister("test");

        assert_eq!(manager.count(), 0);
    }
}
