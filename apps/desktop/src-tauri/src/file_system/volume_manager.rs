//! Volume manager for registering and accessing volumes.
//!
//! The VolumeManager is the central registry for all mounted volumes.
//! It tracks both the available volumes and which one is the current default.

// TODO: Remove this once VolumeManager is used in lib.rs (Phase 4)
#![allow(dead_code)]

use super::volume::Volume;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Manages registered volumes and provides access to them.
///
/// Thread-safe registry storing volumes by ID, with support for a default volume.
pub struct VolumeManager {
    volumes: RwLock<HashMap<String, Arc<dyn Volume>>>,
    default_volume_id: RwLock<Option<String>>,
}

impl VolumeManager {
    /// Creates a new empty volume manager.
    pub fn new() -> Self {
        Self {
            volumes: RwLock::new(HashMap::new()),
            default_volume_id: RwLock::new(None),
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
    use super::*;
    use crate::file_system::volume::InMemoryVolume;

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
