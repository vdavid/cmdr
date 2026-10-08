# Tabs details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision rationale, persistence, and closed-tab history live
here.

## Files

- `tab-types.ts`: type definitions (`TabId`, `TabState`, `PersistedTab`, `PersistedPaneTabs`, `UnreachableState`)
- `tab-state-manager.svelte.ts`: reactive state manager (`$state()`); all tab operations + the closed-tab stack. Max 10
  tabs per pane
- `TabBar.svelte`: tab bar UI (always visible, Chrome-style shrinking tabs, pin icons, close buttons, context menu)
- `tab-drag-controller.svelte.ts`, `tab-drop-slot.ts`, `TabDragOverlay.svelte`: dragging a tab (§ Moving a tab)
- `tab-label.ts`: `deriveTabLabel(path, volume)` (see `tab-label.test.ts`). At a volume root carrying a `rootLabel` (an
  SMB share, whose mount dir may be a disambiguated `/Volumes/public-1`) the label is that name, the one the header
  shows. It special-cases only the MTP scheme: an `mtp://…` path derives from the within-storage path
  (`getMtpDisplayPath`), so the storage root shows "/" instead of the raw storage id (`65537`); normal paths and mounted
  volume roots (`/Volumes/USB`) keep their basename
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

## Persistence

Tab state persists via `loadPaneTabs` / `savePaneTabs` in `app-status-store.ts`. Migrates from old scalar keys on first
load.

A tab showing a search-results snapshot persists the folder it searched from instead, and any snapshot path still on
disk is swapped for a real folder at load. Its live `path` is untouched, so Back still reaches the snapshot within the
session. Both layers and the launch loop they prevent: `../pane/DETAILS.md` § "A snapshot never comes back".

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

Two neighbors of that rule. A history that's COPIED claims its own refs: `newTab` clones the active tab's stack, so it
runs `retainSnapshotRefs` on the clone ("Open in pane" clones on every promotion, so this path is ordinary). A tab that
MOVES takes its history, and so its refs, along untouched.

The Tab menu's "Reopen closed tab" item enables/disables based on the focused pane's stack via the
`set_reopen_closed_tab_enabled` Tauri command (mirrors `update_pin_tab_menu`). Frontend pushes the state after every
close, reopen, and focus change. Empty-stack reopen toasts "No recently closed tabs in this pane."; reopen at the cap
toasts "Tab limit reached" and leaves the stack untouched.

## Moving a tab

Drag a tab to reorder it, or drop it on the other pane's bar to move it there. The MCP `tab` tool's `move` action does
the same thing for an agent. There's no keyboard shortcut, menu item, or palette entry for it.

**One rule-owner.** `moveTab(source, target, tabId, toIndex?)` in `tab-state-manager.svelte.ts` decides everything, so
the two entry points can't drift:

- A pinned tab doesn't move, on its own side either. An unpinned tab lands anywhere, including between or before pinned
  tabs (pins aren't grouped to the left).
- A pane's only tab can't leave it, and a pane at `MAX_TABS_PER_PANE` takes no more. A reorder ignores the cap: it adds
  no tab.
- The tab is never activated where it lands, and a drag never activates an inactive tab. Within its own pane a tab stays
  active only if it already was (`activeTabId` is an id, so this is free).
- The ACTIVE tab leaving its pane hands the active slot on through `spliceTabOut`, the same code a close runs: the tab
  to its right, or the one to its left when it was last.
- A move isn't a close. Nothing goes on the closed-tab stack, and the `TabState` object itself crosses over, so its id,
  history, snapshot refs, and `unreachable` state all travel with it.

`toIndex` is the index the tab holds AFTER the move (the end when omitted, clamped to it). For a same-pane move that's
counted with the tab already taken out, which is why the drag needs the conversion below.

**Around the state change** (`../pane/tab-operations.ts::moveTabToPane`, the one layer both entry points share): persist
both panes on a cross-pane move and one on a reorder, report `tab_moved`, and re-sync the Pin tab menu when the focused
pane's active tab left. An active tab that leaves takes its cursor filename along, read from its `FilePane` before the
move, so it shows the same row when it's next opened. Pane focus is never touched. `handleTabDrop` is the mouse's
wrapper: it toasts a drop on a full pane, the one refusal a drag can reach; the MCP path returns every refusal instead.

**The drag** (`tab-drag-controller.svelte.ts`):

- ONE controller sits above both bars, owned by `DualPaneExplorer`, because each `TabBar` sees only its own pane. A bar
  gets a `forPane()` face as its `drag` prop: `attach` (the drop zone, and where its tabs are measured), `press`,
  `draggedTabId`, `isDragging`.
- **Decision**: pointer events on `window`. **Why**: HTML5 drag and drop belongs to Tauri's native file-drop handler
  (`../pane/drag-drop-controller.svelte.ts`, which is for FILE drags and holds no tab logic).
- A press becomes a drag after 5px of travel on the primary button; below that it's a plain click. A pinned tab, a
  pane's only tab (it has nowhere to go), and the close button never start one.
- The pointer picks a SLOT: a bar with `n` tabs has `n + 1`, and everything past the last tab's middle (the empty strip,
  the "+" button) is the append slot. `tab-drop-slot.ts` is that arithmetic, pure: `dropSlotAt`, `slotLineX`, and
  `resolveDrop`, which turns a slot into `moveTab`'s index. The two same-pane slots touching the dragged tab both mean
  "stay", since taking the tab out shifts every later slot down by one.
- Nothing changes until the drop. A release off the bars cancels, and so do Esc, the window losing focus, and
  `pointercancel`. The controller reads the tab lists only to draw an honest preview (no line on a no-op slot, a refusal
  over a bar that would refuse); `moveTab` re-decides on the drop.
- **Gotcha**: a drag must swallow the `click` that follows the release, or dragging an inactive tab would switch to it
  and focus its pane. One capture-phase listener eats it, and a zero-delay timer takes the listener down when no click
  comes. The same goes for a drag cancelled with the button still down (Esc): the release that follows is still owed.
- **Gotcha**: a release the window never saw (the user switched apps mid-drag and let go there) would leave that wait
  armed and eat the next real click. Two things disarm it: a `pointermove` with no button down, and any fresh
  `pointerdown`.

`TabDragOverlay.svelte` draws the drag from the controller's `view`: a see-through ghost copy of the tab that follows
the pointer (locked to a bar's row while over one, free and fainter off the bars; it's never opaque, because it sits on
the landing line and would hide it), a 2px accent line on the landing slot, and the cursor. Its layer covers the whole
window for the length of the drag, which is what keeps hover states and tooltips asleep and gives the "not allowed"
cursor one owner. The dragged tab itself stays in its slot, dimmed; `TabBar` also nulls its tooltips while `isDragging`,
so a tooltip whose delay started before the press can't fire mid-drag. The `is-dragging` / `cannot-drop` class names are
the ones the contrast checker exempts as drag feedback.

**MCP** (`tab` with `action: move`): `pane` is where the tab is now, `tabId` defaults to that pane's active tab,
`toPane` defaults to `pane` (a reorder), and `toIndex` defaults to the end; at least one of `toPane` / `toIndex` is
required. It's the one `tab` action that is a round-trip. The others wait on a generation ack, but a move can be
refused, and the rules live here in the frontend, so `apps/desktop/src/routes/(main)/mcp-tab-move.ts` replies on the
request id with a typed `outcome` (`moved` + `toIndex`, `unchanged`, `pinned`, `onlyTab`, `targetFull`, `notFound`)
after flushing both panes' tab lists past the mirror's debounce (`tab-mcp-sync`'s `syncTabsNow`). The backend words it
(`apps/desktop/src-tauri/src/mcp/executor/app.rs`): a refusal is an error whose `data.reason` is `tabPinned` / `onlyTab`
/ `tabLimitReached` / `tabNotFound`.

## Double-click empty tab bar to open a new tab

`TabBar.svelte`'s `ondblclick` routes to `onNewTab` when the target isn't inside `.tab`, `.close-btn`, or
`.new-tab-btn`, so the bar's right padding strip and the trailing flex space of `.tab-list` both count as "new tab"
surfaces.

## Narrow tabs drop their close button

A tab whose content box shrinks to 80px or less loses its close button; there isn't room for a label and a button both.

The threshold is measured in JS, not queried in CSS. `TabBar.svelte` puts `useInlineSize`
(`$lib/utils/inline-size-action`) on every `.tab`, keeps the ids that are under the threshold in a `SvelteSet`, and
renders `class:narrow` from it; `.tab.narrow .close-btn { display: none }` does the rest. `useInlineSize` reports the
same content box a size query reads, so the number is the one the old `@container (max-width: 80px)` rule used.

**Why not a container query**: `@container` and `container-type` need Safari 16, and Cmdr's WebKit floor is Safari 15
(macOS 12 Monterey ships 15.0). Old WebKit drops the block whole and in silence, so on the oldest macOS Cmdr supports
every tab kept a close button it had no room for, with nothing to say so. Stylelint now rejects both spellings; see
`apps/desktop/src/lib/utils/DETAILS.md` § `inline-size-action.ts`.

A width of 0 reads as "not measured yet", never as narrow: the observer's first callback lands after the first paint,
and treating it as narrow would blink every close button out and back in on mount. Closing a tab leaves its id in the
set, so an `$effect` prunes ids no longer in `tabs`.
