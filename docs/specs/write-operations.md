# Write operations specification

This spec describes the requirements for production-ready copy, move, and delete operations in Cmdr. The current
implementation (v1) provides the basic structure but has gaps in safety, performance, and UX that must be addressed
before shipping.

## Current state

The v1 implementation in `apps/desktop/src-tauri/src/file_system/write_operations.rs` provides:

- Async operations with progress events
- Same-filesystem move via `rename()`
- Basic copy via `std::fs::copy`
- Recursive delete
- Cancellation support

## Requirements

### R1: macOS-native file operations

**Problem:** `std::fs::copy` does not preserve extended attributes, ACLs, resource forks, or Finder metadata.

**Solution:** Use macOS `copyfile(3)` API with appropriate flags.

#### R1.1: Use copyfile() for all copy operations

```rust
// Required flags for full fidelity copy
copyfile(src, dst, state, COPYFILE_ALL | COPYFILE_CLONE | COPYFILE_NOFOLLOW_SRC)
```

| Flag | Purpose |
|------|---------|
| `COPYFILE_ALL` | Copy data, metadata, xattrs, ACLs |
| `COPYFILE_CLONE` | Use clonefile when possible (APFS instant copy) |
| `COPYFILE_NOFOLLOW_SRC` | Copy symlinks as symlinks, not their targets |
| `COPYFILE_EXCL` | Fail if destination exists (when overwrite=false) |

#### R1.2: FFI bindings required

Add to `src/file_system/write_operations.rs` or a new `src/file_system/macos_copy.rs`:

```rust
#[cfg(target_os = "macos")]
mod ffi {
    use std::ffi::c_int;
    use std::os::raw::c_char;

    pub type copyfile_state_t = *mut std::ffi::c_void;
    pub type copyfile_flags_t = u32;

    pub const COPYFILE_ACL: copyfile_flags_t = 1 << 0;
    pub const COPYFILE_STAT: copyfile_flags_t = 1 << 1;
    pub const COPYFILE_XATTR: copyfile_flags_t = 1 << 2;
    pub const COPYFILE_DATA: copyfile_flags_t = 1 << 3;
    pub const COPYFILE_ALL: copyfile_flags_t = COPYFILE_ACL | COPYFILE_STAT | COPYFILE_XATTR | COPYFILE_DATA;
    pub const COPYFILE_CLONE: copyfile_flags_t = 1 << 24;
    pub const COPYFILE_NOFOLLOW_SRC: copyfile_flags_t = 1 << 18;
    pub const COPYFILE_EXCL: copyfile_flags_t = 1 << 17;

    // Progress callback constants
    pub const COPYFILE_STATE_STATUS_CB: c_int = 6;
    pub const COPYFILE_STATE_COPIED: c_int = 8;
    pub const COPYFILE_PROGRESS: c_int = 1;
    pub const COPYFILE_CONTINUE: c_int = 0;
    pub const COPYFILE_QUIT: c_int = 1;

    #[link(name = "System", kind = "dylib")]
    extern "C" {
        pub fn copyfile(
            from: *const c_char,
            to: *const c_char,
            state: copyfile_state_t,
            flags: copyfile_flags_t,
        ) -> c_int;

        pub fn copyfile_state_alloc() -> copyfile_state_t;
        pub fn copyfile_state_free(state: copyfile_state_t) -> c_int;
        pub fn copyfile_state_set(
            state: copyfile_state_t,
            flag: c_int,
            value: *const std::ffi::c_void,
        ) -> c_int;
        pub fn copyfile_state_get(
            state: copyfile_state_t,
            flag: c_int,
            value: *mut std::ffi::c_void,
        ) -> c_int;
    }
}
```

#### R1.3: Progress callback for large files

`copyfile` supports progress callbacks for byte-level progress on large files:

```rust
type CopyfileProgressCallback = extern "C" fn(
    what: c_int,
    stage: c_int,
    state: copyfile_state_t,
    src: *const c_char,
    dst: *const c_char,
    ctx: *mut c_void,
) -> c_int;
```

The callback receives `COPYFILE_PROGRESS` during data copy, allowing us to:
1. Query bytes copied via `copyfile_state_get(state, COPYFILE_STATE_COPIED, &bytes)`
2. Check cancellation flag and return `COPYFILE_QUIT` to abort
3. Emit progress events

### R2: Atomic cross-filesystem moves

**Problem:** Current implementation copies then deletes. If copy succeeds but delete fails, user has duplicates. If copy
fails midway, partial files remain.

**Solution:** Use atomic staging pattern.

#### R2.1: Staging directory

For cross-filesystem moves:

```
1. Create staging dir: {destination}/.cmdr-staging-{operation_id}/
2. Copy all files into staging dir (preserving structure)
3. For each file in staging:
   a. Rename from staging to final destination (atomic within same FS)
4. Delete source files
5. Remove staging directory
```

If operation fails at any point:
- Staging dir can be safely deleted (no data loss)
- Source files remain intact
- No partial files in destination

#### R2.2: Failure recovery

On any error after staging begins:
1. Stop operation
2. Delete staging directory recursively
3. Emit error event with details
4. Source files remain untouched

### R3: Rollback on copy failure

**Problem:** If copy operation fails midway, partial files remain at destination.

**Solution:** Track created files and clean up on failure.

```rust
struct CopyTransaction {
    created_files: Vec<PathBuf>,
    created_dirs: Vec<PathBuf>,
}

impl CopyTransaction {
    fn rollback(&self) {
        // Delete files first (in reverse order)
        for file in self.created_files.iter().rev() {
            let _ = fs::remove_file(file);
        }
        // Then directories (deepest first, already in reverse due to creation order)
        for dir in self.created_dirs.iter().rev() {
            let _ = fs::remove_dir(dir);
        }
    }
}
```

### R4: Symlink handling

**Problem:** Current implementation may copy symlink targets instead of the symlinks themselves.

**Solution:**

#### R4.1: Preserve symlinks

When copying a symlink:
```rust
let target = fs::read_link(source)?;
std::os::unix::fs::symlink(target, destination)?;
```

When using `copyfile()`, the `COPYFILE_NOFOLLOW_SRC` flag handles this.

#### R4.2: Handle broken symlinks

Broken symlinks should be copied as-is (preserving the target path). `copyfile` with `COPYFILE_NOFOLLOW_SRC` does this.

#### R4.3: Detect symlink loops

Before recursing into a directory, check for symlink loops:

```rust
fn is_symlink_loop(path: &Path, visited: &HashSet<PathBuf>) -> bool {
    if let Ok(canonical) = path.canonicalize() {
        visited.contains(&canonical)
    } else {
        false
    }
}
```

### R5: Conflict handling

**Problem:** Current implementation stops entirely on first conflict during batch operations.

**Solution:** Configurable conflict resolution.

#### R5.1: Conflict modes

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Stop operation on first conflict (current behavior)
    Stop,
    /// Skip conflicting files, continue with others
    Skip,
    /// Overwrite all conflicts
    Overwrite,
    /// Rename conflicting files (e.g., "file (1).txt")
    Rename,
}
```

#### R5.2: Conflict event

When `Stop` mode encounters a conflict, emit an event so frontend can ask user:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteConflictEvent {
    pub operation_id: String,
    pub source_path: String,
    pub destination_path: String,
    /// Whether destination is newer than source
    pub destination_is_newer: bool,
    /// Size difference (positive = destination is larger)
    pub size_difference: i64,
}
```

Add command to resume with chosen resolution:

```rust
#[tauri::command]
pub fn resolve_write_conflict(
    operation_id: String,
    resolution: ConflictResolution,
    apply_to_all: bool,
) -> Result<(), WriteOperationError>;
```

### R6: Large file progress

**Problem:** No progress indication while copying a single large file.

**Solution:** Use `copyfile` progress callback (R1.3) or chunked copy fallback.

#### R6.1: Chunked copy fallback (non-macOS or when copyfile fails)

```rust
const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks

fn copy_file_chunked(
    source: &Path,
    dest: &Path,
    state: &WriteOperationState,
    progress: &mut ProgressTracker,
) -> Result<u64, WriteOperationError> {
    let mut src = File::open(source)?;
    let mut dst = File::create(dest)?;
    let mut buf = [0u8; CHUNK_SIZE];
    let mut total = 0u64;

    loop {
        if state.cancelled.load(Ordering::Relaxed) {
            // Clean up partial file
            drop(dst);
            let _ = fs::remove_file(dest);
            return Err(WriteOperationError::Cancelled { ... });
        }

        let n = src.read(&mut buf)?;
        if n == 0 { break; }
        dst.write_all(&buf[..n])?;
        total += n as u64;
        progress.add_bytes(n as u64);
    }

    // Copy permissions
    let metadata = fs::metadata(source)?;
    fs::set_permissions(dest, metadata.permissions())?;

    Ok(total)
}
```

### R7: TypeScript types and frontend integration

**Problem:** No TypeScript types for frontend developers.

#### R7.1: Add to tauri-commands.ts

```typescript
// Types
export interface WriteOperationConfig {
  progressIntervalMs?: number;
  overwrite?: boolean;
  conflictResolution?: 'stop' | 'skip' | 'overwrite' | 'rename';
}

export interface WriteOperationStartResult {
  operationId: string;
  operationType: 'copy' | 'move' | 'delete';
}

export interface WriteProgressEvent {
  operationId: string;
  operationType: 'copy' | 'move' | 'delete';
  phase: 'scanning' | 'copying' | 'deleting';
  currentFile: string | null;
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number;
}

export interface WriteCompleteEvent {
  operationId: string;
  operationType: 'copy' | 'move' | 'delete';
  filesProcessed: number;
  bytesProcessed: number;
}

export interface WriteErrorEvent {
  operationId: string;
  operationType: 'copy' | 'move' | 'delete';
  error: WriteOperationError;
}

export type WriteOperationError =
  | { type: 'source_not_found'; path: string }
  | { type: 'destination_exists'; path: string }
  | { type: 'permission_denied'; path: string; message: string }
  | { type: 'insufficient_space'; required: number; available: number }
  | { type: 'same_location'; path: string }
  | { type: 'destination_inside_source'; source: string; destination: string }
  | { type: 'cancelled'; message: string }
  | { type: 'io_error'; path: string; message: string };

// Commands
export async function copyFiles(
  sources: string[],
  destination: string,
  config?: WriteOperationConfig
): Promise<WriteOperationStartResult> {
  return invoke('copy_files', { sources, destination, config });
}

export async function moveFiles(
  sources: string[],
  destination: string,
  config?: WriteOperationConfig
): Promise<WriteOperationStartResult> {
  return invoke('move_files', { sources, destination, config });
}

export async function deleteFiles(
  sources: string[],
  config?: WriteOperationConfig
): Promise<WriteOperationStartResult> {
  return invoke('delete_files', { sources, config });
}

export function cancelWriteOperation(operationId: string): void {
  invoke('cancel_write_operation', { operationId });
}
```

#### R7.2: Event subscription helpers

```typescript
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export function onWriteProgress(
  callback: (event: WriteProgressEvent) => void
): Promise<UnlistenFn> {
  return listen('write-progress', (e) => callback(e.payload as WriteProgressEvent));
}

export function onWriteComplete(
  callback: (event: WriteCompleteEvent) => void
): Promise<UnlistenFn> {
  return listen('write-complete', (e) => callback(e.payload as WriteCompleteEvent));
}

export function onWriteError(
  callback: (event: WriteErrorEvent) => void
): Promise<UnlistenFn> {
  return listen('write-error', (e) => callback(e.payload as WriteErrorEvent));
}

export function onWriteCancelled(
  callback: (event: WriteCancelledEvent) => void
): Promise<UnlistenFn> {
  return listen('write-cancelled', (e) => callback(e.payload as WriteCancelledEvent));
}
```

### R8: Performance optimizations

#### R8.1: Parallel small file operations (optional, measure first)

For directories with many small files (>100 files, <1MB each), consider parallel I/O:

```rust
use rayon::prelude::*;

// Only for delete - copy needs ordering for rollback
files.par_iter().try_for_each(|f| fs::remove_file(f))?;
```

**Caveat:** Parallel I/O can be slower on HDDs and some SSDs. Benchmark before enabling. Consider making this
configurable or auto-detecting based on file sizes.

#### R8.2: Batch metadata operations

When scanning, minimize syscalls by reusing directory handles:

```rust
// Good: single opendir, multiple readdir
for entry in fs::read_dir(path)? {
    // ...
}

// Avoid: stat() on every file during scan if not needed
```

### R9: Comprehensive testing

#### R9.1: Integration tests required

Create `apps/desktop/src-tauri/src/file_system/write_operations_integration_test.rs`:

| Test | Description |
|------|-------------|
| `test_copy_single_file` | Copy file, verify content and metadata match |
| `test_copy_directory_recursive` | Copy nested structure, verify all files |
| `test_copy_preserves_permissions` | Verify chmod bits preserved |
| `test_copy_preserves_symlinks` | Symlink copied as symlink, not target |
| `test_copy_preserves_xattrs` | Extended attributes preserved (use `xattr` crate to verify) |
| `test_copy_handles_broken_symlink` | Broken symlink copied as-is |
| `test_copy_detects_symlink_loop` | Error on symlink loop, no infinite recursion |
| `test_copy_rollback_on_failure` | Simulate failure, verify cleanup |
| `test_move_same_fs_uses_rename` | Verify inode unchanged (same filesystem) |
| `test_move_cross_fs_uses_staging` | Verify staging pattern used |
| `test_move_cross_fs_atomic` | Verify source intact if move fails |
| `test_delete_recursive` | Delete nested structure |
| `test_delete_preserves_on_error` | If one file fails, others remain |
| `test_cancellation_mid_copy` | Cancel during copy, verify partial cleanup |
| `test_cancellation_mid_delete` | Cancel during delete, verify remaining files |
| `test_conflict_stop_mode` | Verify operation stops on conflict |
| `test_conflict_skip_mode` | Verify conflicting files skipped |
| `test_conflict_overwrite_mode` | Verify files overwritten |
| `test_large_file_progress` | Verify progress events during large file copy |
| `test_concurrent_operations` | Multiple operations don't interfere |
| `test_special_characters_in_paths` | Unicode, spaces, quotes in filenames |
| `test_long_paths` | Paths near 1024 char limit |
| `test_empty_directory` | Copy/move/delete empty directories |
| `test_readonly_source` | Copy from readonly location |
| `test_readonly_destination` | Proper error when destination not writable |

#### R9.2: Stress tests

| Test | Description |
|------|-------------|
| `test_many_small_files` | 10,000 files, <1KB each |
| `test_large_file` | Single 1GB file |
| `test_deep_nesting` | 50+ levels of nesting |
| `test_wide_directory` | 50,000 files in one directory |

### R10: Error messages

All errors should be user-friendly and actionable.

| Error | Current message | Required message |
|-------|-----------------|------------------|
| Permission denied | "Permission denied" | "Cannot write to {path}: permission denied. Check folder permissions in Finder." |
| Disk full | "No space left on device" | "Not enough space on {volume}. Need {required}, have {available}." |
| Source not found | "No such file or directory" | "Cannot find {path}. It may have been moved or deleted." |
| Destination exists | "File exists" | "{filename} already exists in {destination}." |

## Non-requirements (out of scope)

- **Undo support** - Future feature, not required for v1
- **Background operations after app close** - Operations cancel if app closes
- **Network file systems** - SMB/NFS may have different semantics, test but don't optimize
- **Trash instead of delete** - Future feature, current delete is permanent

## Acceptance criteria

1. All R9 tests pass
2. Copy/move preserves all macOS metadata (verify with `ls -l@` showing xattrs)
3. Cross-filesystem move leaves no orphans on failure
4. Cancellation during large file copy cleans up partial file
5. Progress events fire at least every 200ms during large file operations
6. Same-filesystem move of 1M files completes in <100ms (rename speed)
7. TypeScript types compile with `pnpm svelte-check`
