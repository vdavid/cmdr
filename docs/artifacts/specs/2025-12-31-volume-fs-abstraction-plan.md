# Volume filesystem abstraction plan

**Date**: 2025-12-31  
**Status**: ✅ **FULLY IMPLEMENTED** (completed 2025-12-31, 80 tests passing)  
**Goal**: Abstract file system access to enable multiple volume types and testability

## Summary

Introduce a `Volume` trait that abstracts file system operations. This enables:

1. **LocalPosixVolume** - Real file system access (with configurable root path)
2. **InMemoryVolume** - In-memory file system for testing

Future implementations (out of scope): NetworkShareVolume (SMB/AFP), S3Volume, SftpVolume.

## Design decisions

| #   | Decision    | Choice                                                              |
| --- | ----------- | ------------------------------------------------------------------- |
| 1   | Terminology | **Volume** (macOS-native)                                           |
| 2   | Trait scope | **Read required + optional write** (defaults return `NotSupported`) |
| 3   | Root path   | **In the instance** (each Volume has a root)                        |
| 4   | Path type   | **`&Path`** (standard Rust, converted internally)                   |
| 5   | TestFS      | **In-memory** (supports create/delete for realistic tests)          |
| 6   | Watcher     | **Default no-op** in trait, LocalPosix overrides                    |
| 7   | API pattern | **Provider owns the API** (methods on Volume)                       |
| 8   | FileEntry   | **Keep as-is** (use `None` for unsupported fields)                  |

## Architecture

### Before (current)

```
commands/file_system.rs
    └─> file_system/operations.rs
            └─> std::fs (direct calls)
            └─> LISTING_CACHE (static)
```

### After

```
commands/file_system.rs
    └─> VolumeManager (holds active volumes)
            └─> dyn Volume (trait object)
                    ├── LocalPosixVolume (wraps std::fs)
                    └── InMemoryVolume (for tests)
```

## New files and modules

```
src-tauri/src/file_system/
├── mod.rs                    # Updated: exports new types
├── volume/
│   ├── mod.rs                # Volume trait + error types
│   ├── local_posix.rs        # LocalPosixVolume implementation
│   └── in_memory.rs          # InMemoryVolume for testing
├── operations.rs             # Refactored: uses Volume trait
├── watcher.rs                # Minor updates for Volume integration
└── (other existing files)
```

## Trait design

```rust
/// Error type for volume operations
#[derive(Debug, Clone)]
pub enum VolumeError {
    NotFound(String),
    PermissionDenied(String),
    NotSupported,
    IoError(String),
}

/// Core trait for volume file system operations
pub trait Volume: Send + Sync {
    /// Returns the display name for this volume (e.g., "Macintosh HD", "Dropbox")
    fn name(&self) -> &str;

    /// Returns the root path of this volume
    fn root(&self) -> &Path;

    // ========================================
    // Required: All volumes must implement
    // ========================================

    /// Lists directory contents at the given path (relative to volume root)
    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError>;

    /// Gets metadata for a single path (relative to volume root)
    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError>;

    /// Checks if a path exists (relative to volume root)
    fn exists(&self, path: &Path) -> bool;

    // ========================================
    // Optional: Default to NotSupported
    // ========================================

    /// Creates a file with the given content
    fn create_file(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let _ = (path, content);
        Err(VolumeError::NotSupported)
    }

    /// Creates a directory
    fn create_directory(&self, path: &Path) -> Result<(), VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Deletes a file or empty directory
    fn delete(&self, path: &Path) -> Result<(), VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    // ========================================
    // Watching: Optional, default no-op
    // ========================================

    /// Returns true if this volume supports file watching
    fn supports_watching(&self) -> bool {
        false
    }

    /// Starts watching a directory (no-op by default)
    fn start_watching(
        &self,
        _path: &Path,
        _callback: Box<dyn Fn() + Send + Sync>,
    ) -> Result<(), VolumeError> {
        Ok(()) // No-op default
    }

    /// Stops watching (no-op by default)
    fn stop_watching(&self, _path: &Path) {}
}
```

## Implementation details

### LocalPosixVolume

- Wraps real `std::fs` operations
- Constructor takes `name` and `root_path`
- Implements all required methods by calling existing `operations.rs` code
- Overrides `supports_watching()` → `true`
- Integrates with existing `notify-debouncer-full` watcher

**Key**: The existing `list_directory_core()` and related functions will be refactored to take paths relative to a root,
or the Volume will convert paths before calling them.

### InMemoryVolume

- Stores entries in a `HashMap<PathBuf, InMemoryEntry>`
- Supports all optional methods (create, delete)
- Useful for testing file operations without disk I/O
- Can simulate large directories (50k+ files) for stress tests

```rust
struct InMemoryEntry {
    metadata: FileEntry,
    content: Option<Vec<u8>>,  // None for directories
    children: Vec<String>,     // For directories
}

pub struct InMemoryVolume {
    name: String,
    root: PathBuf,
    entries: RwLock<HashMap<PathBuf, InMemoryEntry>>,
}
```

### VolumeManager

A registry of available volumes, used by the command layer:

```rust
pub struct VolumeManager {
    volumes: RwLock<HashMap<String, Arc<dyn Volume>>>,
    default_volume_id: RwLock<Option<String>>,
}

impl VolumeManager {
    pub fn register(&self, id: &str, volume: Arc<dyn Volume>);
    pub fn get(&self, id: &str) -> Option<Arc<dyn Volume>>;
    pub fn default(&self) -> Option<Arc<dyn Volume>>;
}
```

### Migration strategy

1. **Phase 1**: Create new `volume/` module with trait and implementations
2. **Phase 2**: Refactor `operations.rs` to call through Volume trait internally
3. **Phase 3**: Update `commands/file_system.rs` to use VolumeManager
4. **Phase 4**: Migrate existing tests to use InMemoryVolume
5. **Phase 5**: Remove old test-only `#[cfg(test)]` provider code

## Tasks

### Phase 1: Core abstraction (foundation)

- [ ] **1.1** Create `file_system/volume/mod.rs` with `Volume` trait and `VolumeError`
- [ ] **1.2** Create `file_system/volume/local_posix.rs` with `LocalPosixVolume`
    - Implement required methods: `list_directory`, `get_metadata`, `exists`
    - Call into existing `operations.rs` code initially
- [ ] **1.3** Create `file_system/volume/in_memory.rs` with `InMemoryVolume`
    - Implement all methods including `create_file`, `create_directory`, `delete`
    - Add helper methods: `with_entries()`, `with_file_count()` for test setup
- [ ] **1.4** Add unit tests for both implementations

### Phase 2: Refactor operations

- [ ] **2.1** Extract path resolution logic (joining root + relative path)
- [ ] **2.2** Create `VolumeManager` struct
- [ ] **2.3** Refactor `list_directory_start` to accept a Volume reference
- [ ] **2.4** Update `LISTING_CACHE` to store which volume each listing belongs to
- [ ] **2.5** Refactor `get_file_range`, `get_total_count`, etc. to work with volumes

### Phase 3: Integrate watcher

- [ ] **3.1** Add watcher methods to LocalPosixVolume
- [ ] **3.2** Update `handle_directory_change` to use Volume for re-reading
- [ ] **3.3** Test that file watching still works end-to-end

### Phase 4: Update command layer

- [ ] **4.1** Initialize VolumeManager in app setup (create "root" LocalPosixVolume)
- [ ] **4.2** Update `list_directory_start` command to use default volume
- [ ] **4.3** Add volume ID parameter to commands (optional, for future multi-volume)

### Phase 5: Testing and cleanup

- [ ] **5.1** Create integration tests using InMemoryVolume
- [ ] **5.2** Migrate existing `mock_provider` tests to use InMemoryVolume
- [ ] **5.3** Remove deprecated `#[cfg(test)]` provider code
- [ ] **5.4** Run full check suite, fix any issues
- [ ] **5.5** Manual testing: verify app still works normally

## Out of scope (future)

- Volume chooser UI
- NetworkShareVolume, S3Volume, SftpVolume implementations
- Multi-volume support in frontend
- Volume configuration/settings UI

## Risks and mitigations

| Risk                            | Mitigation                                                     |
| ------------------------------- | -------------------------------------------------------------- |
| Breaking existing functionality | Each phase ends with working code, run checks after each       |
| Performance regression          | LocalPosixVolume adds minimal overhead (one trait dispatch)    |
| Watcher complexity              | Keep watcher logic mostly unchanged, just route through volume |

## Success criteria

1. All existing tests pass
2. `./scripts/check.sh` passes
3. App works exactly as before (manual test)
4. New tests for InMemoryVolume demonstrate testability
5. Code is ready for future volume types
