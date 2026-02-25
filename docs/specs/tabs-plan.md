# Tabs feature plan

## Context

Adding Total Commander-style tabs: each pane side (left/right) gets an independent tab bar. Tabs cold-load their directory on switch-to (no warm cache — keeps things simple). Cursor position and navigation history are snapshotted on switch-away and restored on switch-to, so directory changes while backgrounded don't corrupt state. Selection is cleared on tab switch (intentional v1 simplification — see key decisions).

## Key decisions

- **Tab bar always visible** — even with a single tab. Avoids layout shift on first `⌘T`, and shows pin state for single-tab users. 28px is a small cost and matches the convention of every major tabbed app (Chrome, VS Code, Terminal.app). Not configurable in v1.
- **`⌘W` with one tab closes the window** — matches macOS convention. `closeActiveTab()` returns `'last-tab'` signal; `+page.svelte` calls `getCurrentWindow().close()`. DualPaneExplorer stays free of window-management concerns. Even if the last tab is pinned, `⌘W` closes the window without confirmation — closing the window is the expected "last tab" behavior, the user is closing the app, not deciding about a pin.
- **Sort order is per-tab (collateral cleanup)** — the global `columnSortOrders` store (per-column sort direction memory) is intentionally removed. Each tab owns its `sortBy` + `sortOrder`. This is simpler and more intuitive (tabs are independent workspaces). Removing per-column memory is an explicit simplification we want — it was minor convenience that conflicts with the per-tab model.
- **Cold load on tab switch** — switching to a background tab triggers a fresh directory load. No warm listing cache is kept in Rust. This drastically simplifies the implementation (no listing lifecycle management, no stale-cache eviction, no memory concerns from backgrounded listings). Most local directories load in single-digit milliseconds; large directories and network shares are slower, but users expect that. Inactive tabs don't load anything in the background — only the active tab's content is loaded and displayed in the pane, where existing loading state indicators apply. If a tab's directory was deleted or became unreachable while backgrounded, the cold load fails gracefully via existing FilePane error handling (falls back to parent or shows error) — no special tab-level handling needed.
- **Why `{#key}` (destroy/recreate) and not reuse a single FilePane** — Reusing the FilePane instance and calling setters on tab switch creates a fragile maintenance coupling: every new piece of FilePane internal state (dialogs, rename UI, progress counters, notifications) must be explicitly reset on tab switch, or it leaks across tabs. Anyone modifying FilePane in the future would need to "think about tabs" with every change. `{#key}` avoids this by guaranteeing a clean slate — the DOM and all internal state are destroyed and recreated. The cost is ~1–2ms of DOM teardown/setup plus a cold directory load per switch. **Focus is not a concern**: DualPaneExplorer's container `div` holds DOM focus (not FilePane), and keydown events are forwarded to the active pane ref. After `{#key}` recreation, `bind:this` automatically updates the pane ref when the new FilePane mounts — no focus reclaiming needed.
- **Debounce rapid `⌃Tab` cycling (leading-edge, `cycleTab` only)** — when holding `⌃Tab`, each intermediate tab triggers a `{#key}` destroy/create cycle plus a cold directory load that gets immediately cancelled. Use a leading-edge debounce (~50ms) on the `activeTabId` update **only in `cycleTab`** so that single tab switches feel instant (first switch fires immediately) while rapid cycling only cold-loads the tab the user settles on. Direct tab switches (clicking a tab, `switchTab`) fire immediately with no debounce — adding 50ms lag to a direct click would feel sluggish. This avoids IPC noise (many listing starts + immediate cancels) and prevents intermediate tabs from flashing partial content.
- **`⌘T` creates a new tab (instant via clone trick)** — To the user, `⌘T` opens a new tab to the right of the current tab and selects it instantly. Under the hood, we insert a *clone* of the active tab to the **left** and keep the current tab selected. Since `activeTabId` doesn't change, no `{#key}` fires, no reload — it's instant. The user perceives "new tab appeared to the right, already loaded!" because the rightmost of the two identical tabs is the one that stays active. The clone (sitting to the left) cold-loads whenever the user eventually switches to it. The clone gets its `NavigationHistory` via `$state.snapshot()` (Svelte 5's built-in way to get a plain deep copy from reactive proxies — `structuredClone` doesn't work on `$state` proxies). Back/forward work naturally in both tabs after they diverge.
- **Tab switching blocked during file operations** — if a file operation dialog is open (copy, move, mkdir, delete), tab switches via clicks and keyboard are ignored. This is already enforced by the existing modal dialog guard that blocks input while dialogs are open. No new code needed — just verify the guard covers tab-related shortcuts (`⌘T`, `⌘W`, `⌃Tab`).
- **Rename-in-progress cancelled on tab switch** — if the user is renaming a file and switches tabs, the `{#key}` destroy cancels the rename naturally (no commit). No special handling needed.
- **Selection cleared on tab switch (v1 limitation)** — only cursor position is saved/restored (by filename). This avoids the need for a batch IPC call to resolve 200k+ selection indices to filenames. Simple and predictable. Known gap: a future version could save small selections (< 1000 files) as `Set<filename>` and restore lazily on switch-back.
- **Tab tooltip** — each tab shows a tooltip with the full directory path on hover, since tab labels (last path segment) frequently truncate at narrow widths.
- **`closeOtherTabs` skips pinned tabs** — only unpinned tabs are closed. Pinned tabs "stick." No confirmation dialog needed. The kept tab (the one right-clicked) becomes the active tab if it wasn't already.
- **Swap panes (`⌘U`) swaps active tab content only** — the tab lists stay on their respective sides. Only the two active tabs' content (path, volume, sort, history, view mode) is swapped via the existing `getSwapState/adoptListing` mechanism. Active tab IDs don't change, so `{#key}` doesn't fire.
- **Max tabs cap: 10 per pane** — hard cap, no user setting. When the user tries to create an 11th tab, show a brief toast notification ("Tab limit reached") and ignore the action. `addTab` returns a success/failure signal so the caller can trigger the toast.
- **`⌘W` on a pinned active tab (with other tabs present)** — shows the pinned-tab confirmation dialog before closing. If the user cancels, no-op.
- **Middle-click on pinned tab** — unpins and closes without confirmation. Middle-click is a deliberate, quick gesture — interrupting it with a dialog defeats the purpose.

### Consciously deferred features

These are out of scope for v1 but worth noting:

- **Tab reordering** (drag to reorder) — data model supports it (array position = order), just needs UI work.
- **Drag file onto tab** to copy/move to that tab's directory.
- **Duplicate tab** (`⌘D` or context menu).
- **`⌘1`–`⌘9`** for direct tab switching by position.
- **Preserve selection across tab switches** for small selections (< 1000 files).

## Data model

**New file: `apps/desktop/src/lib/file-explorer/tabs/tab-types.ts`**

```typescript
export type TabId = string  // crypto.randomUUID()

/** Full runtime state for one tab */
export interface TabState {
    id: TabId
    path: string
    volumeId: string
    history: NavigationHistory
    sortBy: SortColumn
    sortOrder: SortOrder
    viewMode: ViewMode
    pinned: boolean
    cursorFilename: string | null  // saved on switch-away, restored on switch-to
}

/** Stored in app-status.json per tab */
export interface PersistedTab {
    id: TabId
    path: string
    volumeId: string
    sortBy: SortColumn
    sortOrder: SortOrder
    viewMode: ViewMode
    pinned: boolean
}

/** Stored in app-status.json per pane side */
export interface PersistedPaneTabs {
    tabs: PersistedTab[]
    activeTabId: TabId
}
```

History and cursor are session-only (not persisted) — matches current behavior. Selection is cleared on every tab switch.

## Tab state manager

**New file: `apps/desktop/src/lib/file-explorer/tabs/tab-state-manager.svelte.ts`**

Reactive module (uses `$state()`) encapsulating all tab operations for one pane side:

- `createTabManager(initialTab)` — initialize with one tab
- `getActiveTab(mgr)` — current tab
- `addTab(mgr, beforeTabId, path, volumeId, sort, viewMode, history)` — insert clone to the left of `beforeTabId`; returns `false` if at 10 (hard cap, caller shows toast). The `history` param receives a `$state.snapshot()` of the active tab's `NavigationHistory` (see clone trick decision — `structuredClone` doesn't work on `$state` proxies).
- `closeTab(mgr, tabId)` — returns `{ closed: true, newActiveTabId }` or `{ closed: false }` when it's the last tab
- `closeOtherTabs(mgr, tabId)` — close all unpinned tabs except the given one. Pinned tabs are kept. The given tab becomes active if it wasn't already.
- `switchTab(mgr, tabId, cursorFilename)` — stores cursor filename on old tab, activates new
- `pinTab/unpinTab(mgr, tabId)` — toggle pin state
- `cycleTab(mgr, direction, cursorFilename)` — next/prev tab wrapping around. Uses leading-edge debounce (~50ms) so rapid `⌃Tab` cycling only commits the final tab (direct `switchTab` calls are not debounced)
- `getAllTabs(mgr)` / `getTabCount(mgr)` — accessors

Close-active logic: activate next tab to the right, or left neighbor if rightmost. With a single tab, `closeTab` returns `{ closed: false }` — the caller (`+page.svelte`) decides to close the window.

Hard cap: 10 tabs per pane, no user setting. Constant `MAX_TABS_PER_PANE = 10` in `tab-state-manager`.

Fully unit-testable — pure state operations.

## DualPaneExplorer refactoring

**File: `apps/desktop/src/lib/file-explorer/pane/DualPaneExplorer.svelte`**

Replace scalar per-pane state (`leftPath`, `rightPath`, `leftVolumeId`, etc.) with two tab managers:

```typescript
let leftTabMgr = $state<TabManager>(...)
let rightTabMgr = $state<TabManager>(...)
```

Derive current values from active tab:
```typescript
const leftPath = $derived(getActiveTab(leftTabMgr).path)
// ... etc for volumeId, history, sortBy, sortOrder, viewMode
```

Pane accessors (`getPanePath`, `setPanePath`, etc.) updated to read/write the active tab — rest of DualPaneExplorer logic stays identical.

Swap panes (`⌘U`): swaps only the active tabs' content. Tab lists (and non-active tabs) stay on their respective sides. Uses the existing `getSwapState/adoptListing` mechanism — since active tab IDs don't change, `{#key}` doesn't fire. After the swap, update both active tabs' `TabState` fields (path, volumeId, sort, history, viewMode) so persistence stays correct.

Force FilePane recreation on tab switch via Svelte `{#key}`:
```svelte
{#key getActiveTab(leftTabMgr).id}
    <FilePane bind:this={leftPaneRef} ... />
{/key}
```

When `{#key}` fires (tab switch), the old FilePane is destroyed with normal cleanup (`cancelListing`/`listDirectoryEnd` run as usual). The new FilePane cold-loads the new tab's directory.

New exports for MCP/commands: `newTab()` (returns `boolean` — `false` if at cap), `closeActiveTab()` (returns `'closed' | 'last-tab'`), `cycleTab(direction)`.

### Remove `columnSortOrders` (collateral cleanup)

The global `columnSortOrders` store in `app-status-store.ts` is intentionally removed in this milestone. Per-column sort direction memory conflicts with the per-tab model and adds complexity we don't want. This is an explicit simplification, not a side effect:

- `getColumnSortOrder()` and `saveColumnSortOrder()` are deleted from `app-status-store.ts`
- The `columnSortOrders` store key is removed
- `handleSortChange()` in DualPaneExplorer no longer reads remembered per-column order; when switching columns, it uses the `defaultSortOrders` map from `types.ts` (already exists)
- Sort order is persisted per-tab in `PersistedTab.sortOrder`
- Migration: on first load, ignore `columnSortOrders` key entirely; each migrated tab gets `defaultSortOrders[sortBy]` as its initial sort order

**Note (sort persistence gap):** M2 removes `columnSortOrders` but per-tab persistence is only added in M5. Between these milestones, sort preferences are not persisted — every restart resets to defaults. This is a dev-time annoyance only (all milestones ship together) and is intentional to avoid dead code in M2.

## Tab switching flow

**Switch away** from active tab:
1. If a rename is in progress, it's cancelled naturally by `{#key}` destroy (no commit).
2. Read cursor filename from FilePane's cached `entryUnderCursor` (synchronous, no IPC call). This must happen before `activeTabId` changes, since `{#key}` destroys the FilePane reactively in the same microtask.
3. Store cursor filename on the departing tab's `TabState.cursorFilename`.
4. Selection is cleared (not preserved across tabs — v1 limitation).

**Switch to** new tab:
1. Update `activeTabId` on tab manager.
2. `{#key}` destroys old FilePane (normal cleanup — listing freed), creates new one with new tab's props.
3. FilePane cold-loads the directory from scratch.
4. After listing is ready, restore cursor: resolve `cursorFilename` -> index via `findFileIndex`, apply. If the file no longer exists, cursor goes to 0.

**FilePane changes needed**:
- **Use `onPathChange` for cursor restoration** — FilePane already fires `onPathChange` from inside `handleListingComplete` after a directory load finishes. Since `{#key}` creates a fresh FilePane (no "old" path), `onPathChange` fires even for the same directory. Use this callback to resolve `cursorFilename` → index and restore cursor position. No new callback prop needed.

## TabBar component

**New file: `apps/desktop/src/lib/file-explorer/tabs/TabBar.svelte`**

Sits above FilePane in each pane-wrapper div. Props: `tabs`, `activeTabId`, `paneId`, callbacks for switch/close/contextmenu.

Visual design:
- Height defined as `--tab-bar-height: 28px` CSS variable in `app.css`
- Uses CSS variables from `app.css` (`--color-bg-primary/secondary`, `--color-border`, `--spacing-*`)
- Each tab shows folder name (last path segment). Full path in `title` tooltip. Pinned tabs show a small pin icon.
- Active tab: primary bg, blends with pane below. Inactive: secondary bg.
- Chrome-style shrinking: `min-width: 32px`, `max-width: 180px`, flex-shrink evenly. Tabs get narrower as more are added; 10 tabs always fit.
- Close (x) button on each non-pinned tab; hidden when tab width < 80px
- Middle-click on a tab closes it (browser/editor convention). On pinned tabs, middle-click **unpins and closes** without confirmation — it's a deliberate, quick gesture.
- "+" button at right end. Disabled (visually dimmed) when at max tabs.
- **Always visible** — even with a single tab. Single tab shows folder name, no close button (since `⌘W` closes the window).
- `role="tablist"` / `role="tab"` / `aria-selected` for accessibility

## Context menu (pin/unpin/close others)

Follow existing native context menu pattern:

**Rust** (`menu.rs`): `build_tab_context_menu(app, is_pinned, can_close, has_other_unpinned_tabs)` — menu with "Pin tab"/"Unpin tab", separator, "Close other tabs" (disabled if no other unpinned tabs), "Close tab" (disabled if last tab).

**Rust** (`commands/ui.rs`): `show_tab_context_menu` Tauri command — builds menu, shows popup, returns selected action string.

**Frontend** (`tauri-commands/tab.ts`): `showTabContextMenu(pane, tabId, isPinned, canClose, hasOtherUnpinnedTabs)` wrapper. TabBar calls this on `oncontextmenu`, dispatches result to tab manager.

## Close pinned tab confirmation

Closing a pinned tab requires confirmation **only via `⌘W` and the context menu "Close tab" action** — these are deliberate actions where a safety net helps. Middle-click on a pinned tab unpins and closes without confirmation (quick gesture, no interruption).

Use existing `confirmDialog()` from `$lib/utils/confirm-dialog.ts` (native Tauri dialog):

```typescript
if (tab.pinned) {
    const ok = await confirmDialog('This tab is pinned. Close it anyway?', 'Close pinned tab')
    if (!ok) return
}
```

Exception: if the pinned tab is the **last tab**, `⌘W` closes the window without confirmation — that's the expected "last tab" behavior.

## Persistence

**File: `apps/desktop/src/lib/app-status-store.ts`**

New store keys: `leftTabs: PersistedPaneTabs`, `rightTabs: PersistedPaneTabs`.

New functions:
- `loadPaneTabs(side, pathExists)` — `PersistedPaneTabs` with path validation/fallback
- `savePaneTabs(side, tabs)` — persists tab array + activeTabId

Removed: `getColumnSortOrder()`, `saveColumnSortOrder()`, `columnSortOrders` store key (removed in M2; persistence migration also ignores these keys).

Migration: if `leftTabs` is missing, create single-tab state from old `leftPath`/`leftVolumeId`/`leftSortBy`/`leftViewMode`. Sort order defaults to `defaultSortOrders[sortBy]`. Old scalar keys and `columnSortOrders` are ignored after migration. Add a `// TODO(2026-04-01): remove migration` comment.

Save triggers: tab create/close/switch/pin-change, sort change, and existing path/viewMode change paths.

**Debounce `saveAppStatus()`** — currently every `saveAppStatus()` call writes to disk immediately. With tabs adding more save triggers (especially rapid `⌃Tab` cycling), debounce all `saveAppStatus()` calls with a trailing-edge 200ms delay (so the *final* state is what gets persisted, not the first). This is a global change to `app-status-store.ts`, not tab-specific — all existing callers benefit.

## Commands and shortcuts

**File: `src/lib/commands/command-registry.ts`** — add:
- `tab.new` — `⌘T`, "New tab", scope: `Main window`, `showInPalette: true`
- `tab.close` — `⌘W`, "Close tab", scope: `Main window`, `showInPalette: true`. With one tab: closes window.
- `tab.next` — `⌃Tab`, "Next tab", scope: `Main window`, `showInPalette: true`
- `tab.prev` — `⌃⇧Tab`, "Previous tab", scope: `Main window`, `showInPalette: true`

All four commands are user-configurable in Settings > Shortcuts (automatic — the command registry feeds the shortcuts UI, no extra work needed).

**`⌘W` strategy: replace native "Close Window" in `menu.rs`.** The app menu is already fully custom-built in `menu.rs` (starting from `Menu::default(app)` and modifying each submenu). The default File submenu includes a predefined "Close Window" item with `⌘W`. Remove this predefined item and add a custom "Close tab" `MenuItem` with `⌘W` instead. This is the same pattern already used for the app menu's About item (remove predefined, insert custom). No Rust-level interception or `on_window_event(CloseRequested)` needed — the menu event handler emits a frontend event, and the JS command system handles the close-tab-vs-close-window routing.

**Native menu items in `menu.rs`** — add to the File submenu (after the existing custom items, before the remaining default items):
- "New tab" (`⌘T`) — emits `new-tab` event to frontend
- "Close tab" (`⌘W`) — replaces the predefined "Close Window" item, emits `close-tab` event to frontend

Add corresponding constants: `NEW_TAB_ID`, `CLOSE_TAB_ID`.

**File: `+page.svelte`** — add Tauri event listeners for `new-tab` and `close-tab` menu events (same pattern as existing `switch-pane`, `swap-panes` listeners in `setupMenuListeners`), routing to `handleCommandExecute`:
- `tab.close`: If the active tab is pinned **and** it's not the last tab, show `confirmDialog` first. Then call `explorerRef.closeActiveTab()`. If it returns `'last-tab'`, call `getCurrentWindow().close()` (no confirmation even if pinned — closing the window is the natural "last tab" behavior). This keeps window-management in `+page.svelte` and tab logic in DualPaneExplorer.
- `tab.new`: call `explorerRef.newTab()`. If it returns `false` (cap reached), show a brief toast ("Tab limit reached").
- Others delegate to `.cycleTab('next'|'prev')`.

## MCP changes

**Backward compatible**: `left`/`right` in status resource continues to show the active tab's state.

**Additive**:
1. Add `tabs: Vec<TabInfo>` to `PaneState` in `pane_state.rs`:
   ```rust
   pub struct TabInfo { pub id: String, pub path: String, pub pinned: bool, pub active: bool }
   ```
2. YAML resource gets a `tabs:` section per pane showing all tabs with `[active]`/`[pinned]` markers
3. New tool `activate_tab` in `tools.rs`: takes `pane` + `tabId`, emits Tauri event, DualPaneExplorer handles it
4. Frontend syncs tab list to backend via new `update_pane_tabs(pane, tabs)` Tauri command, called from a debounced `$effect` (~100ms) in DualPaneExplorer watching **structural** tab changes only (tab count, tab order, active tab ID, pinned state) — not path/sort/viewMode, which are already synced via existing MCP pane state mechanisms. Prevents IPC spam during rapid operations

## Files to create

| File | Purpose |
|------|---------|
| `src/lib/file-explorer/tabs/tab-types.ts` | Types |
| `src/lib/file-explorer/tabs/tab-state-manager.svelte.ts` | Reactive tab state logic |
| `src/lib/file-explorer/tabs/tab-state-manager.test.ts` | Unit tests |
| `src/lib/file-explorer/tabs/TabBar.svelte` | Tab bar UI component |
| `src/lib/file-explorer/tabs/CLAUDE.md` | Module docs |
| `src/lib/tauri-commands/tab.ts` | Tab context menu Tauri command wrapper |

## Files to modify

| File | Change |
|------|--------|
| `DualPaneExplorer.svelte` | Replace scalar state with tab managers, add `{#key}` tab switching, new exports |
| `FilePane.svelte` | No changes expected (cursor restoration uses existing `onPathChange` callback) |
| `app.css` | Add `--tab-bar-height: 28px` variable |
| `app-status-store.ts` | Add tab persistence + migration, remove `columnSortOrders` |
| `src/lib/commands/command-registry.ts` | Add 4 tab commands |
| `+page.svelte` | Add command handler cases (including `⌘W` -> pinned check -> `'last-tab'` -> close window) |
| `menu.rs` | Remove predefined "Close Window", add "New tab" + "Close tab" menu items, add `build_tab_context_menu` |
| `commands/ui.rs` | Add `show_tab_context_menu` command |
| `lib.rs` | Register new command |
| `mcp/pane_state.rs` | Add `TabInfo` struct and field |
| `mcp/resources.rs` | Render tabs in YAML |
| `mcp/tools.rs` | Add `activate_tab` tool |
| `mcp/executor.rs` | Handle `activate_tab` execution |
| `tauri-commands/index.ts` | Re-export from `tab.ts` |
| `coverage-allowlist.json` | Add new files if needed |
| `file-explorer/CLAUDE.md` | Document tabs |

## Verification

- Manual: `pnpm dev`, press `⌘T` to create tabs, navigate, switch, close, pin via right-click, middle-click to close, verify persistence across restart
- `⌘T`: verify it's instant (no reload), new tab appears visually to the right of the active tab (clone trick)
- Single tab `⌘W`: verify it closes the window (even if pinned — no confirmation)
- Multi-tab `⌘W` on pinned tab: verify confirmation dialog appears
- Tab bar: verify always visible, single tab shows folder name, pin icon works
- Tab tooltip: hover over tab, verify full path shown
- Selection: verify cleared on tab switch (intentional v1 limitation)
- Cursor: verify cursor position remembered by filename across tab switches
- History: verify back/forward work independently per tab after cloning via `⌘T`
- Context menu: verify "Close other tabs" closes only unpinned tabs, is disabled when no other unpinned tabs exist
- Middle-click on pinned tab: verify it unpins and closes without confirmation
- Sort: verify changing sort in one tab doesn't affect another tab. Verify sort persists per-tab across restart.
- Swap panes: verify `⌘U` swaps only active tab content, tab lists stay in place
- Max tabs: verify toast appears when trying to create 11th tab, "+" button is dimmed
- MCP: check `cmdr://state` resource shows tabs, use `activate_tab` tool
- Rename: start renaming a file, press `⌃Tab` — verify rename is cancelled (not committed)
- Shortcuts: open Settings > Shortcuts, verify all four tab commands appear and are configurable
- Native menu: verify File menu shows "New tab" (⌘T) and "Close tab" (⌘W), no "Close Window"
- Tests: `pnpm vitest run` for unit tests, `./scripts/check.sh --svelte` for full frontend checks, `./scripts/check.sh --rust` for backend
