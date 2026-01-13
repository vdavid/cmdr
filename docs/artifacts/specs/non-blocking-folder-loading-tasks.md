# Non-blocking folder loading: Implementation tasks

**Spec**: [non-blocking-folder-loading.md](./non-blocking-folder-loading.md)

---

## 1. Rust: Types and streaming state

- [x] Add `ListingStatus` enum to `operations.rs` (`Loading`, `Ready`, `Cancelled`, `Error`)
- [x] Add `StreamingListingStartResult` struct (returns `listing_id` + `status`)
- [x] Add event payload structs: `ListingProgressEvent`, `ListingCompleteEvent`, `ListingErrorEvent`
- [x] Add `StreamingListingState` with `AtomicBool` cancellation flag
- [x] Add `STREAMING_STATE` static cache for in-progress listings

---

## 2. Rust: Streaming read loop

- [x] Create `read_directory_with_progress()` function in `operations.rs`
- [x] Check cancellation flag on each directory entry iteration
- [x] Emit `listing-progress` event every 500ms with `loaded_count`
- [x] After iteration complete: sort entries, cache listing, start watcher
- [x] Emit `listing-complete` event with `total_count` and `max_filename_width`
- [x] On error: emit `listing-error` event
- [x] On cancel: emit `listing-cancelled` event
- [x] Clean up `STREAMING_STATE` entry after task completes

---

## 3. Rust: Async command + cancel

- [x] Change `list_directory_start` in `commands/file_system.rs` to `async fn`
- [x] Add `app: tauri::AppHandle` parameter to access event emitter
- [x] Wrap blocking logic in `tokio::task::spawn_blocking()`
- [x] Return immediately with `{ listingId, status: 'loading' }`
- [x] Add `cancel_listing(listing_id: String)` command
- [x] Implement `ops_cancel_listing()` that sets cancellation flag
- [x] Register `cancel_listing` in the Tauri command list in `lib.rs`

---

## 4. Rust: Unit tests

- [x] Test: `cancel_listing` sets cancellation flag correctly
- [ ] Test: Cancelled listing does not emit `listing-complete` (requires Tauri app mock)
- [ ] Test: Error during read emits `listing-error` (requires Tauri app mock)
- [x] Test: Entries are sorted before caching

---

## 5. Frontend: Types and commands

- [x] Add `StreamingListingStartResult` type to `types.ts`
- [x] Add `ListingProgressEvent` type to `types.ts`
- [x] Add `ListingCompleteEvent` type to `types.ts`
- [x] Add `ListingErrorEvent` type to `types.ts`
- [x] Update `listDirectoryStart()` return type in `tauri-commands.ts`
- [x] Add `cancelListing(listingId: string)` function to `tauri-commands.ts`

---

## 6. Frontend: LoadingIcon component

- [x] Add `loadedCount?: number` prop
- [x] Add `showCancelHint?: boolean` prop
- [x] Show "Loaded N files..." when `loadedCount` is set
- [x] Show "Press ESC to cancel and go back" when `showCancelHint` is true
- [x] Style the cancel hint (tertiary color, small font)

---

## 7. Frontend: FilePane streaming logic

- [x] Add `loadingCount` state variable
- [x] Add event listener refs: `unlistenProgress`, `unlistenComplete`, `unlistenError`, `unlistenCancelled`
- [x] Subscribe to `listing-progress` → update `loadingCount`
- [x] Subscribe to `listing-complete` → set `totalCount`, `maxFilenameWidth`, `loading = false`, call `onPathChange`
- [x] Subscribe to `listing-error` → set `error`, `loading = false`
- [x] Subscribe to `listing-cancelled` → reset state
- [x] Cancel abandoned listing when `loadGeneration` changes
- [x] Add `onCancelLoading?: () => void` prop
- [x] Add `isLoading()` export method for DualPaneExplorer to check
- [x] Clean up all event listeners in `onDestroy`
- [x] Pass `loadedCount` and `showCancelHint={true}` to `<LoadingIcon />`

---

## 8. Frontend: DualPaneExplorer ESC handling

- [x] Add `handleLeftCancelLoading()` function (back in history, or home if empty)
- [x] Add `handleRightCancelLoading()` function (back in history, or home if empty)
- [x] Pass `onCancelLoading` prop to both FilePane components
- [x] Handle ESC key in `handleKeyDown`: check if focused pane is loading, call cancel handler

---

## 9. Frontend: History timing fix

- [x] Remove `onPathChange?.(path)` from start of `loadDirectory()`
- [x] Move `onPathChange?.(path)` to inside `listing-complete` handler
- [x] Verify back/forward still works after a successful navigation

---

## 10. Frontend: Unit tests (Vitest)

- [x] Test: `LoadingIcon` shows count when `loadedCount` prop is set
- [x] Test: `LoadingIcon` shows cancel hint when `showCancelHint` is true
- [x] Test: `LoadingIcon` shows default "Loading..." when no props
- [ ] Test: FilePane mock – verify event subscriptions are created on load start (requires Tauri mock)
- [ ] Test: FilePane mock – verify `cancelListing` called when navigating away during load (requires Tauri mock)

---

## 11. Integration tests

### Backend (Rust – cargo nextest)

- [x] Test: Streaming state lifecycle (create, cancel, cleanup)
- [x] Test: Multiple concurrent streaming states
- [ ] Test: Full flow with mock volume – start, progress events emitted, complete event emitted (requires Tauri app mock)
- [ ] Test: Cancellation mid-stream – verify no complete event, cleanup occurs (requires Tauri app mock)
- [ ] Test: Error handling – verify error event emitted, state cleaned up (requires Tauri app mock)

### Frontend (Vitest with mocked Tauri)

- [x] Test: Streaming event handling logic (progress, complete, error, cancelled)
- [x] Test: Load generation tracking (stale loads ignored)
- [x] Test: Cancel loading behavior
- [x] Test: History timing (onPathChange only on completion)
- [ ] Test: FilePane receives progress events → updates loading count (requires Tauri mock)
- [ ] Test: FilePane receives complete event → renders file list (requires Tauri mock)
- [ ] Test: FilePane receives error event → shows error state (requires Tauri mock)
- [ ] Test: ESC during loading → calls `cancelListing` and `onCancelLoading` (requires Tauri mock)

---

## 12. Manual testing

- [x] Test with slow network path (NAS/SMB mount)
- [x] Test with large local directory (50k files – use test data generator)
- [ ] Test app restart with slow path saved in `app_status.json`
- [x] Test Tab switching while one pane is loading
- [x] Test ESC when history is empty → goes to home (~)
- [x] Test ESC when history exists → goes back to previous folder

---

## 13. Documentation

- [x] Create `docs/features/file-loading.md`:
  - Overview of streaming loading architecture
  - Event flow (text diagram)
  - Cancellation behavior (ESC → back or home)
  - Progress display
- [x] Add inline code comments in `operations.rs` explaining streaming flow

---

## 14. Cleanup and verification

- [x] Remove any dead code from old sync implementation
- [x] Run `./scripts/check.sh` and fix all issues
- [x] Verify no regressions in existing tests
