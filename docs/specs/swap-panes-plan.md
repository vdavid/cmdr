# Swap panes (⌘U)

## Context

Orthodox file managers (Total Commander, Far, Midnight Commander, etc.) universally offer a "swap panels"
feature at Ctrl+U / ⌘U. It swaps the directories shown in the left and right panes. Cmdr needs this too.

The feature name is **"Swap panes"** (sentence case, matching Cmdr's "panes" terminology).

**Approach A (chosen):** Swap everything — path, volume, history, sort, view mode, cursor, selection.
This avoids re-sorting, keeps cursor/selection indices valid, and matches the mental model of
"left became right, right became left."

**Key architectural insight:** The Rust backend has no concept of left/right — it's all keyed by `listing_id`
(UUID). We swap listing_id ownership on the frontend only. The cache, watchers, and diff events all remain
consistent because we don't touch the backend mapping. Zero disk I/O, zero backend changes to core logic.

## Files to modify

| File | Change |
|------|--------|
| `apps/desktop/src/lib/file-explorer/pane/FilePane.svelte` | Add `adoptListing()` and `getSwapState()` exports |
| `apps/desktop/src/lib/file-explorer/pane/DualPaneExplorer.svelte` | Add `swapPanes()` export |
| `apps/desktop/src-tauri/src/menu.rs` | Add `SWAP_PANES_ID` menu item with ⌘U |
| `apps/desktop/src-tauri/src/lib.rs` | Handle `SWAP_PANES_ID` menu click → emit event |
| `apps/desktop/src/routes/(main)/+page.svelte` | Listen for `swap-panes` event → call `explorerRef.swapPanes()` |
| `apps/desktop/src-tauri/src/mcp/tools.rs` | Add `swap_panes` tool definition |
| `apps/desktop/src-tauri/src/mcp/executor.rs` | Add `swap_panes` executor (emit event, swap MCP pane state) |
| `apps/desktop/src-tauri/src/mcp/tests.rs` | Update tool count assertions |

## Implementation

### 1. FilePane: new exports

**`getSwapState()`** — Returns everything the other pane needs to adopt this listing:

```typescript
export function getSwapState(): SwapState {
    return {
        listingId,
        totalCount,
        maxFilenameWidth,
        cursorIndex,
        selectedIndices: selection.getSelectedIndices(),
        lastSequence,
    }
}
```

**`adoptListing(state: SwapState)`** — Takes over another pane's listing without any backend calls:

```typescript
export function adoptListing(state: SwapState): void {
    // Cancel any in-flight loads (safety — should be guarded at DualPaneExplorer level)
    loadGeneration++

    // Adopt the listing identity
    listingId = state.listingId
    totalCount = state.totalCount
    maxFilenameWidth = state.maxFilenameWidth
    lastSequence = state.lastSequence

    // Restore cursor and selection
    cursorIndex = state.cursorIndex
    selection.setSelectedIndices(state.selectedIndices)

    // Force virtual list to re-fetch visible range from (now-swapped) cache
    cacheGeneration++

    // Clear loading/error state
    loading = false
    error = null

    // Scroll to cursor position
    void tick().then(() => {
        const listRef = viewMode === 'brief' ? briefListRef : fullListRef
        listRef?.scrollToIndex(cursorIndex)
    })
}
```

**Why this works with event listeners:**
- The `directory-diff` listener is inside a `$effect` that filters by
  `diff.listingId !== listingId`. Since `listingId` is a `$state` variable, reading it in the callback
  always returns the current value. After `adoptListing` sets `listingId` to the new value, the existing
  listener automatically filters for the adopted listing's events. No re-subscription needed.
- Same applies to the `directory-deleted` listener.

**Type** (add near other FilePane types):
```typescript
interface SwapState {
    listingId: string
    totalCount: number
    maxFilenameWidth: number | undefined
    cursorIndex: number
    selectedIndices: number[]
    lastSequence: number
}
```

### 2. DualPaneExplorer: `swapPanes()` export

```typescript
export function swapPanes(): void {
    const leftRef = getPaneRef('left')
    const rightRef = getPaneRef('right')

    // Guard: don't swap during loading, active copy, or if refs missing
    if (!leftRef || !rightRef || leftRef.isLoading() || rightRef.isLoading()) return
    if (isCopyInProgress) return  // Check if copy/move dialog is active

    // 1. Snapshot both panes' listing state
    const leftSwap = leftRef.getSwapState()
    const rightSwap = rightRef.getSwapState()

    // 2. Swap DualPaneExplorer state variables
    ;[leftPath, rightPath] = [rightPath, leftPath]
    ;[leftVolumeId, rightVolumeId] = [rightVolumeId, leftVolumeId]
    ;[leftHistory, rightHistory] = [rightHistory, leftHistory]
    ;[leftViewMode, rightViewMode] = [rightViewMode, leftViewMode]
    ;[leftSortBy, rightSortBy] = [rightSortBy, leftSortBy]
    ;[leftSortOrder, rightSortOrder] = [rightSortOrder, leftSortOrder]

    // 3. Each pane adopts the other's listing (no backend calls)
    leftRef.adoptListing(rightSwap)
    rightRef.adoptListing(leftSwap)

    // 4. Persist
    void saveAppStatus({
        leftPath,
        rightPath,
        leftVolumeId,
        rightVolumeId,
        leftViewMode,
        rightViewMode,
        leftSortBy,
        rightSortBy,
    })

    containerElement?.focus()
}
```

### 3. Rust menu: add ⌘U shortcut

In `menu.rs`, add constant and menu item alongside switch pane:
```rust
pub const SWAP_PANES_ID: &str = "swap_panes";
```

In `build_menu`, after the existing switch pane item:
```rust
let swap_panes_item = MenuItem::with_id(app, SWAP_PANES_ID, "Swap panes", true, Some("Cmd+U"))?;
submenu.append(&swap_panes_item)?;
```

### 4. Rust lib.rs: handle menu event

In the menu event handler, alongside the `SWITCH_PANE_ID` case:
```rust
} else if id == SWAP_PANES_ID {
    let _ = app.emit_to("main", "swap-panes", ());
}
```

### 5. +page.svelte: listen for event

Alongside the existing `switch-pane` listener:
```typescript
unlistenSwapPanes = await safeListenTauri('swap-panes', () => {
    explorerRef?.swapPanes()
})
```

### 6. MCP: add swap_panes tool

In `tools.rs`, add to `get_app_tools()`:
```rust
Tool::no_params("swap_panes", "Swap left and right pane directories, view modes, sort orders, and selections"),
```

In `executor.rs`, add handler that:
1. Swaps MCP `PaneStateStore` left/right state immediately
2. Emits `swap-panes` event to frontend

In `tests.rs`, update the app tools count and add `"swap_panes"` to expected list.

## Edge cases

| Scenario | Handling |
|----------|----------|
| Either pane loading | Guard: `isLoading()` check, skip swap |
| Copy/move in progress | Guard: check dialog state, skip swap |
| Both panes same path | Works (visually a no-op, listing_ids swap) |
| Network/MTP volume | Works — volumeId follows the listing |
| Swap → Swap round-trip | Perfect no-op (all state round-trips) |
| Pane width | Not swapped (layout property) |
| Focused pane | Not swapped (user stays on same side) |
| In-flight diff event arrives with old listing_id | Filtered out by `diff.listingId !== listingId` check |
| `lastSequence` after swap | Transferred with the listing; no events will be skipped or replayed |

## Verification

### Automated tests
- **Vitest**: Test `swapPanes()` in DualPaneExplorer (mock FilePane refs, verify state swaps correctly)
- **Vitest**: Test `adoptListing()` / `getSwapState()` round-trip
- **Rust**: Test updated MCP tool count in `mcp/tests.rs`
- **Rust**: Menu constant compiles, `cargo nextest run`

### Manual testing (MCP)
1. Run `pnpm dev`
2. Navigate left to a large directory, right to a different one
3. Select some files and position cursor in both panes
4. Press ⌘U — verify panes swap instantly
5. Verify cursor position and selection preserved in both panes
6. Verify back/forward navigation works correctly after swap
7. Press ⌘U again — verify perfect round-trip (everything restored)
8. Test with one pane on Network volume
9. Test during loading (should be blocked)
10. Test MCP `swap_panes` tool via Tauri MCP server

### Check script
Run `./scripts/check.sh --svelte --check clippy --check rustfmt --check rust-tests` to verify no
regressions.

## Task list

### Milestone 1: Core swap logic
- [ ] Add `SwapState` type and `getSwapState()` / `adoptListing()` exports to FilePane.svelte
- [ ] Add `swapPanes()` export to DualPaneExplorer.svelte
- [ ] Wire up ⌘U: menu item in menu.rs, event handler in lib.rs, listener in +page.svelte

### Milestone 2: MCP
- [ ] Add `swap_panes` tool definition in tools.rs
- [ ] Add executor in executor.rs (swap PaneStateStore + emit event)
- [ ] Update test assertions in tests.rs

### Milestone 3: Testing and polish
- [ ] Write Vitest tests for swap logic
- [ ] Manual testing with MCP (large dirs, network volumes, round-trip)
- [ ] Run `./scripts/check.sh --svelte --check clippy --check rustfmt --check rust-tests`
- [ ] Add `adoptListing`/`getSwapState` to coverage allowlist if needed
