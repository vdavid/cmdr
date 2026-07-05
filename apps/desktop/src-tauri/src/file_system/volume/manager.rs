//! Volume manager for registering and accessing volumes.
//!
//! The VolumeManager is the central registry for all mounted volumes.
//! It tracks both the available volumes and which one is the current default.

use super::Volume;
use super::backends::archive::{
    ArchiveVolume, archive_boundary_candidate, bytes_start_with_zip_signature, confirm_archive_boundary,
};
use crate::ignore_poison::IgnorePoison;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

/// Max number of `ArchiveVolume`s kept registered at once. Browsing many zips
/// must not leak volumes + parents + index caches, so the archive LRU evicts the
/// least-recently-resolved archive past this cap. Eviction is harmless: the next
/// navigation re-resolves and re-registers lazily (`ArchiveVolume::new` is cheap;
/// the index re-parses on demand).
const ARCHIVE_LRU_CAP: usize = 16;

/// Outcome of [`VolumeManager::resolve`]: the volume that should serve `path`.
///
/// `path` is ALWAYS the caller's input path unchanged. An archive resolve only
/// swaps in the `ArchiveVolume` — which maps the full `/…/foo.zip/inner` path to
/// its own inner namespace via `inner_path()` — and a passthrough returns the
/// requested volume untouched. Adoption sites read `resolved.path` so the "full
/// path, unchanged" contract lives in exactly one place.
pub struct ResolvedVolume {
    /// The volume to call, or `None` when `volume_id` isn't registered (an
    /// unmount race). Sites keep their existing `.ok_or_else(...)?` handling.
    pub volume: Option<Arc<dyn Volume>>,
    /// The path to pass to `volume`'s methods — the input path, verbatim.
    pub path: PathBuf,
    /// `true` when `path` crossed into an archive and `volume` is its
    /// `ArchiveVolume`. Sites use it to skip drive-index enrich/verify and the
    /// read-only write guards.
    pub is_archive: bool,
}

impl ResolvedVolume {
    /// A non-archive resolve: the requested volume (if any), path unchanged.
    fn passthrough(volume: Option<Arc<dyn Volume>>, path: &Path) -> Self {
        Self {
            volume,
            path: path.to_path_buf(),
            is_archive: false,
        }
    }
}

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

    /// Path-aware volume lookup: routes a path that crosses a `.zip` boundary to
    /// the read-only [`ArchiveVolume`] for that archive, registering it on demand.
    ///
    /// No archive-extension component → a plain [`get`](Self::get) with the path
    /// unchanged, and zero I/O (the pure candidate check gates everything below).
    /// A confirmed boundary (a path component that's a real archive FILE, magic-
    /// byte checked) → `register_if_absent` the archive under `archive-{hash(zip)}`,
    /// bump the LRU, and return `(archive_volume, path)`. The returned path is
    /// ALWAYS the input path — `ArchiveVolume` maps it to its inner namespace
    /// itself (see [`ResolvedVolume`]).
    ///
    /// Confirmation is parent-aware. A LOCAL parent (`supports_local_fs_access`)
    /// confirms with a cheap `std::fs` stat + magic read — byte-identical to the
    /// pre-remote path. A REMOTE parent (direct SMB / MTP) can't be `std::fs`'d,
    /// so it confirms through the parent's own `get_metadata` (is-it-a-file) + a
    /// four-byte `read_range` (the zip magic). That's why this is `async`. See
    /// [`confirm_remote_archive_boundary`].
    ///
    /// Adopt this at every site that did `get(volume_id)` then `volume.method(path)`
    /// so a `.zip` path transparently routes to the archive. The sync-only
    /// [`resolve_local_only`](Self::resolve_local_only) exists for the one caller
    /// that can't `.await` (the write-op fresh-listing oracle).
    pub async fn resolve(&self, volume_id: &str, path: &Path) -> ResolvedVolume {
        // Pure string pre-filter: no archive-extension component ⇒ zero I/O on
        // the hot listing path (neither disk nor network is touched here).
        let Some((zip_path, _inner)) = archive_boundary_candidate(path) else {
            return ResolvedVolume::passthrough(self.get(volume_id), path);
        };

        // The requested volume physically holds the `.zip`, so it's the archive's
        // parent (shared lane key, space info, and the remote byte source).
        let Some(parent) = self.get(volume_id) else {
            return ResolvedVolume::passthrough(None, path);
        };

        let confirmed = if parent.supports_local_fs_access() {
            // Local: the existing std::fs stat + magic sniff, unchanged.
            confirm_archive_boundary(path).is_some()
        } else {
            // Remote: confirm through the parent volume's own I/O.
            confirm_remote_archive_boundary(parent.as_ref(), &zip_path).await
        };
        if !confirmed {
            return ResolvedVolume::passthrough(Some(parent), path);
        }

        self.register_archive(volume_id, parent, zip_path, path)
    }

    /// Sync sibling of [`resolve`](Self::resolve) that confirms **only local**
    /// archive boundaries. A remote (direct SMB / MTP) `.zip` path returns a
    /// passthrough (its parent volume, path unchanged), because a remote confirm
    /// needs async I/O this method can't do.
    ///
    /// The ONE caller is the write-op fresh-listing oracle
    /// (`listing::caching::try_get_watched_listing`), which runs on sync recursive
    /// scan walkers. That oracle guards remote archives separately (a non-local
    /// parent's volume-level `listing_is_watched` would falsely claim freshness),
    /// so the local-only routing here is sufficient there. Every other caller uses
    /// the async [`resolve`](Self::resolve) and gets full remote routing.
    pub fn resolve_local_only(&self, volume_id: &str, path: &Path) -> ResolvedVolume {
        let Some((zip_path, _inner)) = confirm_archive_boundary(path) else {
            return ResolvedVolume::passthrough(self.get(volume_id), path);
        };
        let Some(parent) = self.get(volume_id) else {
            return ResolvedVolume::passthrough(None, path);
        };
        self.register_archive(volume_id, parent, zip_path, path)
    }

    /// Async, parent-aware sibling of the sync
    /// [`archive::path_is_inside_archive`](super::backends::archive::path_is_inside_archive):
    /// true when `path` points INSIDE a confirmed archive (a non-empty inner
    /// path), for BOTH local and remote (direct SMB / MTP) parents. The `.zip`
    /// file itself is a plain file → false.
    ///
    /// The write-routing seams (delete / rename / copy-out source) use this so a
    /// remote archive-inner write reaches the managed edit driver instead of
    /// falling through to a confusing parent-volume write. The sync free function
    /// is `std::fs`-only, so it silently returns false for `smb://` / `mtp://`
    /// paths — the bug this closes. Confirmation mirrors [`resolve`](Self::resolve):
    /// a local parent uses the zero-network `std::fs` stat + magic sniff; a remote
    /// parent confirms through its own `get_metadata` + four-byte `read_range`.
    pub async fn path_is_inside_archive(&self, volume_id: &str, path: &Path) -> bool {
        self.confirm_boundary_on_parent(volume_id, path)
            .await
            .is_some_and(|(_zip, inner)| !inner.as_os_str().is_empty())
    }

    /// Async, parent-aware sibling of the sync
    /// [`archive::path_crosses_archive_boundary`](super::backends::archive::path_crosses_archive_boundary):
    /// true when `path` crosses into a confirmed archive — the `.zip` file itself
    /// (empty inner) OR a path inside it.
    ///
    /// `create` uses this (not [`path_is_inside_archive`](Self::path_is_inside_archive)):
    /// a new entry's parent can BE the `.zip` file (creating at the archive root),
    /// which must still route to the managed edit driver.
    pub async fn path_crosses_archive_boundary(&self, volume_id: &str, path: &Path) -> bool {
        self.confirm_boundary_on_parent(volume_id, path).await.is_some()
    }

    /// Shared confirm behind the two async boundary predicates above: returns the
    /// `(zip_path, inner_path)` split when `path` crosses into a CONFIRMED archive
    /// on `volume_id`'s volume, else `None` (not an archive path, or the volume
    /// isn't registered). Local parents confirm with `std::fs`; remote parents
    /// (direct SMB / MTP) through the parent's own I/O, which is why it's `async`.
    async fn confirm_boundary_on_parent(&self, volume_id: &str, path: &Path) -> Option<(PathBuf, PathBuf)> {
        let (zip_path, inner) = archive_boundary_candidate(path)?;
        let parent = self.get(volume_id)?;
        let confirmed = if parent.supports_local_fs_access() {
            confirm_archive_boundary(path).is_some()
        } else {
            confirm_remote_archive_boundary(parent.as_ref(), &zip_path).await
        };
        confirmed.then_some((zip_path, inner))
    }

    /// Registers (or reuses) the [`ArchiveVolume`] for a confirmed `.zip`, bumps
    /// the LRU, and returns it resolved. Shared by both [`resolve`](Self::resolve)
    /// and [`resolve_local_only`](Self::resolve_local_only); the confirm step (the
    /// only local-vs-remote difference) happens before this.
    fn register_archive(
        &self,
        volume_id: &str,
        parent: Arc<dyn Volume>,
        zip_path: PathBuf,
        path: &Path,
    ) -> ResolvedVolume {
        let archive_id = archive_volume_id(&zip_path);
        let archive = Arc::new(ArchiveVolume::new(parent, zip_path));
        // Only the resolve that actually registered starts the content watch, so
        // repeated resolves of an already-registered archive don't churn notify
        // watchers. The watch lives on the registered `ArchiveVolume` and stops
        // when the LRU evicts it (the volume's `Arc` drops). `volume_id` is the
        // parent drive id the listing cache keys on. Gated on the app handle: the
        // watch only refreshes frontend listings, so there's nothing to watch
        // before the frontend exists — and this keeps headless unit tests from
        // starting real OS watches (production sets the handle at startup, before
        // any archive is browsed). A non-local parent's watch never establishes
        // (notify can't watch an `smb://` / `mtp://` path), so a remote archive's
        // `listing_is_watched` stays false — pre-op rescans stay honest.
        if self.register_if_absent(&archive_id, archive.clone()) && crate::file_system::watcher::app_handle_present() {
            archive.start_content_watch(volume_id);
        }
        self.touch_archive_lru(&archive_id);

        match self.get(&archive_id) {
            Some(volume) => ResolvedVolume {
                volume: Some(volume),
                path: path.to_path_buf(),
                is_archive: true,
            },
            // Registered-then-evicted before we could read it back (only reachable
            // under a pathologically small cap). Fall back to the parent volume.
            None => ResolvedVolume::passthrough(self.get(volume_id), path),
        }
    }

    /// Records `id` as the most-recently-resolved archive and unregisters the
    /// least-recently-resolved ones past [`ARCHIVE_LRU_CAP`] (each volume's
    /// `ArchiveIndexCache` drops with it). Eviction is lazy-safe: a later
    /// `resolve` re-registers. Unregisters OUTSIDE the LRU lock so the LRU and
    /// volumes locks are never held at once.
    fn touch_archive_lru(&self, id: &str) {
        let evicted: Vec<String> = {
            let mut lru = self.archive_lru.lock_ignore_poison();
            lru.retain(|existing| existing != id);
            lru.push_back(id.to_string());
            let mut evicted = Vec::new();
            while lru.len() > ARCHIVE_LRU_CAP {
                if let Some(old) = lru.pop_front() {
                    evicted.push(old);
                }
            }
            evicted
        };
        for old in evicted {
            self.unregister(&old);
        }
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

/// Confirms a `.zip` candidate on a REMOTE parent (direct SMB / MTP) is a real,
/// browsable archive, using the parent volume's own I/O instead of `std::fs`
/// (the `.zip` isn't reachable through the local filesystem).
///
/// Mirrors the local [`confirm_archive_boundary`] check: the component must be a
/// FILE (a directory named `foo.zip` loses to normal navigation) whose first
/// bytes are a zip signature. It reuses the SAME magic-byte predicate
/// ([`bytes_start_with_zip_signature`]) so local and remote agree on "is a zip".
///
/// **Refuse-typed on an unavailable primitive.** If the parent can't do a
/// positioned read yet (`SmbVolume::read_range` before its smb2 primitive lands
/// → `NotSupported`), or the sniff hits a transient remote fault, we can't rule
/// the file out — so we route it anyway. The archive layer then re-attempts the
/// read while parsing and surfaces a clean typed "unreadable archive" rather than
/// mis-listing the `.zip` as a plain file. When the SMB primitive lands, the same
/// path simply starts sniffing real magic and browsing for real — no code change.
/// Only a *successful* read whose bytes AREN'T a zip signature (a genuinely
/// mislabeled remote file) declines the route.
async fn confirm_remote_archive_boundary(parent: &dyn Volume, zip_path: &Path) -> bool {
    match parent.get_metadata(zip_path).await {
        Ok(meta) if !meta.is_directory => {}
        // Not a file (a real remote dir named `foo.zip`), missing, or a metadata
        // fault: don't route.
        _ => return false,
    }
    match parent.read_range(zip_path, 0, 4).await {
        Ok(bytes) => bytes_start_with_zip_signature(&bytes),
        // Can't sniff (positioned reads unsupported, or a transient fault): route
        // and let the archive layer refuse typed if it's truly unreadable.
        Err(_) => true,
    }
}

/// The registry id for the `ArchiveVolume` over `zip_path`:
/// `archive-{hash(canonical zip path)}`. Canonicalized so two spellings of the
/// same file share one registration. This id is backend-internal only — it never
/// enters frontend state, history, or persistence (the FE holds the parent drive
/// id for display), so a fixed-seed hash that's stable within a process is all
/// it needs.
fn archive_volume_id(zip_path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let canonical = std::fs::canonicalize(zip_path).unwrap_or_else(|_| zip_path.to_path_buf());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut hasher);
    format!("archive-{:016x}", hasher.finish())
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

    /// Writes a file whose first bytes are a valid zip start-of-file signature.
    /// Enough for `confirm_archive_boundary`'s magic check (routing only; these
    /// tests never parse the archive).
    fn write_zip_magic(path: &Path) {
        std::fs::write(path, b"PK\x03\x04not-a-real-archive-body").expect("write zip magic");
    }

    /// Writes N zip-magic bytes into a non-local `InMemoryVolume` at `path`, so a
    /// resolve against it takes the REMOTE confirm path (parent metadata + a
    /// four-byte ranged read), the way a direct-SMB / MTP parent does.
    async fn remote_parent_with_zip(zip_path: &Path) -> InMemoryVolume {
        let parent = InMemoryVolume::new("Remote");
        parent
            .create_file(zip_path, b"PK\x03\x04not-a-real-archive-body")
            .await
            .expect("seed remote zip");
        parent
    }

    #[tokio::test]
    async fn resolve_passes_through_a_non_archive_path() {
        let manager = VolumeManager::new();
        manager.register("root", Arc::new(InMemoryVolume::new("Root")));

        let resolved = manager.resolve("root", Path::new("/some/plain/dir")).await;
        assert!(!resolved.is_archive);
        assert_eq!(resolved.path, PathBuf::from("/some/plain/dir"));
        assert_eq!(resolved.volume.expect("volume").name(), "Root");
    }

    #[tokio::test]
    async fn resolve_routes_an_archive_inner_path_to_an_archive_volume() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        write_zip_magic(&zip);

        let manager = VolumeManager::new();
        // A LOCAL parent (real temp file) takes the std::fs confirm path.
        manager.register("root", Arc::new(InMemoryVolume::new("Root").with_local_fs_access()));

        let inner = zip.join("docs/readme.txt");
        let resolved = manager.resolve("root", &inner).await;
        assert!(resolved.is_archive);
        // The path is handed back unchanged (the ArchiveVolume maps it itself).
        assert_eq!(resolved.path, inner);
        // The resolved volume is the archive: its root is the `.zip` path.
        assert_eq!(resolved.volume.expect("archive volume").root(), zip);
    }

    #[tokio::test]
    async fn resolve_routes_a_remote_zip_via_the_parents_ranged_reads() {
        // No local file exists; the parent is non-local and serves the `.zip`
        // bytes from its in-memory store through `get_metadata` + `read_range`.
        let zip = PathBuf::from("/device/bundle.zip");
        let manager = VolumeManager::new();
        manager.register("root", Arc::new(remote_parent_with_zip(&zip).await));

        let inner = zip.join("docs/readme.txt");
        let resolved = manager.resolve("root", &inner).await;
        assert!(resolved.is_archive, "a remote zip must route to an ArchiveVolume");
        assert_eq!(resolved.path, inner);
        assert_eq!(resolved.volume.expect("archive volume").root(), zip);
    }

    #[tokio::test]
    async fn resolve_declines_a_remote_directory_named_like_an_archive() {
        // A real remote directory named `foo.zip` must lose to normal navigation.
        let zip_dir = PathBuf::from("/device/folder.zip");
        let parent = InMemoryVolume::new("Remote");
        parent.create_directory(&zip_dir).await.expect("seed remote dir");
        let manager = VolumeManager::new();
        manager.register("root", Arc::new(parent));

        let resolved = manager.resolve("root", &zip_dir.join("sub")).await;
        assert!(!resolved.is_archive, "a remote dir named foo.zip is not an archive");
    }

    #[tokio::test]
    async fn resolve_declines_a_mislabeled_remote_file() {
        // A remote file with an archive extension but non-zip bytes is a plain file.
        let mislabeled = PathBuf::from("/device/notreally.zip");
        let parent = InMemoryVolume::new("Remote");
        parent
            .create_file(&mislabeled, b"plain text, definitely not a zip")
            .await
            .expect("seed remote file");
        let manager = VolumeManager::new();
        manager.register("root", Arc::new(parent));

        let resolved = manager.resolve("root", &mislabeled.join("inner")).await;
        assert!(!resolved.is_archive, "a mislabeled remote file is not an archive");
    }

    #[tokio::test]
    async fn resolve_routes_a_remote_zip_even_when_ranged_reads_are_unsupported() {
        // Models SMB before its positioned-read primitive lands: `get_metadata`
        // confirms the file, but `read_range` is `NotSupported`. We still route so
        // the archive layer surfaces a clean typed "unreadable" rather than
        // mis-listing the `.zip` as a plain file — and it lights up unchanged once
        // the primitive lands.
        let zip = PathBuf::from("/share/bundle.zip");
        let parent = InMemoryVolume::new("Remote").with_read_range_unsupported();
        parent
            .create_file(&zip, b"PK\x03\x04body")
            .await
            .expect("seed remote zip");
        let manager = VolumeManager::new();
        manager.register("root", Arc::new(parent));

        let resolved = manager.resolve("root", &zip.join("inner")).await;
        assert!(
            resolved.is_archive,
            "route on an unavailable positioned-read primitive so the archive layer refuses typed"
        );
    }

    #[tokio::test]
    async fn resolve_reuses_the_same_archive_volume_across_calls() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        write_zip_magic(&zip);

        let manager = VolumeManager::new();
        manager.register("root", Arc::new(InMemoryVolume::new("Root").with_local_fs_access()));

        let a = manager.resolve("root", &zip.join("a")).await.volume.expect("a");
        let b = manager.resolve("root", &zip.join("b")).await.volume.expect("b");
        // register_if_absent means the second resolve reuses the first volume.
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[tokio::test]
    async fn resolve_evicts_the_least_recently_used_archive_past_the_cap() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = VolumeManager::new();
        manager.register("root", Arc::new(InMemoryVolume::new("Root").with_local_fs_access()));

        // Resolve one more archive than the cap allows.
        let mut zips = Vec::new();
        for i in 0..=ARCHIVE_LRU_CAP {
            let zip = dir.path().join(format!("z{i}.zip"));
            write_zip_magic(&zip);
            manager.resolve("root", &zip.join("inner")).await;
            zips.push(zip);
        }

        // The registry holds the parent + exactly `ARCHIVE_LRU_CAP` archives:
        // the oldest was evicted.
        assert_eq!(manager.count(), 1 + ARCHIVE_LRU_CAP);

        // The first (oldest) archive is gone from the registry...
        let oldest_id = archive_volume_id(&zips[0]);
        assert!(manager.get(&oldest_id).is_none());
        // ...but re-resolving it re-registers lazily (eviction is harmless).
        let re = manager.resolve("root", &zips[0].join("inner")).await;
        assert!(re.is_archive);
        assert!(manager.get(&oldest_id).is_some());
    }

    #[tokio::test]
    async fn resolve_lists_a_real_zip_end_to_end() {
        use std::io::Write;

        let dir = tempfile::tempdir().expect("tempdir");
        let zip_path = dir.path().join("bundle.zip");
        {
            let file = std::fs::File::create(&zip_path).expect("create zip");
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("readme.txt", options).expect("start file");
            writer.write_all(b"hello").expect("write");
            writer.add_directory("docs/", options).expect("add dir");
            writer.finish().expect("finish zip");
        }

        let manager = VolumeManager::new();
        // The parent stands in for a local drive holding the `.zip`, so it must
        // report local FS access (else the archive takes its remote read path and
        // can't reach the real temp file).
        manager.register("root", Arc::new(InMemoryVolume::new("Root").with_local_fs_access()));

        // Resolving the `.zip` path lists the archive root through the ArchiveVolume.
        let resolved = manager.resolve("root", &zip_path).await;
        assert!(resolved.is_archive);
        let volume = resolved.volume.expect("archive volume");
        let entries = volume
            .list_directory(&resolved.path, None)
            .await
            .expect("list archive root");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"readme.txt"), "got: {names:?}");
        assert!(names.contains(&"docs"), "got: {names:?}");
    }

    #[tokio::test]
    async fn resolve_without_a_registered_parent_yields_no_volume() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("orphan.zip");
        write_zip_magic(&zip);

        let manager = VolumeManager::new();
        // No parent registered under "root".
        let resolved = manager.resolve("root", &zip.join("inner")).await;
        assert!(!resolved.is_archive);
        assert!(resolved.volume.is_none());
    }

    #[tokio::test]
    async fn path_is_inside_archive_confirms_a_local_inner_path_but_not_the_zip_itself() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        write_zip_magic(&zip);

        let manager = VolumeManager::new();
        manager.register("root", Arc::new(InMemoryVolume::new("Root").with_local_fs_access()));

        // A genuinely-inner path is inside the archive; the `.zip` file itself is a
        // plain file (write-routing must treat deleting/renaming it as a normal op).
        assert!(manager.path_is_inside_archive("root", &zip.join("inner.txt")).await);
        assert!(!manager.path_is_inside_archive("root", &zip).await);
        // `crosses` includes the `.zip` file itself (create-at-archive-root).
        assert!(manager.path_crosses_archive_boundary("root", &zip).await);
        assert!(
            manager
                .path_crosses_archive_boundary("root", &zip.join("inner.txt"))
                .await
        );
        // A plain path is neither.
        assert!(!manager.path_is_inside_archive("root", dir.path()).await);
        assert!(!manager.path_crosses_archive_boundary("root", dir.path()).await);
    }

    #[tokio::test]
    async fn path_is_inside_archive_confirms_a_remote_inner_path_through_the_parent() {
        // The bug this pins: the sync `archive::path_is_inside_archive` is
        // `std::fs`-only, so it silently returns false for an `smb://` / `mtp://`
        // zip and the remote archive-inner write falls through to the parent volume.
        // The async parent-aware method confirms through the parent's own I/O.
        let zip = PathBuf::from("/device/bundle.zip");
        let manager = VolumeManager::new();
        manager.register("root", Arc::new(remote_parent_with_zip(&zip).await));

        assert!(
            manager
                .path_is_inside_archive("root", &zip.join("docs/readme.txt"))
                .await,
            "a remote archive-inner path must be detected so the write reaches the edit driver"
        );
        // The remote `.zip` file itself is still a plain file.
        assert!(!manager.path_is_inside_archive("root", &zip).await);
        assert!(manager.path_crosses_archive_boundary("root", &zip).await);
    }

    #[tokio::test]
    async fn path_is_inside_archive_routes_a_remote_zip_even_without_ranged_reads() {
        // SMB before its positioned-read primitive lands: `get_metadata` confirms
        // the file but `read_range` is `NotSupported`. We still detect it as
        // archive-inner so the edit driver refuses typed rather than the write
        // silently landing on the parent volume.
        let zip = PathBuf::from("/share/bundle.zip");
        let parent = InMemoryVolume::new("Remote").with_read_range_unsupported();
        parent
            .create_file(&zip, b"PK\x03\x04body")
            .await
            .expect("seed remote zip");
        let manager = VolumeManager::new();
        manager.register("root", Arc::new(parent));

        assert!(manager.path_is_inside_archive("root", &zip.join("inner")).await);
    }

    #[tokio::test]
    async fn path_is_inside_archive_declines_a_mislabeled_remote_file() {
        let mislabeled = PathBuf::from("/device/notreally.zip");
        let parent = InMemoryVolume::new("Remote");
        parent
            .create_file(&mislabeled, b"plain text, definitely not a zip")
            .await
            .expect("seed remote file");
        let manager = VolumeManager::new();
        manager.register("root", Arc::new(parent));

        assert!(!manager.path_is_inside_archive("root", &mislabeled.join("inner")).await);
        assert!(
            !manager
                .path_crosses_archive_boundary("root", &mislabeled.join("inner"))
                .await
        );
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
