# Implementation plan: Backend-driven virtual scrolling

## Problem statement

When opening a 50k-file directory, we currently serialize all 50k FileEntry objects over IPC. The viewport only shows
~50 files at a time.

**Solution**: Keep all data in Rust, fetch only visible items on demand.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          RUST BACKEND                           │
├─────────────────────────────────────────────────────────────────┤
│  LISTING_CACHE: HashMap<listing_id, CachedListing>              │
│    - entries: Vec<FileEntry>  (all files, unfiltered, sorted)   │
│    - path: PathBuf                                              │
│                                                                 │
│  APIs return filtered data based on include_hidden param:       │
│    get_range(listing_id, start, count, include_hidden)          │
│    get_total_count(listing_id, include_hidden)                  │
│    find_index(listing_id, name, include_hidden)                 │
│    get_at(listing_id, index, include_hidden)                    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ IPC (~100-500 items per request)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       SVELTE FRONTEND                           │
├─────────────────────────────────────────────────────────────────┤
│  FilePane.svelte:                                               │
│    - listingId: string                                          │
│    - totalCount: number                                         │
│    - selectedIndex: number                                      │
│                                                                 │
│  BriefList/FullList.svelte:                                     │
│    - Prefetch buffer: ~500 items around current position        │
│    - Throttled scroll handling (100ms)                          │
│    - Renders only visible items from buffer                     │
└─────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Backend API changes

### 1.1 Rename session → listing

All "session" references become "listing":

- `SESSION_CACHE` → `LISTING_CACHE`
- `session_id` → `listing_id`
- `currentSessionId` → `currentListingId`

### 1.2 New Tauri commands

```
/// Get a range of entries (for virtual scrolling)
/// Returns entries with sync status included.
#[tauri::command]
fn get_file_range(
    listing_id: String,
    start: usize,
    count: usize,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String>

/// Get total count (for scrollbar sizing)
#[tauri::command]
fn get_total_count(listing_id: String, include_hidden: bool) -> Result<usize, String>

/// Find index of a file by name (for parent folder selection)
#[tauri::command]
fn find_file_index(
    listing_id: String,
    name: String,
    include_hidden: bool,
) -> Result<Option<usize>, String>

/// Get a single file at index (for SelectionInfo)
#[tauri::command]
fn get_file_at(
    listing_id: String,
    index: usize,
    include_hidden: bool,
) -> Result<Option<FileEntry>, String>
```

### 1.3 Modify [list_directory_start](../../src-tauri/src/file_system/operations.rs)

**Current return:**

```
SessionStartResult { session_id, total_count, entries: Vec<FileEntry>, has_more }
```

**New return:**

```
ListingStartResult { listing_id, total_count }  // No entries!
```

The `include_hidden` param is passed at start to get correct initial `total_count`.

### 1.4 Sync status in FileEntry

Add sync status to [FileEntry](../../src-tauri/src/file_system/operations.rs) struct so it's returned with file data:

```rust
pub struct FileEntry {
    // ... existing fields ...
    pub sync_status: Option<String>,  // "synced", "online_only", etc.
}
```

Fetch sync status when populating range response.

---

## Phase 2: Frontend changes

### 2.1 Delete files

- src/lib/file-explorer/FileDataStore.ts → DELETE
- src/lib/file-explorer/FileDataStore.test.ts → DELETE

### 2.2 Update FilePane.svelte

```typescript
// Remove
const CHUNK_SIZE = 5000
let fileStore = createFileDataStore()
let storeVersion = $state(0)

// Add
let listingId = $state('')
let totalCount = $state(0)
let includeHidden = $derived(showHiddenFiles)

// loadDirectory() now just gets listingId + totalCount
async function loadDirectory(path: string, selectName?: string) {
    const result = await listDirectoryStart(path, includeHidden)
    listingId = result.listingId
    totalCount = result.totalCount

    if (selectName) {
        const idx = await findFileIndex(listingId, selectName, includeHidden)
        selectedIndex = idx ?? 0
    }
}
```

### 2.3 Update BriefList/FullList.svelte

**New props:**

```typescript
interface Props {
    listingId: string
    totalCount: number
    selectedIndex: number
    includeHidden: boolean
    // ... existing callbacks
}
```

**Prefetch buffer (~500 items):**

```typescript
const PREFETCH_BUFFER = 500

let cachedEntries = $state<FileEntry[]>([])
let cachedRange = $state({ start: 0, end: 0 })

async function ensureRange(start: number, end: number) {
    // Expand to prefetch buffer
    const fetchStart = Math.max(0, start - PREFETCH_BUFFER / 2)
    const fetchEnd = Math.min(totalCount, end + PREFETCH_BUFFER / 2)

    // Only fetch if needed range isn't cached
    if (fetchStart < cachedRange.start || fetchEnd > cachedRange.end) {
        const entries = await getFileRange(listingId, fetchStart, fetchEnd - fetchStart, includeHidden)
        cachedEntries = entries
        cachedRange = { start: fetchStart, end: fetchEnd }
    }
}
```

**Throttled scroll (100ms):**

```typescript
let scrollThrottleTimer: ReturnType<typeof setTimeout> | undefined

function onScroll() {
    if (!scrollThrottleTimer) {
        void updateVisibleRange()
        scrollThrottleTimer = setTimeout(() => {
            scrollThrottleTimer = undefined
            void updateVisibleRange() // Trailing call
        }, 100)
    }
}
```

---

## Phase 3: Edge cases

### 3.1 Hidden files filtering

**Location**: Rust

**Implementation**: APIs accept `include_hidden: bool`. Rust iterates
[entries](../../src-tauri/src/file_system/mock_provider_test.rs) and skips hidden files when calculating indices/ranges
if `include_hidden = false`.

### 3.2 File watcher with index shifting

When files change _before_ the cursor, the view must shift:

```rust
// In watcher diff handler:
struct DiffResult {
    new_total_count: usize,
    cursor_shift: i32,  // +20 = 20 files added before cursor
    affected_visible_range: bool,
}
```

Frontend receives diff event:

```typescript
interface ListingDiffEvent {
    listingId: string
    newTotalCount: number
    cursorShift: number
    affectedVisibleRange: boolean
}

// On receiving:
totalCount = event.newTotalCount
selectedIndex = Math.max(0, selectedIndex + event.cursorShift)
if (event.affectedVisibleRange) {
    void refetchVisibleRange()
}
```

### 3.3 Parent folder navigation

When navigating up, call:

1. `findFileIndex(listingId, previousFolderName, includeHidden)` → e.g., returns 1000
2. `getFileRange(listingId, 750, 500, includeHidden)` → buffer around index 1000
3. Scroll to position: `index * ROW_HEIGHT`

### 3.4 Sync status

**Location**: Rust

Include `sync_status` in [FileEntry](../../src-tauri/src/file_system/operations.rs). Fetch when building range response
(Dropbox SDK calls are already async-friendly).

### 3.5 Max filename width

**Postponed** — complex with proportional fonts. Use reasonable default for now.

TODO: Consider char-width lookup table in Rust, or estimate based on extension + length.

---

## Deleted complexity

| Removed                                                   | Reason             |
| --------------------------------------------------------- | ------------------ |
| FileDataStore.ts                                          | Data stays in Rust |
| `CHUNK_SIZE`, chunking logic                              | Fetch on demand    |
| listDirectoryNextChunk                                    | Not needed         |
| `loadingMore` state                                       | Not needed         |
| appendFiles() and mergeExtendedData() in FileDataStore.ts | Not needed         |

---

## Added complexity

| Added                       | Purpose                    |
| --------------------------- | -------------------------- |
| `get_file_range`            | Core virtual scroll API    |
| `get_file_at`               | Selected file info         |
| `find_file_index`           | Navigation selection       |
| Prefetch buffer (500 items) | Smooth scrolling           |
| Throttled scroll (100ms)    | Avoid IPC spam             |
| Cursor shift logic          | File watcher index updates |

---

## Test plan

### Rust unit tests

- `get_file_range` returns correct slice
- `get_file_range` clamps out-of-bounds
- `find_file_index` finds correct index with/without hidden
- Hidden file filtering works correctly
- Watcher diff computes correct cursor shift

### TypeScript unit tests

- Prefetch buffer logic
- Throttle behavior
- Index calculation from scroll position

### E2E tests

- 50k folder: first files appear <200ms
- Scroll: smooth, no blank areas
- Toggle hidden: filters correctly
- File watcher: updates + cursor shift work
- Go to parent: previous folder selected

---

## Decisions (resolved)

| Decision              | Choice                       |
| --------------------- | ---------------------------- |
| Hidden file filtering | Rust                         |
| Extended metadata     | Include in response          |
| Scroll strategy       | Throttle 100ms with trailing |
| Sync status storage   | Rust (in FileEntry)          |
| Max filename width    | Postponed                    |
| Prefetch buffer       | 500 items                    |

---

## TODO for future

- Sorting: Add `sort_by`, `sort_order` params to APIs (mention in code as TODO)
- Max filename width: Implement char-width lookup table

---

## Estimated effort

| Phase                               | Effort          |
| ----------------------------------- | --------------- |
| Phase 1: Backend APIs               | 3-4 hours       |
| Phase 2: Frontend refactor          | 4-5 hours       |
| Phase 3: Edge cases (watcher, etc.) | 3-4 hours       |
| Testing + polish                    | 2-3 hours       |
| **Total**                           | **12-16 hours** |
