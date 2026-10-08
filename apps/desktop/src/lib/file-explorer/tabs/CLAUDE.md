# Tabs

Per-pane tab system for the dual-pane file explorer. Each pane side (left/right) has an independent tab bar, max 10
tabs.

## Module map

- **`tab-types.ts`**: `TabId`, `TabState`, `PersistedTab`, `PersistedPaneTabs`, `UnreachableState`
- **`tab-state-manager.svelte.ts`**: Reactive `$state()` manager; all tab ops (add, close, switch, cycle, pin, move) +
  the closed-tab stack
- **`TabBar.svelte`**: Tab bar UI (always visible, Chrome-style shrinking tabs, pins, close buttons, context menu)
- **`tab-drag-controller.svelte.ts`**, **`tab-drop-slot.ts`**, **`TabDragOverlay.svelte`**: drag a tab to reorder it or
  move it to the other pane (controller, pure slot math, ghost + landing line)
- **`tab-label.ts`**: `deriveTabLabel(path, volume)`, the tab title
- **`tab-analytics.ts`**: the event vocabulary. Emitted from `pane/tab-operations.ts`, ❌ never from the pure state
  manager (unit tests drive it directly).

Architecture, decision rationale, persistence, and closed-tab-history detail: `DETAILS.md`.

## Must-knows

- **Tab switch is a cold load: `{#key activeTabId}` destroys and recreates FilePane, no warm cache.** Inactive tabs hold
  no FilePane, watcher, listing cache, or scroll state. Cursor is restored by filename (`findFileIndex`), since the
  listing may change meanwhile. Selection is cleared on switch.
- **`addTab` inserts to the LEFT without changing `activeTabId`** (the clone trick), so no remount happens; switching to
  the new tab is a separate explicit action.
- **`moveTab` owns every rule of a move** (pinned and only tabs stay, a full pane refuses, the tab is never activated
  where it lands). The drag and MCP `tab move` both reach it via `pane/tab-operations.ts::moveTabToPane`. ❌ Don't
  re-decide a rule in the controller or in Rust.
- **A tab drag is pointer events on `window`, ❌ never HTML5 drag and drop**: Tauri's file-drop handler owns that. The
  click after a drop is swallowed, or a drag would switch tabs.
- **Ctrl+Tab cycling uses a leading-edge debounce (50ms)**, so rapid cycling doesn't mount/destroy many FilePanes.
- **Pinned-tab navigation auto-creates a new tab instead of navigating in-place** (pinning preserves a location). It
  falls back to in-place only at the 10-tab cap.
- **Tab context menu must use the async event path, not a synchronous channel.** Tauri 2's `Menu::popup()` returns
  before `on_menu_event` fires, so a `mpsc::channel` with timeout always loses the race.
- **`getActiveTab` silently falls back to the first tab when `activeTabId` is stale** (after close or restore). Throwing
  would crash the UI; auto-correcting keeps the pane usable.
- **Closed-tab history (Cmd+Shift+T) transfers search-results snapshot refs on close, releases on eviction.** The
  recording closes keep the refs on the stack entry so a reopen works; eviction (cap overflow, `trimClosedStack`) is the
  decrement. The non-recording `closeTab` / `closeOtherTabs` release at once. A move carries its refs along untouched.
- **A CLONED history claims its own refs**: `newTab` copies the active tab's stack, so `retainSnapshotRefs` runs on the
  clone. Skip it and the first of the two tabs to close evicts rows the other still shows.
- **`tab-label.ts` special-cases only the MTP scheme**, so a storage root shows "/" instead of the raw storage id
  (`65537`). Pinned by `tab-label.test.ts`.

## MCP

- `tab` tool with `action`: `new`, `close`, `close_others`, `activate`, `set_pinned`, `reopen`, `move`.
- `tabId` defaults to the active tab for close / close_others / set_pinned / move; required for activate.
- `close` on the last tab errors instead of closing the window and skips the pinned-tab confirmation; `set_pinned` is
  idempotent; `reopen` is a no-op when the stack is empty or at the cap.
- `move` takes `toPane` and/or `toIndex` and answers a typed outcome (`DETAILS.md` § Moving a tab).
- Tab list shows in `cmdr://state`. Frontend syncs state via debounced `updatePaneTabs` IPC.
