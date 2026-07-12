/**
 * Explorer store: the dual-pane explorer's navigation + UI-chrome state, lifted
 * out of `DualPaneExplorer`'s component closures into one module so consumers
 * read state directly instead of through `explorerRef` getters.
 *
 * Owns four of the component's fields:
 * - `focusedPane` — which pane has focus (`'left' | 'right'`),
 * - `showHiddenFiles` — the dotfile-visibility toggle,
 * - `leftPaneWidthPercent` — the layout split (the right pane is the remainder),
 * - the two **tab-manager holders** `leftTabMgr` / `rightTabMgr`, each a
 *   `$state<TabManager>` reference.
 *
 * ## What this store does NOT own
 *
 * The tab managers are _values the store holds_, not store fields. They keep
 * their existing setter-based API (`createTabManager`) and are mutated through
 * the free functions in `tabs/tab-state-manager.svelte` / `tab-operations`. The
 * store only holds the *holder reference* and swaps it via `setTabMgr`; the
 * A1/A2 private-state + one-mutator rules govern the store's own fields, never
 * the tab-manager internals. `cursorIndex`, selection, and listing UI state stay
 * local to `FilePane` (perf invariant P3) — they're not here.
 *
 * ## Shape (A1/A2)
 *
 * State is module-private: `createExplorerState()` closes over `$state` locals
 * and exposes only getters and one named mutator per field. There is no exported
 * writable surface — callers can't assign a field, only call a mutator. The
 * `cmdr/no-explorer-state-writes` lint rule makes that a hard wall: assigning to
 * any property of the store object outside this module is a lint error.
 *
 * ## Live references (reactivity transparency)
 *
 * `getTabMgr(pane)` returns the **live** `$state<TabManager>` holder, never a
 * copy or a `$state.snapshot`. A `$derived` reading `getActiveTab(getTabMgr(p))`
 * keeps tracking both when the holder is swapped (`setTabMgr`) and when the held
 * manager mutates in place. This is the Phase-0 `PaneAccess` contract: the
 * factories never see reactivity sever at the seam when state moves from the
 * component into this store. Verified by `explorer-state.svelte.test.ts`.
 *
 * ## Factory + default instance
 *
 * `createExplorerState()` is factory-first for testability (vitest instantiates
 * fresh instances that never share state). The module-level `explorerState`
 * singleton is what the component binds. `_resetForTesting()` resets the
 * singleton to defaults for tests that exercise it; `SvelteSet`/`SvelteMap`
 * (none here yet) would reset via `.clear()`, never reassignment.
 *
 * Writers are enumerated in this module's colocated `pane/CLAUDE.md` (A2).
 */

import { DEFAULT_VOLUME_ID } from '$lib/tauri-commands'
import { createTabManager, type TabManager } from '../tabs/tab-state-manager.svelte'
import { createInitialTabState } from './tab-operations'

/** Default left/right split: an even 50/50 layout. */
const DEFAULT_PANE_WIDTH_PERCENT = 50

/** Builds the per-pane starting tab manager: a single tab at the home folder. */
function createDefaultTabMgr(): TabManager {
  return createTabManager(createInitialTabState('~', DEFAULT_VOLUME_ID))
}

/**
 * The explorer store's public surface: getters + one named mutator per field,
 * plus the live-reference tab-manager holder accessors. No writable state leaks.
 */
export interface ExplorerState {
  /** Returns the focused pane. Reactive. */
  getFocusedPane: () => 'left' | 'right'
  /** Sets the focused pane. The single writer of `focusedPane`. */
  setFocusedPane: (pane: 'left' | 'right') => void

  /** Returns whether hidden (dot) files are shown. Reactive. */
  getShowHiddenFiles: () => boolean
  /** Sets the hidden-files flag to an explicit value. */
  setShowHiddenFiles: (value: boolean) => void
  /** Flips the hidden-files flag. */
  toggleHiddenFiles: () => void

  /** Returns the left pane's width as a percentage; the right pane is the remainder. Reactive. */
  getLeftPaneWidthPercent: () => number
  /** Sets the left pane's width percentage. */
  setLeftPaneWidthPercent: (percent: number) => void

  /** Returns the LIVE tab-manager holder for `pane` (never a copy/snapshot). Reactive. */
  getTabMgr: (pane: 'left' | 'right') => TabManager
  /** Swaps the tab-manager holder for `pane` (e.g. when loading persisted tabs). */
  setTabMgr: (pane: 'left' | 'right', mgr: TabManager) => void

  /**
   * Whether the Ask Cmdr rail owns focus. A PARALLEL third focus region, deliberately
   * NOT folded into the binary `focusedPane` union: exactly one PANE is always focused,
   * and this flag says whether input is actually routed to the rail's composer instead.
   * Reactive.
   */
  getRailFocused: () => boolean
  /** Sets the rail-focused flag. The single writer of `railFocused`. */
  setRailFocused: (value: boolean) => void
}

/**
 * Creates a fresh explorer-state instance. Tests use this for full isolation;
 * the app binds the module-level `explorerState` singleton instead.
 */
export function createExplorerState(): ExplorerState {
  let focusedPane = $state<'left' | 'right'>('left')
  let showHiddenFiles = $state(true)
  let leftPaneWidthPercent = $state(DEFAULT_PANE_WIDTH_PERCENT)
  let leftTabMgr = $state<TabManager>(createDefaultTabMgr())
  let rightTabMgr = $state<TabManager>(createDefaultTabMgr())
  let railFocused = $state(false)

  return {
    getFocusedPane: () => focusedPane,
    setFocusedPane: (pane) => {
      focusedPane = pane
    },

    getShowHiddenFiles: () => showHiddenFiles,
    setShowHiddenFiles: (value) => {
      showHiddenFiles = value
    },
    toggleHiddenFiles: () => {
      showHiddenFiles = !showHiddenFiles
    },

    getLeftPaneWidthPercent: () => leftPaneWidthPercent,
    setLeftPaneWidthPercent: (percent) => {
      leftPaneWidthPercent = percent
    },

    getTabMgr: (pane) => (pane === 'left' ? leftTabMgr : rightTabMgr),
    setTabMgr: (pane, mgr) => {
      if (pane === 'left') {
        leftTabMgr = mgr
      } else {
        rightTabMgr = mgr
      }
    },

    getRailFocused: () => railFocused,
    setRailFocused: (value) => {
      railFocused = value
    },
  }
}

/** The app-wide explorer store. The component binds this; tests reset it via `_resetForTesting`. */
export const explorerState = createExplorerState()

/**
 * Test-only reset of the `explorerState` singleton back to defaults: left-focused,
 * hidden files shown, an even split, and a fresh home-folder tab manager per pane.
 * Tests that touch the singleton call this in `beforeEach`. Not for production use;
 * tests import it via the file path. Keep it in sync with the factory's defaults
 * whenever a new field is added.
 */
export function _resetForTesting(): void {
  explorerState.setFocusedPane('left')
  explorerState.setShowHiddenFiles(true)
  explorerState.setLeftPaneWidthPercent(DEFAULT_PANE_WIDTH_PERCENT)
  explorerState.setTabMgr('left', createDefaultTabMgr())
  explorerState.setTabMgr('right', createDefaultTabMgr())
  explorerState.setRailFocused(false)
}
