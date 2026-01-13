# Non-blocking folder loading

**Status**: Spec  
**Created**: 2026-01-13  
**Author**: Claude (with David)

## Problem statement

When navigating to a slow-loading folder (network drives, large directories), the **entire application freezes**:

1. **Tab doesn't work** – The user can't switch to the other pane while one pane is loading
2. **No visual feedback** – The loading spinner appears, but there's no indication of progress
3. **No way to cancel** – The user must wait for the load to complete or force-quit the app
4. **Startup freeze** – If the app was closed while viewing a slow network folder, restarting shows a full-screen spinner with no explanation

### User impact

- Network folders on NAS devices can take 30+ seconds to list
- Large folders (50k+ files) cause noticeable delays even on local SSDs
- The frozen state feels like a crash, damaging user trust

## Root causes

### 1. Synchronous Tauri command

The `list_directory_start` command is defined as a **sync function**:

```rust
#[tauri::command]
pub fn list_directory_start(...) -> Result<ListingStartResult, String> {
```

Tauri runs sync commands on the main thread. While `fs::read_dir()` iterates, the entire Rust runtime blocks, preventing any other commands from executing.

### 2. Frontend awaits completion

```typescript
// FilePane.svelte
const result = await listDirectoryStart(path, ...)
```

The component is stuck at this await until Rust returns. No UI updates, no keyboard handling.

### 3. No cancellation mechanism

The `loadGeneration` counter discards stale results on the frontend, but:
- Rust keeps working even if the user navigates away
- The UI can't do anything while waiting
- There's no way to abort the operation

### 4. History pushed before load completes

Navigation history is updated when `loadDirectory()` starts, meaning cancelled navigations leave incorrect history entries.

## Key files involved

### Rust (backend)

| File | Role |
|------|------|
| `apps/desktop/src-tauri/src/commands/file_system.rs` | Tauri command definitions |
| `apps/desktop/src-tauri/src/file_system/operations.rs` | `list_directory_start_with_volume()`, `CachedListing`, sorting |
| `apps/desktop/src-tauri/src/file_system/volume/local_posix.rs` | `LocalPosixVolume::list_directory()` |

### Svelte (frontend)

| File | Role |
|------|------|
| `apps/desktop/src/lib/file-explorer/DualPaneExplorer.svelte` | Top-level component, Tab handling, history management |
| `apps/desktop/src/lib/file-explorer/FilePane.svelte` | Per-pane loading logic, `loadDirectory()` function |
| `apps/desktop/src/lib/LoadingIcon.svelte` | Loading spinner component |
| `apps/desktop/src/lib/tauri-commands.ts` | TypeScript wrappers for Tauri commands |
| `apps/desktop/src/lib/file-explorer/types.ts` | TypeScript types |

## Solution: Streaming directory listing with event-based progress

### Overview

Transform directory listing from a blocking request-response pattern to an async streaming pattern:

1. **`list_directory_start` returns immediately** with `{ listingId, status: 'loading' }`
2. **Rust spawns a background task** that reads the directory asynchronously
3. **Progress events** are emitted every 500ms with the current file count
4. **Completion event** is emitted when done, containing total count and metadata
5. **Frontend stays responsive** – Tab, ESC, and other keys work during loading
6. **ESC cancels loading** and navigates back in history (or to home if history is empty)

### Rust implementation

#### New types

```rust
// In operations.rs

/// Status of a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ListingStatus {
    /// Listing is in progress
    Loading,
    /// Listing completed successfully  
    Ready,
    /// Listing was cancelled by the user
    Cancelled,
    /// Listing failed with an error
    Error { message: String },
}

/// Result of starting a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingListingStartResult {
    /// Unique listing ID for subsequent API calls
    pub listing_id: String,
    /// Initial status (always "loading")
    pub status: ListingStatus,
}

/// Progress event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingProgressEvent {
    pub listing_id: String,
    pub loaded_count: usize,
}

/// Completion event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingCompleteEvent {
    pub listing_id: String,
    pub total_count: usize,
    pub max_filename_width: Option<f32>,
}

/// Error event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingErrorEvent {
    pub listing_id: String,
    pub message: String,
}
```

#### Streaming listing state

```rust
// In operations.rs

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// State for an in-progress streaming listing
struct StreamingListingState {
    /// Cancellation flag - checked periodically during iteration
    cancelled: AtomicBool,
    /// Current status
    status: RwLock<ListingStatus>,
}

/// Cache for streaming state (separate from completed listings cache)
static STREAMING_STATE: LazyLock<RwLock<HashMap<String, Arc<StreamingListingState>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
```

#### Async command implementation

```rust
// In commands/file_system.rs

#[tauri::command]
pub async fn list_directory_start(
    app: tauri::AppHandle,
    path: String,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
) -> Result<StreamingListingStartResult, String> {
    let expanded_path = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded_path);
    
    ops_list_directory_start_streaming(
        app,
        "root",
        &path_buf,
        include_hidden,
        sort_by,
        sort_order,
    )
    .await
    .map_err(|e| format!("Failed to start directory listing '{}': {}", path, e))
}

#[tauri::command]
pub fn cancel_listing(listing_id: String) {
    ops_cancel_listing(&listing_id);
}
```

#### Streaming implementation

```rust
// In operations.rs

pub async fn list_directory_start_streaming(
    app: tauri::AppHandle,
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
) -> Result<StreamingListingStartResult, std::io::Error> {
    // Generate listing ID immediately
    let listing_id = Uuid::new_v4().to_string();
    
    // Create streaming state with cancellation flag
    let state = Arc::new(StreamingListingState {
        cancelled: AtomicBool::new(false),
        status: RwLock::new(ListingStatus::Loading),
    });
    
    // Store state for cancellation
    if let Ok(mut cache) = STREAMING_STATE.write() {
        cache.insert(listing_id.clone(), Arc::clone(&state));
    }
    
    // Clone values for the spawned task
    let listing_id_clone = listing_id.clone();
    let path_owned = path.to_path_buf();
    let volume_id_owned = volume_id.to_string();
    
    // Spawn background task
    tokio::spawn(async move {
        // Run blocking I/O on dedicated thread pool
        let result = tokio::task::spawn_blocking(move || {
            read_directory_with_progress(
                &app,
                &listing_id_clone,
                &state,
                &volume_id_owned,
                &path_owned,
                include_hidden,
                sort_by,
                sort_order,
            )
        }).await;
        
        // Clean up streaming state
        if let Ok(mut cache) = STREAMING_STATE.write() {
            cache.remove(&listing_id_clone);
        }
        
        match result {
            Ok(Ok(())) => { /* Success - events already emitted */ }
            Ok(Err(e)) => {
                // Emit error event
                let _ = app.emit("listing-error", ListingErrorEvent {
                    listing_id: listing_id_clone,
                    message: e.to_string(),
                });
            }
            Err(e) => {
                // Task panicked
                let _ = app.emit("listing-error", ListingErrorEvent {
                    listing_id: listing_id_clone,
                    message: format!("Task failed: {}", e),
                });
            }
        }
    });
    
    Ok(StreamingListingStartResult {
        listing_id,
        status: ListingStatus::Loading,
    })
}

fn read_directory_with_progress(
    app: &tauri::AppHandle,
    listing_id: &str,
    state: &Arc<StreamingListingState>,
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
) -> Result<(), std::io::Error> {
    let mut entries = Vec::new();
    let mut last_progress_time = std::time::Instant::now();
    const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
    
    // Get the volume
    let volume = super::get_volume_manager().get(volume_id).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, format!("Volume '{}' not found", volume_id))
    })?;
    
    // Read directory entries one by one
    for entry in std::fs::read_dir(path)? {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit("listing-cancelled", serde_json::json!({ "listingId": listing_id }));
            return Ok(());
        }
        
        let entry = entry?;
        
        // Process entry (same logic as list_directory_core)
        if let Some(file_entry) = process_dir_entry(&entry) {
            entries.push(file_entry);
        }
        
        // Emit progress every 500ms
        if last_progress_time.elapsed() >= PROGRESS_INTERVAL {
            let _ = app.emit("listing-progress", ListingProgressEvent {
                listing_id: listing_id.to_string(),
                loaded_count: entries.len(),
            });
            last_progress_time = std::time::Instant::now();
        }
    }
    
    // Check cancellation one more time before finalizing
    if state.cancelled.load(Ordering::Relaxed) {
        let _ = app.emit("listing-cancelled", serde_json::json!({ "listingId": listing_id }));
        return Ok(());
    }
    
    // Sort entries
    sort_entries(&mut entries, sort_by, sort_order);
    
    // Calculate counts and metadata
    let total_count = if include_hidden {
        entries.len()
    } else {
        entries.iter().filter(|e| !e.name.starts_with('.')).count()
    };
    
    let max_filename_width = {
        let font_id = "system-400-12";
        let filenames: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        crate::font_metrics::calculate_max_width(&filenames, font_id)
    };
    
    // Cache the completed listing
    if let Ok(mut cache) = LISTING_CACHE.write() {
        cache.insert(
            listing_id.to_string(),
            CachedListing {
                volume_id: volume_id.to_string(),
                path: path.to_path_buf(),
                entries,
                sort_by,
                sort_order,
            },
        );
    }
    
    // Start file watcher (only after listing is complete)
    if volume.supports_watching() {
        if let Err(e) = start_watching(listing_id, path) {
            eprintln!("[LISTING] Failed to start watcher: {}", e);
        }
    }
    
    // Emit completion event
    let _ = app.emit("listing-complete", ListingCompleteEvent {
        listing_id: listing_id.to_string(),
        total_count,
        max_filename_width,
    });
    
    Ok(())
}

pub fn cancel_listing(listing_id: &str) {
    if let Ok(cache) = STREAMING_STATE.read() {
        if let Some(state) = cache.get(listing_id) {
            state.cancelled.store(true, Ordering::Relaxed);
        }
    }
}
```

### Frontend implementation

#### New types

```typescript
// In types.ts

export interface StreamingListingStartResult {
    listingId: string
    status: 'loading' | 'ready' | 'cancelled' | { error: string }
}

export interface ListingProgressEvent {
    listingId: string
    loadedCount: number
}

export interface ListingCompleteEvent {
    listingId: string
    totalCount: number
    maxFilenameWidth: number | undefined
}

export interface ListingErrorEvent {
    listingId: string
    message: string
}
```

#### Updated tauri-commands.ts

```typescript
// listDirectoryStart now returns immediately with loading status
export async function listDirectoryStart(
    path: string,
    includeHidden: boolean,
    sortBy: SortColumn,
    sortOrder: SortOrder,
): Promise<StreamingListingStartResult> {
    return invoke<StreamingListingStartResult>('list_directory_start', { 
        path, includeHidden, sortBy, sortOrder 
    })
}

// New: cancel a loading listing
export async function cancelListing(listingId: string): Promise<void> {
    await invoke('cancel_listing', { listingId })
}
```

#### Updated LoadingIcon.svelte

```svelte
<script lang="ts">
    interface Props {
        loadedCount?: number
        showCancelHint?: boolean
    }
    
    const { loadedCount, showCancelHint = false }: Props = $props()
</script>

<div class="loading-container">
    <div class="loader"></div>
    {#if loadedCount !== undefined}
        <div class="loading-text">Loaded {loadedCount} files...</div>
    {:else}
        <div class="loading-text">Loading...</div>
    {/if}
    {#if showCancelHint}
        <div class="cancel-hint">Press ESC to cancel and go back</div>
    {/if}
</div>

<style>
    /* ... existing styles ... */
    
    .cancel-hint {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        margin-top: var(--spacing-sm);
    }
</style>
```

#### Updated FilePane.svelte

Key changes to `loadDirectory()`:

```typescript
// New state
let loadingCount = $state<number | undefined>(undefined)
let unlistenProgress: UnlistenFn | undefined
let unlistenComplete: UnlistenFn | undefined
let unlistenError: UnlistenFn | undefined
let unlistenCancelled: UnlistenFn | undefined

async function loadDirectory(path: string, selectName?: string) {
    const thisGeneration = ++loadGeneration
    
    // End previous listing
    if (listingId) {
        void listDirectoryEnd(listingId)
        listingId = ''
        lastSequence = 0
    }
    
    // Clean up previous event listeners
    unlistenProgress?.()
    unlistenComplete?.()
    unlistenError?.()
    unlistenCancelled?.()
    
    // Set loading state immediately
    loading = true
    loadingCount = undefined
    error = null
    totalCount = 0
    selectedEntry = null
    
    try {
        // Start streaming listing - returns immediately!
        const result = await listDirectoryStart(path, includeHidden, sortBy, sortOrder)
        
        if (thisGeneration !== loadGeneration) {
            // Cancelled - clean up
            void cancelListing(result.listingId)
            return
        }
        
        listingId = result.listingId
        
        // Subscribe to progress events
        unlistenProgress = await listen<ListingProgressEvent>('listing-progress', (event) => {
            if (event.payload.listingId === listingId && thisGeneration === loadGeneration) {
                loadingCount = event.payload.loadedCount
            }
        })
        
        // Subscribe to completion event
        unlistenComplete = await listen<ListingCompleteEvent>('listing-complete', async (event) => {
            if (event.payload.listingId === listingId && thisGeneration === loadGeneration) {
                totalCount = event.payload.totalCount
                maxFilenameWidth = event.payload.maxFilenameWidth
                
                // Handle selectName for cursor positioning
                if (selectName) {
                    const foundIndex = await findFileIndex(listingId, selectName, includeHidden)
                    const adjustedIndex = hasParent ? (foundIndex ?? -1) + 1 : (foundIndex ?? 0)
                    selectedIndex = adjustedIndex >= 0 ? adjustedIndex : 0
                } else {
                    selectedIndex = 0
                }
                
                loading = false
                loadingCount = undefined
                
                // NOW push to history (only on successful completion)
                onPathChange?.(path)
                
                // Fetch selected entry, sync to MCP, scroll
                void fetchSelectedEntry()
                void syncPaneStateToMcp()
                void tick().then(() => {
                    const listRef = viewMode === 'brief' ? briefListRef : fullListRef
                    listRef?.scrollToIndex(selectedIndex)
                })
            }
        })
        
        // Subscribe to error event
        unlistenError = await listen<ListingErrorEvent>('listing-error', (event) => {
            if (event.payload.listingId === listingId && thisGeneration === loadGeneration) {
                error = event.payload.message
                listingId = ''
                totalCount = 0
                loading = false
                loadingCount = undefined
            }
        })
        
        // Subscribe to cancelled event
        unlistenCancelled = await listen<{ listingId: string }>('listing-cancelled', (event) => {
            if (event.payload.listingId === listingId && thisGeneration === loadGeneration) {
                // Cancellation handled by handleLoadingEscape
                listingId = ''
                loading = false
                loadingCount = undefined
            }
        })
        
    } catch (e) {
        if (thisGeneration !== loadGeneration) return
        error = e instanceof Error ? e.message : String(e)
        listingId = ''
        totalCount = 0
        loading = false
        loadingCount = undefined
    }
}

// New: Handle ESC during loading
function handleLoadingEscape() {
    if (!loading || !listingId) return
    
    // Cancel the Rust-side operation
    void cancelListing(listingId)
    
    // Navigate back or to home
    // Note: This is called from DualPaneExplorer which has access to history
    onCancelLoading?.()
}

// Cleanup
onDestroy(() => {
    if (listingId) {
        void listDirectoryEnd(listingId)
    }
    unlisten?.()
    unlistenMenuAction?.()
    unlistenProgress?.()
    unlistenComplete?.()
    unlistenError?.()
    unlistenCancelled?.()
    if (syncPollInterval) {
        clearInterval(syncPollInterval)
    }
})
```

Update template:

```svelte
{:else if loading}
    <LoadingIcon loadedCount={loadingCount} showCancelHint={true} />
```

#### Updated DualPaneExplorer.svelte

Add new prop to FilePane for cancel callback:

```typescript
// In FilePane props
onCancelLoading?: () => void
```

Handle cancel in DualPaneExplorer:

```typescript
function handleLeftCancelLoading() {
    // Navigate back in history, or to home if empty
    if (canGoBack(leftHistory)) {
        void handleNavigationAction('back')
    } else {
        // Navigate to home
        leftPath = '~'
        leftVolumeId = DEFAULT_VOLUME_ID
        void saveAppStatus({ leftPath: '~', leftVolumeId: DEFAULT_VOLUME_ID })
    }
}

function handleRightCancelLoading() {
    if (canGoBack(rightHistory)) {
        void handleNavigationAction('back')  // This uses focusedPane, may need adjustment
    } else {
        rightPath = '~'
        rightVolumeId = DEFAULT_VOLUME_ID
        void saveAppStatus({ rightPath: '~', rightVolumeId: DEFAULT_VOLUME_ID })
    }
}
```

Handle ESC key globally (before delegating to pane):

```typescript
function handleKeyDown(e: KeyboardEvent) {
    // ESC during loading = cancel
    if (e.key === 'Escape') {
        const paneRef = focusedPane === 'left' ? leftPaneRef : rightPaneRef
        if (paneRef?.isLoading?.()) {
            e.preventDefault()
            if (focusedPane === 'left') {
                handleLeftCancelLoading()
            } else {
                handleRightCancelLoading()
            }
            return
        }
    }
    
    // ... rest of existing handling ...
}
```

#### History push timing fix

In `FilePane.svelte`, the `onPathChange` callback is currently called at the start of `loadDirectory()`. Move it to after `listing-complete`:

```typescript
// BEFORE: Called at start of loadDirectory
// onPathChange?.(path)

// AFTER: Called only on successful completion (inside listing-complete handler)
onPathChange?.(path)
```

This ensures cancelled navigations don't pollute the history.

## Events summary

| Event | Direction | Payload | When |
|-------|-----------|---------|------|
| `listing-progress` | Rust → Frontend | `{ listingId, loadedCount }` | Every 500ms during load |
| `listing-complete` | Rust → Frontend | `{ listingId, totalCount, maxFilenameWidth }` | After sort + cache |
| `listing-error` | Rust → Frontend | `{ listingId, message }` | On I/O error |
| `listing-cancelled` | Rust → Frontend | `{ listingId }` | After user cancels |

## Edge cases

### 1. App startup with slow path

- `DualPaneExplorer` shows its own `<LoadingIcon />` until `initialized = true`
- After initialization, each `FilePane` shows loading state with cancel hint
- User can press Tab to switch panes even if one is loading
- User can press ESC to cancel and go to home

### 2. Navigate away during load

- `loadGeneration` check in event handlers ignores stale events
- `cancelListing()` is called on the abandoned listing
- Rust stops iteration when cancellation flag is set

### 3. Both panes loading simultaneously

- Each pane has independent `listingId` and event subscriptions
- Tab works because IPC returns immediately
- ESC cancels the focused pane only

### 4. Network disconnect mid-load

- `read_dir` iteration will fail, error propagates
- `listing-error` event emitted
- Frontend shows error state

### 5. Very fast directories

- If loading completes before 500ms, no progress events emitted
- `listing-complete` goes directly from loading state
- User sees brief "Loading..." then file list

## Testing plan

### Unit tests (Rust)

1. `cancel_listing` sets the flag correctly
2. Progress events are emitted at correct intervals
3. Cancelled listing doesn't emit complete event
4. Sorting happens after all entries collected

### Unit tests (Svelte)

1. Loading state shows cancel hint
2. ESC during loading triggers cancel callback
3. Progress count updates on progress event
4. Loading ends on complete event

### E2E tests

1. Navigate to large directory, press ESC before complete → goes back
2. Navigate to large directory, press Tab → switches pane
3. Navigate to large directory, wait for complete → shows files
4. Start loading, navigate elsewhere → no stale events processed

### Manual testing

1. Test with `/Volumes/naspi/...` network path
2. Test with 50k file directory (use test data generator)
3. Test app restart with slow path saved
4. Test rapid Tab switching during load

## Implementation order

1. **Rust types and streaming state** – Add new types to `operations.rs`
2. **Async command + spawn_blocking** – Modify `list_directory_start`
3. **Progress emission loop** – Implement `read_directory_with_progress`
4. **Cancel command** – Add `cancel_listing` command
5. **Frontend types** – Update `types.ts` and `tauri-commands.ts`
6. **LoadingIcon update** – Add props for count and cancel hint
7. **FilePane streaming** – Subscribe to events, handle cancel
8. **DualPaneExplorer ESC** – Handle ESC for focused loading pane
9. **History timing fix** – Move `onPathChange` to completion
10. **Testing** – Unit, E2E, manual

## Open questions (none)

All decisions have been made.
