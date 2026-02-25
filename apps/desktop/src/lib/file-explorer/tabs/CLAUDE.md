# Tabs

Per-pane tab system for the dual-pane file explorer. Each pane side (left/right) has an independent tab bar.

## Architecture

- `tab-types.ts` — Type definitions: `TabId`, `TabState`, `PersistedTab`, `PersistedPaneTabs`
- `tab-state-manager.svelte.ts` — Reactive state manager using `$state()`. All tab operations (add, close, switch,
  cycle, pin). Max 10 tabs per pane.
- `TabBar.svelte` — Tab bar UI component. Always visible, Chrome-style shrinking tabs, pin icons, close buttons, context
  menu.
- `tab-state-manager.test.ts` — Unit tests for state manager

## Key decisions

- **Cold load on tab switch**: `{#key activeTabId}` destroys and recreates FilePane. No warm cache.
- **Clone trick for new tab**: `addTab` inserts clone to the LEFT without changing `activeTabId`. No `{#key}` fires,
  feels instant.
- **Cursor restored by filename**: Saved on switch-away, resolved via `findFileIndex` on switch-to.
- **Selection cleared**: Not preserved across tab switches (v1 limitation).
- **Sort is per-tab**: No global per-column sort memory.
- **Leading-edge debounce on cycle**: Rapid Ctrl+Tab only loads the final tab.
- **Pinned tab navigation**: Navigating to a different directory on a pinned tab auto-creates a new tab with the target
  path (inserted after the pinned tab). The pinned tab keeps its location. Falls back to in-place navigation if at the
  tab cap (10).

## Context menu

Tab context menu (pin/unpin, close, close others) uses a native Tauri popup menu via `show_tab_context_menu` IPC.

**Gotcha: async event timing.** Tauri 2's `Menu::popup()` returns before `on_menu_event` fires because muda queues the
`MenuEvent` through an event loop proxy. The popup's NSEvent tracking loop on macOS consumes the wakeup signal, so a
synchronous `mpsc::channel` with timeout always races and loses. Instead, `on_menu_event` emits a `tab-context-action`
Tauri event, and the frontend uses a one-shot listener (`onTabContextAction`) registered before showing the popup. Do
NOT try a synchronous channel approach — it will always time out.

## Persistence

Tab state persisted via `loadPaneTabs`/`savePaneTabs` in `app-status-store.ts`. Migrates from old scalar keys on first
load.

## MCP

- `activate_tab` tool switches to a tab by ID
- Tab list shown in `cmdr://state` YAML resource
- Frontend syncs tab state to backend via debounced `updatePaneTabs` IPC
