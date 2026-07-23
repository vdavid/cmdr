# Tabs details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision rationale, persistence, and closed-tab history live
here.

## Files

- `tab-types.ts`: type definitions (`TabId`, `TabState`, `PersistedTab`, `PersistedPaneTabs`, `UnreachableState`)
- `tab-state-manager.svelte.ts`: reactive state manager (`$state()`); all tab operations + the closed-tab stack. Max 10
  tabs per pane
- `TabBar.svelte`: tab bar UI (always visible, Chrome-style shrinking tabs, pin icons, close buttons, context menu)
- `tab-label.ts`: `deriveTabLabel(path)` (see `tab-label.test.ts`)
- `tab-state-manager.test.ts`: unit tests for the state manager

## Key decisions

- **Tabs sit flush with the window title-bar and the pane's left edge, no spacer on either.** Tab and bar both use
  `--spacing-tab-bar-height`; with matching heights and `align-items: end`, tabs land at the bar's bottom edge with no
  offset, so the active tab's accent band touches the title-bar at every text scale. Left padding is zero so the first
  tab's left edge runs into the pane edge. The right side keeps `--spacing-xxs` for the `+` button. The active tab uses
  `bar-height + 1px` with `margin-bottom: -1px` so it hangs 1 px into the path bar below (covers any 1 px seam).
- **Tabs are square (`border-radius: 0`); the only curve is the concave shoulder pair at the bottom corners.** Its arc
  is `--radius-tab-shoulder`, consumed by the shoulder box's size, its offset, and its mask radius — change the
  variable, not the call sites. The name needs a `--radius-` prefix to satisfy stylelint's `custom-property-pattern`
  (`^(color|spacing|font|radius|shadow|transition|z|sheet|titlebar)-`), which is easy to trip on a local geometry
  variable. `.tab.active::after` uses `border-radius: inherit`, so the accent band tracks the tab's corners
  automatically if they ever come back.
- **The active tab's accent is a 2px band on the TOP EDGE ONLY, and it clips itself.** `.tab.active::after` is a
  full-tab-sized box (`inset: 0`) repeating the tab's top radii, painting only its first 2px via a `linear-gradient`: a
  background is clipped to the rounded border box for free, so each end sweeps along the curve instead of stopping
  square. It has to clip ITSELF because `.tab.active` runs `overflow: visible` so its shoulder wedges can escape, which
  means no clipping comes from `.tab`. Two things NOT to do: shrinking the box to the band's height and rounding it
  (browsers scale corner radii down to fit a short box, flattening the curve), and using an inset `box-shadow` ring
  (paints all four sides, so accent runs down the tab's edges). `pointer-events: none` keeps the overlay off the label
  and close button.
- **Cold load on tab switch (`{#key activeTabId}`), no warm cache.** Keeping inactive tabs alive means multiple
  FilePanes with active watchers, listing caches, and scroll state; for 20 tabs total that's untenable. Cold load with
  cursor-by-filename restoration is fast enough that the simplicity wins.
- **Clone trick for new tab.** `addTab` inserts to the LEFT without changing `activeTabId`; since `{#key activeTabId}`
  drives recreation, no remount happens. The user sees the new tab instantly while staying put; switching is separate.
- **Cursor restored by filename, not index.** The listing may have changed while the tab was inactive (watcher events
  still apply); index-based restoration would point to the wrong file. `findFileIndex` is resilient to
  insertions/deletions.
- **Selection cleared on tab switch.** A v1 simplification; preserving it would need a `Set<number>` per tab plus index
  remapping after re-sort. Not worth it without a concrete need.
- **Sort is per-tab, no global per-column memory.** Users browse different directories with different sort needs
  (Downloads by date, projects by name); per-tab sort avoids surprising column changes on switch.
- **Leading-edge debounce on Ctrl+Tab cycling (50ms).** Each switch is a full FilePane remount; rapid cycling without
  debounce mounts/destroys many panes (flicker, wasted IPC). The debounce fires the first press immediately, batches the
  rest, commits only the final target.
- **Pinned-tab navigation auto-creates a new tab.** Pinning preserves a location; navigating in-place would make pinning
  meaningless. The new tab inherits the target path and appears after the pinned tab. Falls back to in-place only at the
  cap (10) to avoid blocking the user.

## Unreachable tabs

When a tab's `resolvePathVolume` call times out during startup restoration, the tab enters an "unreachable" state
(`TabState.unreachable: UnreachableState`). Instead of silently falling back to the default volume, it shows an inline
banner (`VolumeUnreachableBanner.svelte`) with the original path, a "Retry" button, and an "Open home folder" button.
The tab bar shows a small warning icon. Runtime-only (not persisted); volume resolution is re-attempted next startup.

## Context menu

The tab context menu (pin/unpin, close, close others) uses a native Tauri popup via `show_tab_context_menu` IPC.

- **Gotcha: Tauri 2's `Menu::popup()` returns before `on_menu_event` fires.** muda queues the `MenuEvent` through an
  event-loop proxy; the popup's NSEvent tracking loop on macOS consumes the wakeup, so a synchronous `mpsc::channel`
  with timeout always races and loses. Instead, `on_menu_event` emits a `tab-context-action` Tauri event and the
  frontend uses a one-shot listener (`onTabContextAction`) registered before showing the popup. Do NOT try a synchronous
  channel.
- **Gotcha: `getActiveTab` silently fixes stale `activeTabId` by falling back to the first tab.** After closing or
  restoring, `activeTabId` may reference a gone tab; throwing would crash the UI, so auto-correcting keeps the pane
  usable.
- **Gotcha: the tab-bar close button is hidden via a CSS container query at `max-width: 80px`.** Chrome-style shrinking
  tabs can get very narrow; a close button on a 40px tab leaves no room for the label. The container query hides it
  without JS measurement. Middle-click close still works at any width.

## Persistence

Tab state persists via `loadPaneTabs` / `savePaneTabs` in `app-status-store.ts`. Migrates from old scalar keys on first
load.

## Closed-tab history (Cmd+Shift+T)

Per-pane in-memory LIFO stack of recently closed tabs (`closedStack: ClosedTab[]` on `TabManager`). Session-only. Capped
by `fileExplorer.tabs.closedTabHistorySize` (default 10, range 1-50, Advanced settings). When the cap shrinks, both
panes' stacks are trimmed live (oldest first); when the cap is reached on close, the oldest entry is dropped and the
close never refuses.

Each entry stores `{ tab, originalIndex }` where `tab` is a `$state.snapshot` of the closed tab with `unreachable: null`
(runtime-only state isn't restored). Reopening pops the top entry and re-inserts at `min(originalIndex, tabs.length)`,
restoring pin state, sort, view mode, cursor filename, and history. The original tab `id` is kept so consumers see the
same tab return. `closeOtherTabsRecording` pushes closed tabs right-to-left (rightmost first); popping in reverse and
re-inserting at `originalIndex` restores the exact pre-close arrangement.

Search-results snapshot refs follow "transfer on close, release on eviction":

- `closeTabRecording` / `closeOtherTabsRecording` do NOT decrement snapshot refs when pushing onto the stack; the refs
  transfer ownership from the live tab's history to the closed-stack entry, keeping the snapshot alive so a `⌘⇧T` reopen
  restores a usable pane.
- `reopenLastClosedTab` just pops the entry back; refs are still alive, no inc/dec.
- The stack's own eviction (`pushClosed` cap overflow or `trimClosedStack`) is the decrement point: each evicted entry's
  history is walked and every `search-results://` path releases a ref.
- The non-recording `closeTab` / `closeOtherTabs` (tests, programmatic flows) release refs immediately since the close
  isn't recorded anywhere.

Bookkeeping is concentrated in `transferSnapshotRefs(closedTab, 'transfer' | 'release')`, called once at each
transition. See `lib/search/DETAILS.md` § "Snapshot store" for the broader picture.

The Tab menu's "Reopen closed tab" item enables/disables based on the focused pane's stack via the
`set_reopen_closed_tab_enabled` Tauri command (mirrors `update_pin_tab_menu`). Frontend pushes the state after every
close, reopen, and focus change. Empty-stack reopen toasts "No recently closed tabs in this pane."; reopen at the cap
toasts "Tab limit reached" and leaves the stack untouched.

## Double-click empty tab bar to open a new tab

`TabBar.svelte`'s `ondblclick` routes to `onNewTab` when the target isn't inside `.tab`, `.close-btn`, or
`.new-tab-btn`, so the bar's right padding strip and the trailing flex space of `.tab-list` both count as "new tab"
surfaces.
