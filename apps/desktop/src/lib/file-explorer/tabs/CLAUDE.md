# Tabs

Per-pane tab system for the dual-pane file explorer. Each pane side (left/right) has an independent tab bar, max 10
tabs.

## Module map

- **`tab-types.ts`**: `TabId`, `TabState`, `PersistedTab`, `PersistedPaneTabs`, `UnreachableState`
- **`tab-state-manager.svelte.ts`**: Reactive `$state()` manager; all tab ops (add, close, switch, cycle, pin) + the
  closed-tab stack
- **`TabBar.svelte`**: Tab bar UI (always visible, Chrome-style shrinking tabs, pins, close buttons, context menu)
- **`tab-label.ts`**: `deriveTabLabel(path)`, the tab title

Architecture, decision rationale, persistence, and closed-tab-history detail: `DETAILS.md`.

## Must-knows

- **Tab switch is a cold load: `{#key activeTabId}` destroys and recreates FilePane, no warm cache.** Inactive tabs hold
  no FilePane, watcher, listing cache, or scroll state. Cursor is restored by filename (`findFileIndex`), not by index,
  because the listing may change while a tab is inactive. Selection is cleared on switch (intentional v1
  simplification).
- **`addTab` inserts to the LEFT without changing `activeTabId`** (the clone trick), so no remount happens and the user
  stays on their current tab; switching to the new tab is a separate explicit action.
- **Ctrl+Tab cycling uses a leading-edge debounce (50ms).** It fires the first press immediately, then batches and
  commits only the final target, so rapid cycling doesn't mount/destroy many FilePanes.
- **Pinned-tab navigation auto-creates a new tab instead of navigating in-place** (pinning preserves a location).
  Inherits the target path, appears after the pinned tab; falls back to in-place only at the 10-tab cap.
- **Tab context menu must use the async event path, not a synchronous channel.** Tauri 2's `Menu::popup()` returns
  before `on_menu_event` fires, and macOS's NSEvent tracking loop consumes the wakeup, so a `mpsc::channel` with timeout
  always races and loses. `on_menu_event` emits a `tab-context-action` event; the frontend uses a one-shot listener
  (`onTabContextAction`) registered before showing the popup. Do NOT switch to a synchronous channel.
- **`getActiveTab` silently falls back to the first tab when `activeTabId` is stale** (after close or restore). Throwing
  would crash the UI; auto-correcting keeps the pane usable.
- **Closed-tab history (Cmd+Shift+T) transfers search-results snapshot refs on close, releases on eviction.**
  `closeTabRecording` / `closeOtherTabsRecording` do NOT decrement refs for `search-results://<id>` history paths (they
  transfer ownership to the stack entry, keeping the snapshot alive for reopen); the actual decrement is the stack's own
  eviction (cap overflow or `trimClosedStack`). The non-recording `closeTab` / `closeOtherTabs` release immediately. All
  bookkeeping flows through `transferSnapshotRefs(closedTab, 'transfer' | 'release')`. See `lib/search/DETAILS.md` §
  "Snapshot store".
- **`tab-label.ts` special-cases only the MTP scheme.** For `mtp://…` paths it derives from the within-storage path
  (`getMtpDisplayPath`) so the storage root shows "/" instead of the raw storage id (`65537`); normal paths and mounted
  volume roots (`/Volumes/USB`) keep their basename. Pinned by `tab-label.test.ts`.

## MCP

- `tab` tool with `action`: `new`, `close`, `close_others`, `activate`, `set_pinned`, `reopen`.
- `tab_id` defaults to active tab for close / close_others / set_pinned; required for activate; unused for new / reopen.
- `close` on the last tab errors instead of closing the window; `close` skips the pinned-tab confirmation; `set_pinned`
  is idempotent; `reopen` is a no-op (fire-and-forget OK reply) when the stack is empty or at the cap.
- Tab list shows in `cmdr://state`. Frontend syncs state via debounced `updatePaneTabs` IPC.
