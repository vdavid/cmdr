/**
 * The single nav-state persistence subscriber (invariant A5).
 *
 * After this milestone there is exactly ONE module that watches the explorer
 * store and writes pane navigation state to `app-status.json`: this one. The
 * scattered `saveAppStatus` + `saveTabsForPaneSide` trigger sites that used to
 * live in `navigate()`'s `persist` fan-out and the surviving `DualPaneExplorer`
 * handlers (sort / view-mode / focus / swap / mirror) collapse into the reactive
 * `$effect`s here. Grep "where does pane nav-state persist?" → this file.
 *
 * ## Subscribe, don't poll (A9 / AGENTS principle 6)
 *
 * The pane-state effects are reactive: they READ the store's derived nav-state
 * (per-pane active-tab path / volumeId / viewMode / sortBy + `focusedPane`) and
 * re-run whenever any of those change, deriving the `AppStatus` snapshot and
 * firing the existing (already-debounced) `saveAppStatus`. That's the win
 * deviation-2 of the phase plan calls out: ONE trigger site, not new debounce
 * machinery — `saveAppStatus` already debounces 200 ms + merges per-field, so the
 * effect just calls it once per store change and lets the debounce coalesce.
 *
 * ## Per-pane slicing (P1)
 *
 * Two per-pane effects (`left` / `right`), never one effect reading both panes'
 * tab arrays. A left-pane navigation re-runs only the left effect, so the right
 * pane's tabs aren't re-persisted. Focus is its own effect (a scalar, not
 * per-pane). This keeps the reactivity graph honest: changing one pane touches
 * one persistence path.
 *
 * ## Diff against the last-persisted snapshot
 *
 * Each effect diffs the freshly-derived fields against what it last persisted and
 * calls `saveAppStatus` with only the changed subset (matching the partial
 * patches the old scattered call sites sent — `{ leftSortBy }`, `{ focusedPane }`,
 * …). A re-run that changed nothing (a dependency the snapshot doesn't include
 * fired the effect) is a no-op: no `saveAppStatus`, no `saveTabsForPaneSide`. The
 * persisted field SET is byte-identical to before (paths / volumeId / viewMode /
 * sortBy / focusedPane / leftPaneWidthPercent / per-pane tabs).
 *
 * ## What stays OUT of the reactive effects (deliberate)
 *
 * - **`leftPaneWidthPercent` (layout).** A reactive effect on the width would
 *   persist on every drag FRAME (`handlePaneResize` sets the width per frame). The
 *   200 ms debounce would still leak intermediate widths on a slow drag. Today the
 *   width persists ONLY at drag-end (`handlePaneResizeEnd` / `…Reset`). So layout
 *   is NOT reactive: the component calls `persistLayout(percent)` from the
 *   drag-end handlers explicitly. Same single module (A5), drag-end-only semantics
 *   preserved (PR3).
 * - **`last-used-path` (`volumeId → path` map).** This is a DELTA, not a snapshot:
 *   on a volume switch the OLD path of the OLD volume is recorded, a value the
 *   store no longer holds by the time an effect could read it. `navigate()` owns
 *   that delta (it has the old value before the swap), so it stays an explicit
 *   input: `navigate()`'s `persist` callback forwards the `last-used-path` event
 *   here via `persistLastUsedPath(record)`. Still exactly one module fires
 *   `saveLastUsedPathForVolume` (A5) — this one.
 *
 * ## What this module does NOT own (the A5 per-surface split — documented in CLAUDE.md)
 *
 * - **Tab-set STRUCTURE** (open / close / reorder / pin / reopen) persists from
 *   `tab-operations.ts` (`saveTabsForPane`). That's tab CRUD, a separate surface;
 *   the subscriber owns active-tab NAV-state + focus, `tab-operations` owns tab
 *   structure. Both write `app-status.json` tab keys via `savePaneTabs`, but a
 *   nav change and a tab-bar action are distinct triggers.
 * - **The MCP backend mirror** (`updatePaneTabs` / `updateFocusedPane` /
 *   `syncTabsToBackend`, L8) — that's the Rust state store for MCP, a different
 *   target and debounce (100 ms), not disk persistence.
 * - **`showHiddenFiles`** — a SETTING, persisted via the settings store
 *   (`saveSettings`), not `app-status`.
 *
 * ## Effect creation timing (L3)
 *
 * The effects are created synchronously in this factory body (the
 * `initListingDiffSync` / `createDragDropController` pattern), so the factory MUST
 * be called synchronously during component init — it needs Svelte's
 * effect-tracking context. Never lazily, never in `onMount`.
 */

import { saveAppStatus, saveLastUsedPathForVolume } from '$lib/app-status-store'
import type { ViewMode } from '$lib/app-status-store'
import { recordVisit } from '$lib/tauri-commands'
import type { SortColumn, SortOrder } from '../types'
import type { LastUsedPathRecord } from './navigate'

/**
 * The nav-state the subscriber reads off the store, plus the persistence hooks.
 * Mirrors the `NavigateDeps` / `PaneAccess` factory shape: the component builds
 * these from its store-backed getters; tests pass fakes. All getters return LIVE
 * reactive reads (never `$state.snapshot`) so the effects track changes.
 */
export interface PersistenceSubscriberDeps {
  /** True once persisted state has loaded — gates the effects so the load-from-disk
   *  mutations don't immediately re-persist what we just read. Reactive. */
  getInitialized: () => boolean

  /** The focused pane. Reactive. */
  getFocusedPane: () => 'left' | 'right'

  /** A pane's active-tab nav-state. `path` / `volumeId` / `viewMode` / `sortBy`
   *  are `AppStatus` fields; `sortOrder` is NOT an `AppStatus` field but IS a tab
   *  field — an order-only toggle (clicking the same column) must still re-persist
   *  the pane's tab set, so the effect tracks it too. Reactive. */
  getPanePath: (pane: 'left' | 'right') => string
  getPaneVolumeId: (pane: 'left' | 'right') => string
  getPaneViewMode: (pane: 'left' | 'right') => ViewMode
  getPaneSortBy: (pane: 'left' | 'right') => SortColumn
  getPaneSortOrder: (pane: 'left' | 'right') => SortOrder

  /** Persists a pane's whole tab set (history-bearing) via `savePaneTabs`. The
   *  component wires this to `saveTabsForPane(pane, getTabMgr)`. */
  saveTabsForPaneSide: (pane: 'left' | 'right') => void
}

/** Per-pane snapshot of the nav-state a pane owns, for diffing. `sortOrder` rides
 *  along (tab-only, not an `AppStatus` field) so an order-toggle re-persists tabs. */
interface PaneSnapshot {
  path: string
  volumeId: string
  viewMode: ViewMode
  sortBy: SortColumn
  sortOrder: SortOrder
}

/** The subscriber's explicit (non-reactive) persistence hooks for the two deltas
 *  that can't be derived from a store snapshot. */
export interface PersistenceSubscriber {
  /** Record the last-used path for a volume. Forwarded from `navigate()`'s
   *  `last-used-path` persist event (the old-path pre-save on volume switch). */
  persistLastUsedPath: (record: LastUsedPathRecord) => void
  /** Persist the layout split. Called from the drag-END handlers only, never per
   *  frame, so the width persists exactly when it does today. */
  persistLayout: (leftPaneWidthPercent: number) => void
}

/**
 * Builds the partial `saveAppStatus` patch key for a pane field, e.g.
 * `('left', 'sortBy') → 'leftSortBy'`. Matches the old `paneKey` in DPE so the
 * persisted keys are byte-identical.
 */
function paneKey(pane: 'left' | 'right', field: string): string {
  return `${pane}${field.charAt(0).toUpperCase()}${field.slice(1)}`
}

/**
 * Creates the single nav-state persistence subscriber. Call synchronously during
 * component init (L3). Returns the explicit hooks for the two deltas the reactive
 * effects can't cover (last-used-path, layout).
 */
export function initPersistenceSubscriber(deps: PersistenceSubscriberDeps): PersistenceSubscriber {
  // One per-pane snapshot of the last-persisted nav-state, so each effect diffs
  // and emits only the changed `AppStatus` fields (and re-persists the pane's tab
  // set only when its nav-state actually moved). `null` until the first post-init
  // run SEEDS it (without persisting): the baseline is whatever was loaded from
  // disk, so loading doesn't immediately re-persist what it just read (PR3 —
  // matches today, where load triggers no save).
  const lastPersisted: Record<'left' | 'right', PaneSnapshot | null> = { left: null, right: null }
  let lastFocusedPane: 'left' | 'right' | null = null

  // Focus effect: a scalar, not per-pane. Persists `focusedPane` on change.
  $effect(() => {
    const focusedPane = deps.getFocusedPane()
    if (!deps.getInitialized()) return
    // Seed the baseline on the first post-init run (loaded state), no save.
    if (lastFocusedPane === null) {
      lastFocusedPane = focusedPane
      return
    }
    if (focusedPane === lastFocusedPane) return
    lastFocusedPane = focusedPane
    saveAppStatus({ focusedPane })
  })

  // Per-pane nav-state effects (P1: one effect per pane, never both at once).
  for (const pane of ['left', 'right'] as const) {
    $effect(() => {
      const snapshot: PaneSnapshot = {
        path: deps.getPanePath(pane),
        volumeId: deps.getPaneVolumeId(pane),
        viewMode: deps.getPaneViewMode(pane),
        sortBy: deps.getPaneSortBy(pane),
        sortOrder: deps.getPaneSortOrder(pane),
      }
      if (!deps.getInitialized()) return

      const prev = lastPersisted[pane]
      // Seed the baseline on the first post-init run (loaded state), no save.
      if (prev === null) {
        lastPersisted[pane] = snapshot
        return
      }
      // Only the `AppStatus` fields go in the saveAppStatus patch (sortOrder is
      // tab-only — it never had an AppStatus key, so persisting it there would
      // change the persisted field set, a PR3 violation).
      const patch: Record<string, unknown> = {}
      if (prev.path !== snapshot.path) patch[paneKey(pane, 'path')] = snapshot.path
      if (prev.volumeId !== snapshot.volumeId) patch[paneKey(pane, 'volumeId')] = snapshot.volumeId
      if (prev.viewMode !== snapshot.viewMode) patch[paneKey(pane, 'viewMode')] = snapshot.viewMode
      if (prev.sortBy !== snapshot.sortBy) patch[paneKey(pane, 'sortBy')] = snapshot.sortBy

      // Nothing tab-relevant changed (the effect re-ran for an unrelated reason): no-op.
      const tabStateChanged = prev.sortOrder !== snapshot.sortOrder || Object.keys(patch).length > 0
      if (!tabStateChanged) return

      lastPersisted[pane] = snapshot
      if (Object.keys(patch).length > 0) saveAppStatus(patch)
      // The active tab's nav-state moved (path / volumeId / sort / order / view),
      // so its persisted tab record is stale — re-persist the pane's tab set.
      deps.saveTabsForPaneSide(pane)
    })
  }

  return {
    persistLastUsedPath: (record) => {
      void saveLastUsedPathForVolume(record.volumeId, record.path)
      // Feed the folder-importance visit signal from the same navigation-commit
      // point. Fire-and-forget and failure-silent (the wrapper never throws): a
      // visit that can't be recorded must never affect navigation.
      void recordVisit(record.volumeId, record.path)
    },
    persistLayout: (leftPaneWidthPercent) => {
      saveAppStatus({ leftPaneWidthPercent })
    },
  }
}
