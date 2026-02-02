# MTP copy unification spec

**Goal**: Unify MTP and local filesystem copy operations so users get identical UX regardless of source/destination type.

## Current state

- **Local → Local**: Uses `CopyDialog.svelte` (confirmation) + `CopyProgressDialog.svelte` (progress with conflict resolution)
- **MTP ↔ Local**: Uses separate `MtpCopyDialog.svelte` with fewer features (no conflict resolution, no speed/ETA, no rollback)

## Target state

One unified flow using `CopyDialog` + `CopyProgressDialog` for ALL copy operations:
- Local → Local
- Local → MTP (upload)
- MTP → Local (download)

The Volume trait abstracts the differences, and the UI is identical.

## Implementation plan

### Phase 1: Extend Volume trait for copy operations

**File**: `apps/desktop/src-tauri/src/file_system/volume/mod.rs`

Add these methods to the Volume trait:

```rust
/// Result of scanning for copy operation
#[derive(Debug, Clone)]
pub struct CopyScanResult {
    pub file_count: usize,
    pub dir_count: usize,
    pub total_bytes: u64,
}

/// Information about a potential conflict
#[derive(Debug, Clone, Serialize)]
pub struct ConflictInfo {
    pub source_path: String,
    pub dest_path: String,
    pub source_size: u64,
    pub dest_size: u64,
    pub source_modified: Option<i64>,  // Unix timestamp
    pub dest_modified: Option<i64>,
}

/// Space information for a volume
#[derive(Debug, Clone, Serialize)]
pub struct SpaceInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

// Add to Volume trait:
trait Volume {
    // ... existing methods ...

    /// Scan a path recursively to get file/dir counts and total bytes.
    /// Used for pre-flight copy estimation.
    fn scan_for_copy(&self, path: &Path) -> Result<CopyScanResult, VolumeError>;

    /// Check destination for conflicts with source items.
    /// Returns list of files that already exist at destination.
    fn scan_for_conflicts(
        &self,
        source_items: &[(String, u64, Option<i64>)],  // (name, size, modified)
        dest_path: &Path,
    ) -> Result<Vec<ConflictInfo>, VolumeError>;

    /// Get space information for this volume.
    fn get_space_info(&self) -> Result<SpaceInfo, VolumeError>;

    /// Export a file/directory from this volume to a local path.
    /// Used when this volume is the SOURCE (e.g., MTP download).
    fn export_to_local(
        &self,
        source_path: &Path,
        local_dest: &Path,
        progress: Option<&dyn Fn(u64, u64)>,  // (bytes_done, bytes_total)
    ) -> Result<CopyResult, VolumeError>;

    /// Import a file/directory from a local path to this volume.
    /// Used when this volume is the DESTINATION (e.g., MTP upload).
    fn import_from_local(
        &self,
        local_source: &Path,
        dest_path: &Path,
        progress: Option<&dyn Fn(u64, u64)>,
    ) -> Result<CopyResult, VolumeError>;
}
```

### Phase 2: Implement for LocalVolume

**File**: `apps/desktop/src-tauri/src/file_system/volume/local.rs`

Most of these already exist or are trivial for local filesystem:

- `scan_for_copy`: Use existing `startScanPreview` logic or reimplement with walkdir
- `scan_for_conflicts`: List destination directory, check for name matches
- `get_space_info`: Use `statvfs` (already have `getVolumeSpace` command)
- `export_to_local`: Just `fs::copy` (source and dest are both local)
- `import_from_local`: Same as export (both local)

### Phase 3: Implement for MtpVolume

**File**: `apps/desktop/src-tauri/src/file_system/volume/mtp.rs`

- `scan_for_copy`: Already implemented in `MtpConnectionManager::scan_for_copy()`
- `scan_for_conflicts`: List MTP destination directory, compare with source names
- `get_space_info`: MTP provides `free_space_bytes` and `max_capacity` in storage info
- `export_to_local`: Already implemented as `download_recursive()`
- `import_from_local`: Already implemented as `uploadToMtp` command, needs recursive support

### Phase 4: Create unified copy backend

**New file**: `apps/desktop/src-tauri/src/file_system/copy.rs`

Create a unified copy orchestrator that:
1. Takes source volume + paths, destination volume + path
2. Runs pre-flight scan on source volume
3. Runs conflict scan on destination volume
4. Emits progress events using existing event types
5. Executes copy using `export_to_local` or `import_from_local` based on volume types

```rust
pub struct CopyOperation {
    id: String,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    conflict_policy: ConflictPolicy,
}

impl CopyOperation {
    pub async fn execute(&self, app: &AppHandle) -> Result<CopyResult, CopyError> {
        // 1. Scan source
        // 2. Scan for conflicts
        // 3. Check space
        // 4. Execute copy with progress events
        // 5. Handle conflicts according to policy
    }
}
```

### Phase 5: Add Tauri commands

**File**: `apps/desktop/src-tauri/src/commands/file_system.rs`

Add or modify commands:

```rust
#[tauri::command]
pub async fn copy_between_volumes(
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    conflict_policy: ConflictPolicy,
    // ... other options
) -> Result<CopyOperationId, String>;

#[tauri::command]
pub async fn scan_volume_for_conflicts(
    volume_id: String,
    source_items: Vec<SourceItemInfo>,
    dest_path: String,
) -> Result<Vec<ConflictInfo>, String>;

#[tauri::command]
pub async fn get_volume_space(
    volume_id: String,
) -> Result<SpaceInfo, String>;
```

### Phase 6: Update frontend to use unified flow

**File**: `apps/desktop/src/lib/file-explorer/DualPaneExplorer.svelte`

Modify `handleCopy()` to:
1. Determine source and destination volumes
2. Use the same `CopyDialog` for all cases
3. Use the same `CopyProgressDialog` for all cases
4. Remove MTP-specific branching

**File**: `apps/desktop/src/lib/write-operations/CopyDialog.svelte`

Add:
- Pre-flight conflict detection (new feature for all copy types)
- Display conflicts found with resolution options
- Handle MTP volumes in volume selector

**File**: `apps/desktop/src/lib/write-operations/CopyProgressDialog.svelte`

Works as-is - already handles progress events generically.

### Phase 7: Add pre-flight conflict detection UI

**File**: `apps/desktop/src/lib/write-operations/CopyDialog.svelte`

Enhance the dialog to:
1. After source scan completes, scan destination for conflicts
2. If conflicts found, show a summary: "3 files already exist"
3. Let user choose policy: Skip all / Overwrite all / Review each
4. "Review each" expands to show conflict list with checkboxes

```svelte
{#if conflictsFound.length > 0}
    <div class="conflicts-section">
        <p class="conflicts-summary">
            {conflictsFound.length} {conflictsFound.length === 1 ? 'file' : 'files'} already exist
        </p>
        <div class="conflict-policy">
            <label><input type="radio" bind:group={conflictPolicy} value="skip"> Skip all</label>
            <label><input type="radio" bind:group={conflictPolicy} value="overwrite"> Overwrite all</label>
            <label><input type="radio" bind:group={conflictPolicy} value="review"> Review each</label>
        </div>
        {#if conflictPolicy === 'review'}
            <div class="conflicts-list">
                {#each conflictsFound as conflict}
                    <label class="conflict-item">
                        <input type="checkbox" bind:checked={conflict.overwrite}>
                        <span>{conflict.name}</span>
                        <span class="conflict-sizes">
                            {formatBytes(conflict.sourceSize)} → {formatBytes(conflict.destSize)}
                        </span>
                    </label>
                {/each}
            </div>
        {/if}
    </div>
{/if}
```

### Phase 8: Clean up

After everything works:
1. Delete `MtpCopyDialog.svelte`
2. Remove MTP-specific copy handling from `DualPaneExplorer.svelte`
3. Update any imports/references

## TypeScript types to add

**File**: `apps/desktop/src/lib/tauri-commands.ts`

```typescript
export interface ConflictInfo {
    sourcePath: string
    destPath: string
    sourceSize: number
    destSize: number
    sourceModified: number | null
    destModified: number | null
}

export interface SpaceInfo {
    totalBytes: number
    availableBytes: number
    usedBytes: number
}

export type ConflictPolicy = 'skip' | 'overwrite' | 'stop'

export async function copyBetweenVolumes(
    sourceVolumeId: string,
    sourcePaths: string[],
    destVolumeId: string,
    destPath: string,
    options: CopyOptions
): Promise<{ operationId: string }>

export async function scanVolumeForConflicts(
    volumeId: string,
    sourceItems: Array<{ name: string; size: number; modified: number | null }>,
    destPath: string
): Promise<ConflictInfo[]>
```

## Testing requirements

### Unit tests (Rust)

1. `LocalVolume::scan_for_copy` - verify counts and bytes
2. `LocalVolume::scan_for_conflicts` - verify conflict detection
3. `MtpVolume::scan_for_copy` - mock MTP, verify counts
4. `MtpVolume::scan_for_conflicts` - mock MTP, verify detection
5. `CopyOperation` - test various conflict policies

### Unit tests (TypeScript)

1. `CopyDialog` - test conflict UI states
2. Verify event handling for progress

### Integration tests

1. Local → Local copy with conflicts (existing test, verify still works)
2. Progress events received correctly

### Manual testing checklist

With a real MTP device:
- [ ] Local → MTP upload single file
- [ ] Local → MTP upload directory
- [ ] Local → MTP with conflicts (file exists on device)
- [ ] MTP → Local download single file
- [ ] MTP → Local download directory
- [ ] MTP → Local with conflicts (file exists locally)
- [ ] Cancel during MTP transfer
- [ ] Rollback during MTP transfer
- [ ] Space check shows correct MTP storage info
- [ ] Speed/ETA displays correctly for MTP

## Success criteria

1. All existing tests pass
2. All checks pass (`./scripts/check.sh`)
3. MTP copy uses same dialogs as local copy
4. Pre-flight conflict detection works for all copy directions
5. `MtpCopyDialog.svelte` is deleted
6. No MTP-specific branching in copy flow (except Volume trait implementations)

## Out of scope

- MTP → MTP copy (within device) - would require download + upload, not worth it
- MTP file watching integration (separate feature, not part of copy)
- Preserving all metadata (permissions, xattrs) - MTP has limitations here

## Notes

- The mtp-rs library at `../mtp-rs` supports device events (`DeviceEvent::ObjectAdded`, etc.) - file watching can be added later
- MTP upload needs recursive directory support if not already present
- Progress reporting granularity may differ between local and MTP - that's acceptable
