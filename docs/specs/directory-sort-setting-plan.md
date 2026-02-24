# Directory sort setting

Add a setting to control how directories sort: either like files (by current sort column) or always by name.

## Current behavior

- `sort_entries()` in `sorting.rs` **always** places directories first, then sorts each group independently
- For **Name/Extension** columns: directories sort by name (correct)
- For **Size** column: directories sort by name because `FileEntry.size` is always `None` for dirs (all equal → name fallback). The `recursive_size` field is populated by the index system but **never used for sorting**
- For **Modified/Created** columns: directories sort by their modification/creation timestamps (already "like files")
- So the current behavior is inconsistent: Modified/Created sort dirs by date, but Size sorts dirs by name

## Goal

Two modes, controlled by a new setting:

| Mode | Behavior |
|------|----------|
| **Like files** (default) | Dirs sort by the active column just like files. For Size, use `recursive_size`. Dirs with no known size (`None`) sort last. Name ASC is always the secondary (tiebreaker) sort. |
| **Always by name** | Dirs always sort by name ASC among themselves, regardless of the active sort column. Files still sort by the active column. |

## Changes

### 1. Settings system (TypeScript)

**`apps/desktop/src/lib/settings/types.ts`**
- Add type: `export type DirectorySortMode = 'likeFiles' | 'alwaysByName'`
- Add to `SettingsValues`: `'listing.directorySortMode': DirectorySortMode`

**`apps/desktop/src/lib/settings/settings-registry.ts`**
- Add new setting in a new `General › Listing` section between Appearance and File operations:
  ```
  id: 'listing.directorySortMode'
  section: ['General', 'Listing']
  label: 'Sort directories'
  description: 'How directories are sorted when changing the sort column.'
  type: 'enum'
  default: 'likeFiles'
  component: 'toggle-group'
  options: [
    { value: 'likeFiles', label: 'Like files' },
    { value: 'alwaysByName', label: 'Always by name' }
  ]
  ```

### 2. Settings UI (Svelte)

**New file: `apps/desktop/src/lib/settings/sections/ListingSection.svelte`**
- Single setting with ToggleGroup, following AppearanceSection pattern

**`apps/desktop/src/lib/settings/components/SettingsContent.svelte`**
- Import and render ListingSection between Appearance and File operations

### 3. Reactive settings bridge

**`apps/desktop/src/lib/settings/reactive-settings.svelte.ts`**
- Add reactive state for `directorySortMode`
- Add getter `getDirectorySortMode()`
- Wire up `onSettingChange` handler

### 4. Frontend → backend wiring

The setting must flow to the Rust sort function. Two approaches exist:

**Approach: Pass through IPC**
- The frontend already passes `sortBy` and `sortOrder` to backend. Add `directorySortMode` to:
  - `listDirectoryStart()` IPC call
  - `resortListing()` IPC call
- Store `directory_sort_mode` in `CachedListing` so watcher re-sorts use the correct mode
- This keeps sorting entirely in Rust (fast, consistent)

**Files to change:**
- `apps/desktop/src/lib/tauri-commands/file-listing.ts` — add param to `listDirectoryStart` and `resortListing`
- `apps/desktop/src-tauri/src/commands/file_system.rs` — add param to Tauri command handlers
- `apps/desktop/src-tauri/src/file_system/listing/streaming.rs` — pass to `sort_entries`
- `apps/desktop/src-tauri/src/file_system/listing/operations.rs` — pass to `sort_entries`, store in `CachedListing`
- `apps/desktop/src-tauri/src/file_system/listing/caching.rs` — add field to `CachedListing`
- `apps/desktop/src/lib/file-explorer/pane/DualPaneExplorer.svelte` — pass setting to IPC calls

### 5. Rust sorting logic

**`apps/desktop/src-tauri/src/file_system/listing/sorting.rs`**

Add a `DirectorySortMode` enum and modify `sort_entries` signature:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DirectorySortMode {
    #[default]
    LikeFiles,
    AlwaysByName,
}
```

Update `sort_entries(entries, sort_by, sort_order, dir_sort_mode)`:

- **Both modes**: Directories still come first (the dirs-first partition is unchanged)
- **`AlwaysByName`**: Within the directory group, always sort by name (current behavior for Name/Size columns)
- **`LikeFiles`**: Within the directory group, sort by the active column:
  - **Name**: Sort by name (unchanged)
  - **Extension**: Sort by extension (unchanged — dirs don't have extensions, falls back to name)
  - **Size**: Use `recursive_size` (not `size`). Dirs with `recursive_size: None` sort **last** (after dirs with known size). Secondary sort: name ASC
  - **Modified**: Sort by `modified_at` (already works this way)
  - **Created**: Sort by `created_at` (already works this way)

Key detail for Size sorting with `LikeFiles`:
```rust
// Within directories, when sorting by size in LikeFiles mode:
// Use recursive_size. None sorts last (not first).
match (a.recursive_size, b.recursive_size) {
    (None, None) => name_cmp(a, b),     // Both unknown → name
    (None, Some(_)) => Ordering::Greater, // Unknown → last
    (Some(_), None) => Ordering::Less,    // Known → first
    (Some(a_sz), Some(b_sz)) => {
        let cmp = a_sz.cmp(&b_sz);
        if cmp == Ordering::Equal { name_cmp(a, b) } else { cmp }
    }
}
```

Also fix the existing **file** size sorting to use name ASC as secondary sort (currently no secondary sort when sizes are equal).

### 6. Frontend handleSortChange

**`apps/desktop/src/lib/file-explorer/pane/DualPaneExplorer.svelte`**
- Read `getDirectorySortMode()` before calling `resortListing()`
- Pass it through to the IPC call

### 7. Tests

**Rust tests (`sorting_test.rs`)**:
- Update all existing `sort_entries` calls to pass the new `dir_sort_mode` parameter (use `DirectorySortMode::LikeFiles` for most existing tests since that's the new default)
- Add new tests:
  - `test_dir_sort_like_files_by_size` — dirs with recursive_size sort by size
  - `test_dir_sort_like_files_size_none_last` — dirs without recursive_size sort last
  - `test_dir_sort_like_files_by_modified` — dirs sort by modification time
  - `test_dir_sort_always_by_name_ignores_size` — dirs sort by name even when column is Size
  - `test_dir_sort_always_by_name_ignores_modified` — dirs sort by name even when column is Modified
  - `test_dir_sort_secondary_name_asc` — when sizes are equal, secondary sort is name ASC

**TypeScript tests**:
- Add Vitest test for `ListingSection` component rendering
- Verify settings registry includes new setting with correct constraints

## Task list

### Milestone 1: Setting definition and UI
- [x] Add `DirectorySortMode` type and `SettingsValues` entry to `types.ts`
- [x] Add `listing.directorySortMode` to `settings-registry.ts`
- [x] Create `ListingSection.svelte` section component
- [x] Wire `ListingSection` into `SettingsContent.svelte`
- [x] Add reactive state and getter to `reactive-settings.svelte.ts`

### Milestone 2: Backend sorting logic
- [x] Add `DirectorySortMode` enum to `sorting.rs`
- [x] Update `sort_entries()` signature to accept `DirectorySortMode`
- [x] Implement `LikeFiles` mode: use `recursive_size` for directory size sorting, None-last
- [x] Implement `AlwaysByName` mode: always sort dirs by name
- [x] Add name ASC as secondary sort for equal values in all columns
- [x] Update all callers of `sort_entries()` to pass the new parameter

### Milestone 3: Frontend → backend wiring
- [x] Add `directorySortMode` param to `listDirectoryStart()` and `resortListing()` TS wrappers
- [x] Add param to Tauri command handlers in `commands/file_system.rs`
- [x] Store `directory_sort_mode` in `CachedListing`
- [x] Pass setting from `DualPaneExplorer.svelte` to IPC calls
- [x] Wire up `handleSortChange` to read and pass the setting
- [x] Re-sort both panes reactively when setting changes (`$effect` in DualPaneExplorer)

### Milestone 4: Tests and docs
- [x] Update all existing Rust sorting tests to pass `DirectorySortMode`
- [x] Add new Rust tests for both modes (8 new tests)
- [x] Update `CLAUDE.md` files (listing, settings, file-explorer)
- [x] Run `./scripts/check.sh --rust` and `./scripts/check.sh --svelte` to verify all passes
