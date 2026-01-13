# Volume filesystem abstraction: task list

**Parent document**: [2025-12-31-volume-fs-abstraction-plan.md](2025-12-31-volume-fs-abstraction-plan.md)

---

## Phase 1: Core abstraction (foundation) ✅ COMPLETED

> **Status**: All Phase 1 tasks completed on 2025-12-31
>
> - All 54 Rust tests pass
> - `./scripts/check.sh` passes
> - App still works correctly (verified via Tauri MCP)

### Task 1.1: Create Volume trait and error types ✅

**File to create**: `src-tauri/src/file_system/volume/mod.rs`

**Steps**:

1. Create the `volume/` subdirectory inside `src-tauri/src/file_system/`
2. Create `mod.rs` with the following contents:
    - `VolumeError` enum with variants: `NotFound(String)`, `PermissionDenied(String)`, `NotSupported`,
      `IoError(String)`
    - Implement `std::fmt::Display` for `VolumeError`
    - Implement `std::error::Error` for `VolumeError`
    - Implement `From<std::io::Error>` for `VolumeError` (for easy conversion)
    - The `Volume` trait with:
        - Required: `fn name(&self) -> &str`
        - Required: `fn root(&self) -> &Path`
        - Required: `fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError>`
        - Required: `fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError>`
        - Required: `fn exists(&self, path: &Path) -> bool`
        - Optional (with defaults): `fn create_file(...)`, `fn create_directory(...)`, `fn delete(...)`
        - Optional (with defaults): `fn supports_watching(&self) -> bool`, `fn start_watching(...)`,
          `fn stop_watching(...)`
3. Add `pub mod volume;` to `src-tauri/src/file_system/mod.rs`
4. Run `cargo check` to verify it compiles

**Acceptance criteria**:

- [ ] `cargo check` passes
- [ ] Trait is `Send + Sync` (required for thread-safe usage)
- [ ] All methods have doc comments explaining their purpose

**Code skeleton**:

```rust
//! Volume trait for abstracting file system access.

use super::FileEntry;
use std::path::Path;

/// Error type for volume operations.
#[derive(Debug, Clone)]
pub enum VolumeError {
    /// Path not found
    NotFound(String),
    /// Permission denied
    PermissionDenied(String),
    /// Operation not supported by this volume type
    NotSupported,
    /// Generic I/O error
    IoError(String),
}

impl std::fmt::Display for VolumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Path not found: {}", path),
            Self::PermissionDenied(path) => write!(f, "Permission denied: {}", path),
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for VolumeError {}

impl From<std::io::Error> for VolumeError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied(err.to_string()),
            _ => Self::IoError(err.to_string()),
        }
    }
}

/// Trait for volume file system operations.
///
/// Implementations provide access to different storage backends:
/// - `LocalPosixVolume`: Real local file system
/// - `InMemoryVolume`: In-memory file system for testing
pub trait Volume: Send + Sync {
    /// Returns the display name for this volume (e.g., "Macintosh HD", "Dropbox").
    fn name(&self) -> &str;

    /// Returns the root path of this volume.
    fn root(&self) -> &Path;

    /// Lists directory contents at the given path (relative to volume root).
    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError>;

    /// Gets metadata for a single path (relative to volume root).
    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError>;

    /// Checks if a path exists (relative to volume root).
    fn exists(&self, path: &Path) -> bool;

    /// Creates a file with the given content.
    fn create_file(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let _ = (path, content);
        Err(VolumeError::NotSupported)
    }

    /// Creates a directory.
    fn create_directory(&self, path: &Path) -> Result<(), VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Deletes a file or empty directory.
    fn delete(&self, path: &Path) -> Result<(), VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Returns true if this volume supports file watching.
    fn supports_watching(&self) -> bool {
        false
    }
}

// Re-export implementations
mod local_posix;
mod in_memory;

pub use local_posix::LocalPosixVolume;
pub use in_memory::InMemoryVolume;
```

---

### Task 1.2: Create LocalPosixVolume ✅

**File to create**: `src-tauri/src/file_system/volume/local_posix.rs`

**Steps**:

1. Create the file with the `LocalPosixVolume` struct
2. The struct holds:
    - `name: String` - display name (e.g., "Macintosh HD")
    - `root: PathBuf` - absolute root path (e.g., "/" or "/Users/you/Dropbox")
3. Implement `Volume` trait:
    - `name()`: Return `&self.name`
    - `root()`: Return `&self.root`
    - `list_directory(path)`:
        - Join `self.root` with `path` to get absolute path
        - Call existing `super::super::operations::list_directory_core()`
        - Convert `std::io::Error` to `VolumeError`
    - `get_metadata(path)`:
        - Join paths
        - Use `std::fs::metadata()` or `std::fs::symlink_metadata()`
        - Build a `FileEntry` from metadata (reuse logic from operations.rs)
    - `exists(path)`:
        - Join paths
        - Call `std::path::Path::exists()`
4. Add constructor: `fn new(name: impl Into<String>, root: impl Into<PathBuf>) -> Self`
5. Run `cargo check`

**Important**: For `get_metadata()`, extract the entry-building logic from `operations.rs` into a helper function that
both can use. Don't duplicate the complex metadata extraction code.

**Code skeleton**:

```rust
//! Local POSIX file system volume implementation.

use super::{Volume, VolumeError};
use crate::file_system::FileEntry;
use std::path::{Path, PathBuf};

/// A volume backed by the local POSIX file system.
pub struct LocalPosixVolume {
    name: String,
    root: PathBuf,
}

impl LocalPosixVolume {
    /// Creates a new local volume with the given name and root path.
    ///
    /// # Arguments
    /// * `name` - Display name (e.g., "Macintosh HD", "Dropbox")
    /// * `root` - Absolute path to the volume root (e.g., "/", "/Users/you/Dropbox")
    pub fn new(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
        }
    }

    /// Resolves a relative path to an absolute path within this volume.
    fn resolve(&self, path: &Path) -> PathBuf {
        if path.as_os_str().is_empty() || path == Path::new(".") {
            self.root.clone()
        } else {
            self.root.join(path)
        }
    }
}

impl Volume for LocalPosixVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let abs_path = self.resolve(path);
        crate::file_system::operations::list_directory_core(&abs_path).map_err(VolumeError::from)
    }

    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let abs_path = self.resolve(path);
        crate::file_system::operations::get_single_entry(&abs_path).map_err(VolumeError::from)
    }

    fn exists(&self, path: &Path) -> bool {
        self.resolve(path).exists()
    }

    fn supports_watching(&self) -> bool {
        true
    }
}
```

**Note**: We need to create `get_single_entry()` helper in `operations.rs` (see Task 2.1).

**Acceptance criteria**:

- [ ] `cargo check` passes
- [ ] Constructor works with various types (`&str`, `String`, `PathBuf`)
- [ ] Path resolution handles empty paths, ".", and relative paths correctly

---

### Task 1.3: Create InMemoryVolume ✅

**File to create**: `src-tauri/src/file_system/volume/in_memory.rs`

**Steps**:

1. Create the file with `InMemoryVolume` struct and helper types
2. Define internal entry type:
    ```rust
    struct InMemoryEntry {
        metadata: FileEntry,
        content: Option<Vec<u8>>,  // None for directories
    }
    ```
3. The struct holds:
    - `name: String`
    - `root: PathBuf` (typically "/" for in-memory)
    - `entries: RwLock<HashMap<PathBuf, InMemoryEntry>>`
4. Implement all `Volume` methods including optional ones:
    - `list_directory(path)`: Filter entries that have `path` as parent, return their FileEntry
    - `get_metadata(path)`: Look up entry in map
    - `exists(path)`: Check if key exists
    - `create_file(path, content)`: Insert new entry with `is_directory: false`
    - `create_directory(path)`: Insert new entry with `is_directory: true`
    - `delete(path)`: Remove entry from map
5. Add builder methods:
    - `fn new(name: &str) -> Self`
    - `fn with_entries(entries: Vec<FileEntry>) -> Self`
    - `fn with_file_count(count: usize) -> Self` (for stress testing)
6. Run `cargo check`

**Code skeleton**:

```rust
//! In-memory file system volume for testing.

use super::{Volume, VolumeError};
use crate::file_system::FileEntry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Entry in the in-memory file system.
struct InMemoryEntry {
    metadata: FileEntry,
    content: Option<Vec<u8>>,
}

/// An in-memory volume for testing without touching the real file system.
pub struct InMemoryVolume {
    name: String,
    root: PathBuf,
    entries: RwLock<HashMap<PathBuf, InMemoryEntry>>,
}

impl InMemoryVolume {
    /// Creates a new empty in-memory volume.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            root: PathBuf::from("/"),
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Creates an in-memory volume pre-populated with entries.
    pub fn with_entries(name: impl Into<String>, entries: Vec<FileEntry>) -> Self {
        let volume = Self::new(name);
        {
            let mut map = volume.entries.write().unwrap();
            for entry in entries {
                let path = PathBuf::from(&entry.path);
                map.insert(path, InMemoryEntry { metadata: entry, content: None });
            }
        }
        volume
    }

    /// Creates an in-memory volume with N auto-generated files for stress testing.
    pub fn with_file_count(name: impl Into<String>, count: usize) -> Self {
        let entries: Vec<FileEntry> = (0..count)
            .map(|i| {
                let is_dir = i % 10 == 0;
                let file_name = format!("file_{:06}.txt", i);
                FileEntry {
                    name: file_name.clone(),
                    path: format!("/{}", file_name),
                    is_directory: is_dir,
                    is_symlink: i % 50 == 0,
                    size: Some(1024 * (i as u64)),
                    modified_at: Some(1640000000 + i as u64),
                    created_at: Some(1639000000 + i as u64),
                    added_at: None,
                    opened_at: None,
                    permissions: 0o644,
                    owner: "testuser".to_string(),
                    group: "staff".to_string(),
                    icon_id: if is_dir { "dir".to_string() } else { "ext:txt".to_string() },
                    extended_metadata_loaded: true,
                }
            })
            .collect();
        Self::with_entries(name, entries)
    }

    /// Helper to get parent path.
    fn parent_of(path: &Path) -> PathBuf {
        path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("/"))
    }
}

impl Volume for InMemoryVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let entries = self.entries.read().map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        // Normalize the path we're looking for
        let target_dir = if path.as_os_str().is_empty() || path == Path::new(".") {
            PathBuf::from("/")
        } else if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        };

        // Find all entries whose parent matches this directory
        let mut result: Vec<FileEntry> = entries
            .iter()
            .filter(|(entry_path, _)| {
                let parent = Self::parent_of(entry_path);
                parent == target_dir
            })
            .map(|(_, entry)| entry.metadata.clone())
            .collect();

        // Sort: directories first, then alphabetically
        result.sort_by(|a, b| {
            match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        Ok(result)
    }

    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let entries = self.entries.read().map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        let normalized = if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        };

        entries
            .get(&normalized)
            .map(|e| e.metadata.clone())
            .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))
    }

    fn exists(&self, path: &Path) -> bool {
        let entries = match self.entries.read() {
            Ok(e) => e,
            Err(_) => return false,
        };

        let normalized = if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        };

        entries.contains_key(&normalized)
    }

    fn create_file(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let mut entries = self.entries.write().map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        let normalized = if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        };

        let name = normalized
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let metadata = FileEntry {
            name: name.clone(),
            path: normalized.display().to_string(),
            is_directory: false,
            is_symlink: false,
            size: Some(content.len() as u64),
            modified_at: Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)),
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o644,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            icon_id: "file".to_string(),
            extended_metadata_loaded: true,
        };

        entries.insert(normalized, InMemoryEntry {
            metadata,
            content: Some(content.to_vec()),
        });

        Ok(())
    }

    fn create_directory(&self, path: &Path) -> Result<(), VolumeError> {
        let mut entries = self.entries.write().map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        let normalized = if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        };

        let name = normalized
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let metadata = FileEntry {
            name,
            path: normalized.display().to_string(),
            is_directory: true,
            is_symlink: false,
            size: None,
            modified_at: Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)),
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o755,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            icon_id: "dir".to_string(),
            extended_metadata_loaded: true,
        };

        entries.insert(normalized, InMemoryEntry {
            metadata,
            content: None,
        });

        Ok(())
    }

    fn delete(&self, path: &Path) -> Result<(), VolumeError> {
        let mut entries = self.entries.write().map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        let normalized = if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        };

        entries
            .remove(&normalized)
            .map(|_| ())
            .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))
    }
}
```

**Acceptance criteria**:

- [ ] `cargo check` passes
- [ ] Can create empty volume, volume with entries, and stress test volume
- [ ] All methods work correctly with path normalization

---

### Task 1.4: Unit tests for Volume implementations ✅

> 28 volume-specific tests added and passing

**Files to create**:

- `src-tauri/src/file_system/volume/local_posix_test.rs`
- `src-tauri/src/file_system/volume/in_memory_test.rs`

**Steps for local_posix_test.rs**:

1. Test that `new()` creates volume with correct name and root
2. Test `resolve()` with empty path, ".", and relative paths
3. Test `list_directory()` on a known system directory (e.g., `/tmp`)
4. Test `exists()` returns true for root, false for nonexistent
5. Test that `supports_watching()` returns true

**Steps for in_memory_test.rs**:

1. Test empty volume has no entries
2. Test `with_entries()` populates correctly
3. Test `with_file_count()` creates correct number
4. Test `create_file()` then `exists()` then `get_metadata()`
5. Test `create_directory()` then `list_directory()` on parent
6. Test `delete()` removes entry
7. Test `delete()` on nonexistent returns error
8. Test `list_directory()` sorts correctly (dirs first, then alpha)

**Acceptance criteria**:

- [ ] All tests pass with `cargo nextest run`
- [ ] Tests cover happy paths and error cases

---

## Phase 2: Refactor operations ✅ COMPLETED

> **Status**: All Phase 2 tasks completed on 2025-12-31

### Task 2.1: Extract helper functions ✅

> `get_single_entry()` added to operations.rs during Phase 1

**File to modify**: `src-tauri/src/file_system/operations.rs`

**Steps**:

1. Create new function `get_single_entry(path: &Path) -> Result<FileEntry, std::io::Error>`:
    - Extract the metadata-reading logic from `list_directory_core()`
    - Handle symlinks correctly (check target for is_directory)
    - Return a single `FileEntry`
2. Refactor `list_directory_core()` to use `get_single_entry()` internally
3. Make `get_single_entry` public within crate: `pub(crate) fn get_single_entry(...)`
4. Run tests to ensure nothing broke

**Acceptance criteria**:

- [ ] `cargo nextest run` passes
- [ ] `get_single_entry` works for files, directories, and symlinks

---

### Task 2.2: Create VolumeManager ✅

> VolumeManager created with 8 tests

**File to create**: `src-tauri/src/file_system/volume_manager.rs`

**Steps**:

1. Create struct:
    ```rust
    pub struct VolumeManager {
        volumes: RwLock<HashMap<String, Arc<dyn Volume>>>,
        default_volume_id: RwLock<Option<String>>,
    }
    ```
2. Implement methods:
    - `new() -> Self`
    - `register(&self, id: &str, volume: Arc<dyn Volume>)`
    - `unregister(&self, id: &str)`
    - `get(&self, id: &str) -> Option<Arc<dyn Volume>>`
    - `default(&self) -> Option<Arc<dyn Volume>>`
    - `set_default(&self, id: &str)`
    - `list_volumes(&self) -> Vec<(String, String)>` (returns id, name pairs)
3. Add to `mod.rs`: `pub mod volume_manager; pub use volume_manager::VolumeManager;`
4. Add tests

**Acceptance criteria**:

- [ ] `cargo check` passes
- [ ] Can register, get, unregister volumes
- [ ] Default volume works correctly

---

### Task 2.3: Update LISTING_CACHE to track volume ✅

> `volume_id` field added to CachedListing

**File to modify**: `src-tauri/src/file_system/operations.rs`

**Steps**:

1. Update `CachedListing` struct to include volume ID:
    ```rust
    struct CachedListing {
        volume_id: String,  // NEW
        path: PathBuf,      // Now relative to volume root
        entries: Vec<FileEntry>,
    }
    ```
2. Update `list_directory_start` signature (internal only for now):
    - Add parameter for volume reference or ID
    - Store volume_id in cache
3. Update `get_listing_entries` and `update_listing_entries` to work with new structure
4. Keep existing public API working (use "default" volume implicitly)

**Acceptance criteria**:

- [ ] Existing functionality still works (backwards compatible)
- [ ] Cache entries track which volume they belong to

---

### Task 2.4: Create internal volume-aware listing functions

**File to modify**: `src-tauri/src/file_system/operations.rs`

**Steps**:

1. Create `list_directory_start_with_volume`:
    ```
    pub(crate) fn list_directory_start_with_volume(
        volume: &dyn Volume,
        path: &Path,
        include_hidden: bool,
    ) -> Result<ListingStartResult, VolumeError>
    ```
2. This function:
    - Calls `volume.list_directory(path)`
    - Generates listing ID
    - Stores in cache with volume info
    - Starts watcher if `volume.supports_watching()`
3. Keep existing `list_directory_start` as a wrapper that uses default volume
4. Run tests

**Acceptance criteria**:

- [ ] New function works with any Volume implementation
- [ ] Old function still works (uses default volume under the hood)

---

### Task 2.5: Update other operations functions

**File to modify**: `src-tauri/src/file_system/operations.rs`

**Steps**:

1. `get_file_range`, `get_total_count`, `find_file_index`, `get_file_at`:
    - These don't need volume awareness - they work with cached data
    - No changes needed (verify this is true)
2. `list_directory_end`:
    - Already works correctly (just needs to stop watcher and remove from cache)
    - No changes needed
3. Run full test suite

**Acceptance criteria**:

- [ ] All existing tests pass
- [ ] Cache operations work correctly

---

## Phase 3: Integrate watcher

### Task 3.1: Add watcher to LocalPosixVolume

**File to modify**: `src-tauri/src/file_system/volume/local_posix.rs`

**Steps**:

1. `supports_watching()` already returns `true`
2. For now, the watcher integration stays in `watcher.rs` - it calls into volume to re-read
3. Update `handle_directory_change` in `watcher.rs`:
    - Get volume from listing cache (or VolumeManager)
    - Call `volume.list_directory()` instead of `list_directory_core()` directly
4. This may require storing volume reference in `WatchedDirectory` or looking it up

**Decision point**: Does `WatchedDirectory` need to store a reference to the volume, or can it look it up by listing_id
from the cache?

**Acceptance criteria**:

- [ ] File watcher still works after refactoring
- [ ] Changes in watched directory emit events correctly

---

### Task 3.2: End-to-end watcher test

**Manual testing steps**:

1. Start app in dev mode: `pnpm tauri dev`
2. Navigate to a directory
3. In terminal, create a new file in that directory
4. Verify file appears in the app without manual refresh
5. Delete the file
6. Verify file disappears from app

**Acceptance criteria**:

- [ ] File additions appear automatically
- [ ] File deletions appear automatically
- [ ] File modifications appear automatically

---

## Phase 4: Update command layer ✅ COMPLETED

> **Status**: Phase 4 completed on 2025-12-31

### Task 4.1: Initialize VolumeManager in app ✅

> Global VOLUME_MANAGER added, init_volume_manager() called from lib.rs

**File to modify**: `src-tauri/src/lib.rs`

**Steps**:

1. Create static `VolumeManager`:
    ```rust
    static VOLUME_MANAGER: LazyLock<VolumeManager> = LazyLock::new(VolumeManager::new);
    ```
2. In app setup, register the root volume:
    ```
    let root_volume = Arc::new(LocalPosixVolume::new("Macintosh HD", "/"));
    VOLUME_MANAGER.register("root", root_volume);
    VOLUME_MANAGER.set_default("root");
    ```
3. Export getter function: `pub fn get_volume_manager() -> &'static VolumeManager`

**Acceptance criteria**:

- [ ] App starts without errors
- [ ] Default volume is available on startup

---

### Task 4.2: Update commands to use VolumeManager

**File to modify**: `src-tauri/src/commands/file_system.rs`

**Steps**:

1. Update `list_directory_start` command:
    - Get default volume from manager
    - Call the volume-aware internal function
2. All other commands (`get_file_range`, etc.) don't need changes - they work with listing_id
3. Run tests

**Acceptance criteria**:

- [ ] All commands work as before
- [ ] App functionality unchanged

---

### Task 4.3: Add volume_id parameter (optional, future-ready)

**File to modify**: `src-tauri/src/commands/file_system.rs`

**Steps**:

1. Add optional `volume_id: Option<String>` parameter to `list_directory_start`
2. If provided, look up that volume; otherwise use default
3. Frontend doesn't need to change (passes `null`/omits parameter)

**Acceptance criteria**:

- [ ] Command works with or without volume_id
- [ ] When specified, uses correct volume

---

## Phase 5: Testing and cleanup ✅ COMPLETED

> **Status**: Phase 5 completed on 2025-12-31
>
> - 5 new integration tests added
> - All 70 Rust tests passing
> - `./scripts/check.sh` passes

### Task 5.1: Create integration tests with InMemoryVolume ✅

> Created `integration_test.rs` with 5 comprehensive tests

**File to create**: `src-tauri/src/file_system/integration_tests.rs`

**Steps**:

1. Create test that:
    - Creates InMemoryVolume with known entries
    - Registers it as default in VolumeManager
    - Calls `list_directory_start_with_volume`
    - Verifies correct entries returned via `get_file_range`
    - Calls `list_directory_end`
2. Create test for large directory (50k files) - performance check
3. Create test for create/delete/list sequence

**Acceptance criteria**:

- [ ] All integration tests pass
- [ ] Tests don't touch real filesystem

---

### Task 5.2: Migrate mock_provider tests

**Files to modify**:

- `src-tauri/src/file_system/mock_provider_test.rs` → update or remove

**Steps**:

1. Review existing mock_provider tests
2. Rewrite them to use InMemoryVolume instead
3. Verify same test coverage

**Acceptance criteria**:

- [ ] All old test scenarios covered with new implementation
- [ ] Tests pass

---

### Task 5.3: Remove deprecated code

**Files to modify/delete**:

- `src-tauri/src/file_system/mock_provider.rs` → DELETE
- `src-tauri/src/file_system/mock_provider_test.rs` → DELETE
- `src-tauri/src/file_system/provider.rs` → DELETE
- `src-tauri/src/file_system/real_provider.rs` → DELETE
- Update `src-tauri/src/file_system/mod.rs` to remove references

**Steps**:

1. Delete the old files
2. Update mod.rs to remove `#[cfg(test)]` imports of old code
3. Run `cargo check` to find any remaining references
4. Fix any compilation errors
5. Run tests

**Acceptance criteria**:

- [ ] Old files deleted
- [ ] No references to old types remain
- [ ] All tests pass

---

### Task 5.4: Run full check suite

**Steps**:

1. Run `./scripts/check.sh`
2. Fix any issues

**Acceptance criteria**:

- [ ] `./scripts/check.sh` passes completely

---

### Task 5.5: Manual testing

**Steps**:

1. Start app: `pnpm tauri dev`
2. Test navigation to various directories
3. Test Brief and Full view modes
4. Test file operations (if any are implemented)
5. Test that watcher still works
6. Test hidden files toggle
7. Test large directories (use the 10k-file test folder)

**Acceptance criteria**:

- [ ] All manual tests pass
- [ ] No visible regressions

---

## Checkpoint: Questions to resolve before starting

Before implementing, confirm these assumptions:

1. **Watcher storage**: Should `WatchedDirectory` store the volume_id, or look it up from the listing cache?
    - Recommendation: Store volume_id in WatchedDirectory for direct lookup

2. **Path in FileEntry**: Currently `FileEntry.path` is absolute. With volumes, should it be:
    - A) Absolute (as now)
    - B) Relative to volume root
    - Recommendation: Keep absolute for now, simpler

3. **Error handling**: Should commands return `VolumeError` or convert to `String`?
    - Recommendation: Convert to String at command boundary (current pattern)

4. **Thread safety**: Is `Arc<dyn Volume>` the right level of sharing?
    - Yes, Volumes are thread-safe (Send + Sync), Arc allows shared ownership
