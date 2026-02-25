# Tabs feature tasks

## M1: Data model + tab state manager
- [x] Create `tab-types.ts` with `TabId`, `TabState` (with `cursorFilename`), `PersistedTab`, `PersistedPaneTabs`
- [x] Create `tab-state-manager.svelte.ts` with all tab operations, `MAX_TABS_PER_PANE = 10` constant
- [x] `addTab` inserts clone to the left of a given tab ID (see plan: "clone trick") and accepts a `$state.snapshot()` of the active tab's `NavigationHistory`
- [x] `addTab` returns `false` when at cap (caller shows toast)
- [x] `closeOtherTabs(mgr, tabId)` only closes unpinned tabs (pinned tabs stick), activates the kept tab
- [x] Write unit tests (create, add-before-active, add-returns-false-at-cap, close, close-others-skips-pinned, switch, pin, cycle, last-tab-guard)
- [x] Run `--check svelte-tests`

## M2: DualPaneExplorer refactoring
- [x] Replace scalar per-pane state with `leftTabMgr`/`rightTabMgr` + `$derived` values
- [x] Update pane accessor helpers to read/write active tab
- [x] Audit all mutation paths (`handlePathChange`, `handleVolumeChange`, `handleSortChange`, `handleViewModeChange`, history push/pop) to ensure they go through active tab state
- [x] Update swap panes (`⌘U`) to swap only active tab content via `getSwapState/adoptListing`, then sync `TabState` fields
- [x] Collateral cleanup: remove `columnSortOrders` entirely — delete `getColumnSortOrder()`, `saveColumnSortOrder()`, and the `columnSortOrders` store key from `app-status-store.ts`
- [x] Update `handleSortChange` to use `defaultSortOrders` map when switching columns (per-column memory intentionally removed)
- [x] Note: removing `columnSortOrders` here before M5 adds per-tab persistence means sort preferences won't persist across restarts during development. This is intentional (avoids dead code). All milestones ship together.
- [x] Verify all existing integration tests still pass (single-tab, no behavior change)
- [x] Run `--svelte`

## M3: Tab switching logic
- [x] Add `newTab()` (returns `boolean`), `closeActiveTab()` (returns `'closed' | 'last-tab'`), `cycleTab()` exports to DualPaneExplorer
- [x] `newTab()`: insert clone to the left of the active tab with `$state.snapshot()` of the active tab's `NavigationHistory`
- [x] Add `{#key activeTabId}` to FilePane instantiation in DualPaneExplorer
- [x] Leading-edge debounce (~50ms) on `cycleTab` only (not `switchTab`) so direct tab clicks are instant while rapid `⌃Tab` cycling only cold-loads the final tab
- [x] Implement switch-away: cancel any rename-in-progress (happens naturally via `{#key}` destroy), read cursor filename from FilePane's cached `entryUnderCursor` (synchronous, no IPC), clear selection
- [x] Implement switch-to: cold-load directory, resolve `cursorFilename` -> index via `findFileIndex` in `onPathChange` callback, fall back to index 0
- [x] Note: focus is not a concern — DualPaneExplorer's container holds DOM focus and `bind:this` auto-updates the pane ref on `{#key}` recreation
- [x] Test tab switching with 2+ tabs manually via `pnpm dev` (create tabs programmatically or via console)
- [x] Run `--svelte`

## M4: TabBar UI
- [x] Add `--tab-bar-height: 28px` CSS variable to `app.css`
- [x] Build `TabBar.svelte` (always visible, shrinking tabs `min-width: 32px`, pin icon, close button, middle-click close, "+" button, full-path tooltip)
- [x] Single tab: show folder name, no close button
- [x] Middle-click on pinned tab: unpin and close without confirmation
- [x] "+" button disabled (dimmed) when at max tabs
- [x] Wire "+" button to `newTab()` — if returns `false`, show brief toast ("Tab limit reached")
- [x] `role="tablist"` / `role="tab"` / `aria-selected` for accessibility
- [x] Test tab bar visuals, tab creation, closing, middle-click on pinned tabs manually
- [x] Run `--svelte`, `--check stylelint`, `--check css-unused`

## M5: Persistence + migration
- [x] Add `loadPaneTabs`/`savePaneTabs` to `app-status-store.ts`
- [x] Add migration from old scalar keys; sort order defaults to `defaultSortOrders[sortBy]` (with `TODO(2026-04-01)` comment)
- [x] Debounce `saveAppStatus()` globally with trailing-edge 200ms in `app-status-store.ts` (trailing-edge so the final state is persisted, not the first; all existing callers benefit)
- [x] Wire save triggers in DualPaneExplorer (tab create/close/switch/pin/sort change)
- [x] Test persistence: create tabs with different sorts, restart app, verify tabs and sort state restored
- [x] Run `--svelte`

## M6: Keyboard shortcuts, commands, and native menu
- [x] Add `tab.new`, `tab.close`, `tab.next`, `tab.prev` to `src/lib/commands/command-registry.ts` (all auto-appear in Settings > Shortcuts)
- [x] Rust (`menu.rs`): remove the predefined "Close Window" item from the File submenu (same remove/insert pattern as the About item), add custom "New tab" (`⌘T`, `NEW_TAB_ID`) and "Close tab" (`⌘W`, `CLOSE_TAB_ID`) menu items
- [x] Add Tauri event listeners for `new-tab` and `close-tab` in `+page.svelte` `setupMenuListeners` (same pattern as `switch-pane`, `swap-panes`)
- [x] Add handler cases in `+page.svelte` `handleCommandExecute`
- [x] `tab.close` (`⌘W`): if active tab is pinned and not the last tab, show `confirmDialog`; then call `explorerRef.closeActiveTab()`. If it returns `'last-tab'`, call `getCurrentWindow().close()` (no confirmation even if pinned)
- [x] `tab.new` (`⌘T`): call `explorerRef.newTab()`. If returns `false`, show toast ("Tab limit reached").
- [x] Test `⌘T` (instant, clone trick — new tab appears to the right), `⌘W` (single-tab closes window, multi-tab closes tab, pinned tab shows confirmation), `⌃Tab`, `⌃⇧Tab` manually
- [x] Verify native File menu shows "New tab" and "Close tab" (no "Close Window")
- [x] Run `--rust`, `--svelte`

## M7: Context menu + pinning
- [x] Rust: add `build_tab_context_menu` to `menu.rs` (with "Pin/Unpin tab", "Close other tabs", "Close tab")
- [x] "Close other tabs" disabled when no other unpinned tabs exist
- [x] "Close tab" on a pinned tab via context menu: show `confirmDialog` before closing
- [x] Rust: add `show_tab_context_menu` command to `commands/ui.rs`
- [x] Rust: register command in `lib.rs`
- [x] Frontend: create `tauri-commands/tab.ts` with `showTabContextMenu` wrapper, re-export from `index.ts`
- [x] Wire context menu in TabBar `oncontextmenu` — dispatch pin/unpin/close/close-others to tab manager
- [x] Run `--rust`, `--svelte`

## M8: MCP integration
- [x] Add `TabInfo` struct to `pane_state.rs`
- [x] Add `tabs` field to `PaneState`
- [x] Render tabs in YAML resource (`resources.rs`)
- [x] Add `activate_tab` tool definition (`tools.rs`) and handler (`executor.rs`)
- [x] Add `update_pane_tabs` Tauri command
- [x] Add debounced (~100ms) `$effect` to sync structural tab changes only (count, order, active ID, pinned) to backend
- [x] Add event listener for MCP `activate_tab` in DualPaneExplorer
- [x] Run `--rust`

## M9: Polish + full check
- [x] Tab bar dark mode styling verification
- [x] Edge case: verify toast appears when max tabs (10) reached, "+" button dimmed
- [x] Test sort independence: change sort in one tab, verify others unaffected
- [x] Test cursor restore: navigate in tab A, switch to B, switch back, verify cursor on correct file
- [x] Test history independence: clone tab via `⌘T`, navigate differently in each, verify back/forward work independently
- [x] Test selection clear: select files, switch tabs, switch back, verify selection is gone
- [x] Test swap panes: verify `⌘U` swaps only active tab content, tab lists stay in place
- [x] Test middle-click: unpinned tab closes, pinned tab unpins and closes (no confirmation)
- [x] Test `⌘W` on pinned tab: confirmation if not last tab, window close (no confirmation) if last tab
- [x] Test rename cancellation: start renaming a file, press `⌃Tab` — verify rename is cancelled (not committed)
- [x] Test operation guard: open a copy/move/mkdir/delete dialog, verify `⌘T`/`⌘W`/`⌃Tab` are all blocked (existing modal dialog guard)
- [x] Test deleted directory: switch away from a tab, delete its directory externally, switch back — verify graceful fallback (parent or error)
- [x] Verify Settings > Shortcuts shows all four tab commands, and they're configurable
- [x] Verify native File menu shows "New tab" (⌘T) and "Close tab" (⌘W), no "Close Window"
- [x] Update `file-explorer/CLAUDE.md` with tabs info
- [x] Create `tabs/CLAUDE.md`
- [x] Add new files to `coverage-allowlist.json` if needed
- [x] Run full `./scripts/check.sh`
