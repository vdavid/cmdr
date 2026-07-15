//! Volume manager for registering and accessing volumes.
//!
//! The VolumeManager is the central registry for all mounted volumes.
//! It tracks both the available volumes and which one is the current default.

use super::Volume;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

/// Archive routing (`resolve`, `.zip`-boundary predicates, the archive LRU, and
/// [`ResolvedVolume`]) lives in a second `impl VolumeManager` block here.
mod archive_routing;

/// Manages registered volumes and provides access to them.
///
/// Thread-safe registry storing volumes by ID, with support for a default volume.
pub struct VolumeManager {
    volumes: RwLock<HashMap<String, Arc<dyn Volume>>>,
    default_volume_id: RwLock<Option<String>>,
    /// Registration recency of the on-demand `ArchiveVolume`s (front = oldest).
    /// A value store: recovering on poison is safe (a lost reorder at worst
    /// evicts slightly early). See [`Self::touch_archive_lru`].
    archive_lru: Mutex<VecDeque<String>>,
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
    /// If a volume with this ID already exists, it will be replaced.
    pub fn register(&self, id: &str, volume: Arc<dyn Volume>) {
        if let Ok(mut volumes) = self.volumes.write() {
            volumes.insert(id.to_string(), volume);
        }
    }

    /// Registers a volume only if no volume with this ID exists yet.
    ///
    /// Returns `true` if the volume was registered, `false` if a volume
    /// with this ID already exists (the existing volume is kept).
    pub fn register_if_absent(&self, id: &str, volume: Arc<dyn Volume>) -> bool {
        if let Ok(mut volumes) = self.volumes.write() {
            use std::collections::hash_map::Entry;
            match volumes.entry(id.to_string()) {
                Entry::Occupied(_) => false,
                Entry::Vacant(e) => {
                    e.insert(volume);
                    true
                }
            }
        } else {
            false
        }
    }

    /// Unregisters a volume by ID.
    ///
    /// If this was the default volume, the default is cleared.
    pub fn unregister(&self, id: &str) {
        if let Ok(mut volumes) = self.volumes.write() {
            volumes.remove(id);
        }
        // Clear default if it was this volume
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
        self.volumes.read().ok()?.get(id).cloned()
    }

    /// Finds a registered volume by its mount path (the value `Volume::root()` returns).
    ///
    /// Used by the unmount path: when `NSWorkspaceDidUnmount` (macOS) or the
    /// `/proc/mounts` watcher (Linux) fires, `statfs` on the now-gone path can no
    /// longer recover the SMB mount info, so we can't rederive the volume ID from
    /// the path. Looking up by `root()` instead lets us find the entry we
    /// registered, whatever ID it was keyed under.
    pub fn find_by_root(&self, root: &Path) -> Option<(String, Arc<dyn Volume>)> {
        self.volumes
            .read()
            .ok()?
            .iter()
            .find(|(_, v)| v.root() == root)
            .map(|(id, v)| (id.clone(), Arc::clone(v)))
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
                    .map(|(id, vol)| (id.clone(), vol.name().to_string()))
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
            .map(|volumes| volumes.iter().map(|(id, vol)| (id.clone(), vol.clone())).collect())
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
