import type { TabId, TabState } from './tab-types'
import { push as navHistoryPush, type HistoryEntry, type NavigationHistory } from '../navigation/navigation-history'
import { decrementRef as decrementSnapshotRef } from '$lib/search/snapshot-store.svelte'

export const MAX_TABS_PER_PANE = 10

/** URL prefix that identifies a history entry pointing at a search-results snapshot. */
const SEARCH_RESULTS_PREFIX = 'search-results://'

/**
 * Extracts the snapshot id from a `search-results://<id>` history-entry path, or
 * returns `null` for any other entry. Keeping the parse in one place means
 * `transferSnapshotRefs`, `applyPushResult`, and any future caller agree on the
 * exact wire format.
 */
function snapshotIdFromEntry(entry: HistoryEntry): string | null {
  if (!entry.path.startsWith(SEARCH_RESULTS_PREFIX)) return null
  return entry.path.slice(SEARCH_RESULTS_PREFIX.length)
}

/**
 * Walks the dropped entries from a `push()` result (or a closed-tab stack
 * eviction) and decrements the snapshot ref for any `search-results://` paths.
 * Other entries are skipped silently. This is the single integration point the
 * tab-state manager owns; `navigation-history.ts` stays pure and search-agnostic.
 */
function releaseSnapshotRefs(entries: HistoryEntry[]): void {
  for (const entry of entries) {
    const id = snapshotIdFromEntry(entry)
    if (id !== null) decrementSnapshotRef(id)
  }
}

/**
 * Public helper used by `DualPaneExplorer.svelte` (and equivalents) to mutate a
 * tab's history via `push()` and release snapshot refs for any dropped entries in
 * one step. Returns the new history so call sites can also use it for things like
 * deriving display state. Keeping the call sites this thin means every push goes
 * through the refcount-decrement path; no caller can accidentally leak a
 * search-results snapshot ref by forgetting to inspect `droppedEntries`.
 */
export function pushHistoryEntry(history: NavigationHistory, entry: HistoryEntry): NavigationHistory {
  const result = navHistoryPush(history, entry)
  releaseSnapshotRefs(result.droppedEntries)
  return result.history
}

/**
 * Mode argument for `transferSnapshotRefs`. The `'transfer'` mode is a no-op on
 * the refcount — by construction, refs stay with the closed-tab stack entry, so
 * neither side changes (the live-tab history is gone but the closed-stack entry
 * keeps the same set of entries pointing at the same snapshots). The `'release'`
 * mode is the actual decrement point: it runs when the closed-tab stack evicts an
 * entry (cap overflow or manual clear).
 */
export type TransferAction = 'transfer' | 'release'

/**
 * Reference-bookkeeping helper for the closed-tab stack. See the close/reopen
 * lifecycle below.
 *
 * - `'transfer'`: a tab is being moved from live → closed-tab stack. Refs already
 *   exist on the snapshots (held by the live tab's history); we leave them in
 *   place. Calling here is a no-op but the call site stays self-documenting.
 * - `'release'`: a closed-tab entry is being evicted (cap overflow or manual
 *   `trimClosedStack`). Walk the tab's history and decrement the snapshot ref for
 *   every `search-results://` entry.
 */
export function transferSnapshotRefs(closedTab: ClosedTab, action: TransferAction): void {
  if (action === 'transfer') return
  releaseSnapshotRefs(closedTab.tab.history.stack)
}

/** One entry on the per-pane closed-tab stack, captured via `$state.snapshot`. */
export interface ClosedTab {
  /** Full snapshot of the closed tab (history, pin state, sort, etc.). `unreachable` is forced to `null`. */
  tab: TabState
  /** Position in the tabs array at close time. Used to restore the tab in its original slot. */
  originalIndex: number
}

export interface TabManager {
  tabs: TabState[]
  activeTabId: TabId
  /** LIFO stack of recently closed tabs (top of stack = most recent). Capped via setting. */
  closedStack: ClosedTab[]
}

/** Creates a tab manager with a single initial tab */
export function createTabManager(initialTab: TabState): TabManager {
  const tabs = $state<TabState[]>([initialTab])
  let activeTabId = $state<TabId>(initialTab.id)
  const closedStack = $state<ClosedTab[]>([])

  return {
    get tabs() {
      return tabs
    },
    set tabs(value: TabState[]) {
      tabs.length = 0
      tabs.push(...value)
    },
    get activeTabId() {
      return activeTabId
    },
    set activeTabId(value: TabId) {
      activeTabId = value
    },
    get closedStack() {
      return closedStack
    },
    set closedStack(value: ClosedTab[]) {
      closedStack.length = 0
      closedStack.push(...value)
    },
  }
}

/** Returns the currently active tab. Falls back to first tab if active ID is stale. */
export function getActiveTab(mgr: TabManager): TabState {
  const tab = mgr.tabs.find((t) => t.id === mgr.activeTabId)
  if (!tab) {
    if (mgr.tabs.length > 0) {
      mgr.activeTabId = mgr.tabs[0].id
      return mgr.tabs[0]
    }
    throw new Error('Tab manager has no tabs')
  }
  return tab
}

/**
 * Inserts a new tab to the left of beforeTabId.
 * Returns false if at cap (10 tabs).
 */
export function addTab(mgr: TabManager, beforeTabId: TabId, tabState: TabState): boolean {
  if (mgr.tabs.length >= MAX_TABS_PER_PANE) {
    return false
  }

  const beforeIndex = mgr.tabs.findIndex((t) => t.id === beforeTabId)
  if (beforeIndex === -1) {
    // If beforeTabId not found, append at end
    mgr.tabs.push(tabState)
  } else {
    mgr.tabs.splice(beforeIndex, 0, tabState)
  }

  // Don't change activeTabId; the clone trick relies on this:
  // the clone is inserted to the LEFT, the original tab stays active
  return true
}

/** Result of closing a tab */
export type CloseTabResult = { closed: true; newActiveTabId: TabId } | { closed: false }

/**
 * Internal: splice the tab out and reassign `activeTabId` if needed. Does NOT
 * touch snapshot refcounts; the caller decides whether to release or transfer
 * refs based on whether the close is being recorded.
 */
function spliceTabOut(mgr: TabManager, tabId: TabId): { closing: TabState; index: number } | null {
  if (mgr.tabs.length <= 1) return null
  const index = mgr.tabs.findIndex((t) => t.id === tabId)
  if (index === -1) return null

  const wasActive = mgr.activeTabId === tabId
  const closing = mgr.tabs[index]
  mgr.tabs.splice(index, 1)

  if (wasActive) {
    const newIndex = index < mgr.tabs.length ? index : mgr.tabs.length - 1
    mgr.activeTabId = mgr.tabs[newIndex].id
  }
  return { closing, index }
}

/**
 * Closes a tab. Returns the new active tab ID if closed, or false if it's the
 * last tab.
 *
 * **Snapshot refs**: this is the "no recording" path — refs in the closed tab's
 * history are released immediately. The recording variant (`closeTabRecording`)
 * transfers refs to the closed-tab stack instead, since reopening must restore
 * the same snapshots; refs only get released when the stack evicts that entry.
 */
export function closeTab(mgr: TabManager, tabId: TabId): CloseTabResult {
  const spliced = spliceTabOut(mgr, tabId)
  if (!spliced) return { closed: false }
  releaseSnapshotRefs(spliced.closing.history.stack)
  return { closed: true, newActiveTabId: mgr.activeTabId }
}

/** Closes all unpinned tabs except the given one. Pinned tabs stay. The given tab becomes active.
 *  Releases snapshot refs for the closed tabs' histories (non-recording path). */
export function closeOtherTabs(mgr: TabManager, tabId: TabId): void {
  const removed = mgr.tabs.filter((t) => t.id !== tabId && !t.pinned)
  mgr.tabs = mgr.tabs.filter((t) => t.id === tabId || t.pinned)
  mgr.activeTabId = tabId
  for (const tab of removed) releaseSnapshotRefs(tab.history.stack)
}

// --- Closed-tab history (Cmd+Shift+T) ----------------------------------------------

/**
 * Snapshots a tab for the closed-tab stack. Drops `unreachable` (runtime-only) and
 * deep-clones reactive state via `$state.snapshot` so the entry survives even if the
 * underlying tab object is later mutated or garbage collected.
 */
function snapshotTabForClose(tab: TabState): TabState {
  const snap = $state.snapshot(tab)
  snap.unreachable = null
  return snap
}

/**
 * Pushes onto the closed-tab stack, dropping the oldest entry if at `cap`.
 * Dropped (evicted) entries release their snapshot refs via `transferSnapshotRefs`
 * — eviction is the actual decrement point for the closed-tab path.
 */
function pushClosed(mgr: TabManager, entry: ClosedTab, cap: number): void {
  mgr.closedStack.push(entry)
  while (mgr.closedStack.length > cap) {
    const evicted = mgr.closedStack.shift() // drop oldest (front)
    if (evicted) transferSnapshotRefs(evicted, 'release')
  }
}

/**
 * Closes a tab and records the close on the closed-tab stack so it can be
 * reopened. No-op record when nothing to record (single-tab case or missing id).
 *
 * **Snapshot refs**: refs are TRANSFERRED to the closed-tab stack entry, not
 * released. They're released later, when the closed-tab stack evicts this entry
 * (cap overflow in `pushClosed`, or manual `trimClosedStack`).
 */
export function closeTabRecording(mgr: TabManager, tabId: TabId, cap: number): CloseTabResult {
  if (mgr.tabs.length <= 1) return { closed: false }
  const index = mgr.tabs.findIndex((t) => t.id === tabId)
  if (index === -1) return { closed: false }
  const tabToClose = mgr.tabs[index]
  const snapshot = snapshotTabForClose(tabToClose)
  const spliced = spliceTabOut(mgr, tabId)
  if (!spliced) return { closed: false }
  // Do NOT release refs here: ownership transfers to the closed-tab stack entry.
  const closedTabEntry: ClosedTab = { tab: snapshot, originalIndex: index }
  transferSnapshotRefs(closedTabEntry, 'transfer')
  pushClosed(mgr, closedTabEntry, cap)
  return { closed: true, newActiveTabId: mgr.activeTabId }
}

/**
 * Closes all other tabs (except the given one + pinned), recording each close on the
 * closed-tab stack in right-to-left order. Pushing rightmost-first means that popping
 * in reverse and re-inserting each tab at its `originalIndex` restores the exact
 * pre-close arrangement. Snapshot refs transfer to the closed-tab stack entries
 * (no release until eviction).
 */
export function closeOtherTabsRecording(mgr: TabManager, tabId: TabId, cap: number): void {
  // Collect tabs to close with their original indices, then push right-to-left.
  const toClose: { snapshot: TabState; originalIndex: number }[] = []
  mgr.tabs.forEach((tab, index) => {
    if (tab.id !== tabId && !tab.pinned) {
      toClose.push({ snapshot: snapshotTabForClose(tab), originalIndex: index })
    }
  })
  // Sort by descending originalIndex so we push rightmost first.
  toClose.sort((a, b) => b.originalIndex - a.originalIndex)
  // Splice tabs out without releasing refs — refs transfer to the closed-tab stack.
  mgr.tabs = mgr.tabs.filter((t) => t.id === tabId || t.pinned)
  mgr.activeTabId = tabId
  for (const entry of toClose) {
    const closedTabEntry: ClosedTab = { tab: entry.snapshot, originalIndex: entry.originalIndex }
    transferSnapshotRefs(closedTabEntry, 'transfer')
    pushClosed(mgr, closedTabEntry, cap)
  }
}

/** Result of `reopenLastClosedTab`. */
export type ReopenResult = { reopened: TabId } | { reason: 'empty' | 'cap' }

/**
 * Pops the most-recently-closed tab and re-inserts it at its original index.
 * Refuses with `{ reason: 'cap' }` when the manager is at `maxTabs` (no pop, no mutation).
 * Refuses with `{ reason: 'empty' }` when the stack is empty.
 */
export function reopenLastClosedTab(mgr: TabManager, maxTabs: number): ReopenResult {
  if (mgr.closedStack.length === 0) {
    return { reason: 'empty' }
  }
  if (mgr.tabs.length >= maxTabs) {
    return { reason: 'cap' }
  }
  const entry = mgr.closedStack.pop()
  if (!entry) return { reason: 'empty' }
  const insertAt = Math.min(entry.originalIndex, mgr.tabs.length)
  // The snapshot is plain data; re-insert directly. Svelte's $state reactivity covers
  // the parent array, so mutations to nested fields work via the manager's accessors.
  mgr.tabs.splice(insertAt, 0, entry.tab)
  mgr.activeTabId = entry.tab.id
  return { reopened: entry.tab.id }
}

/** Trims the closed-tab stack to `cap` entries (drops oldest from the front).
 *  Evicted entries release their snapshot refs via `transferSnapshotRefs`. */
export function trimClosedStack(mgr: TabManager, cap: number): void {
  while (mgr.closedStack.length > cap) {
    const evicted = mgr.closedStack.shift()
    if (evicted) transferSnapshotRefs(evicted, 'release')
  }
}

/** Returns the number of entries on the closed-tab stack. */
export function getClosedStackSize(mgr: TabManager): number {
  return mgr.closedStack.length
}

/** Stores cursor filename on the old active tab, then activates the new tab.
 *  Returns false if the target tab ID doesn't exist (no-op in that case). */
export function switchTab(mgr: TabManager, tabId: TabId, cursorFilename: string | null): boolean {
  const targetTab = mgr.tabs.find((t) => t.id === tabId)
  if (!targetTab) {
    return false
  }
  const currentTab = mgr.tabs.find((t) => t.id === mgr.activeTabId)
  if (currentTab) {
    currentTab.cursorFilename = cursorFilename
  }
  mgr.activeTabId = tabId
  return true
}

/** Pins a tab */
export function pinTab(mgr: TabManager, tabId: TabId): void {
  const tab = mgr.tabs.find((t) => t.id === tabId)
  if (tab) {
    tab.pinned = true
  }
}

/** Unpins a tab */
export function unpinTab(mgr: TabManager, tabId: TabId): void {
  const tab = mgr.tabs.find((t) => t.id === tabId)
  if (tab) {
    tab.pinned = false
  }
}

/** Debounce state for cycleTab */
let cycleDebounceTimer: ReturnType<typeof setTimeout> | null = null
let cycleDebounceActiveTabId: TabId | null = null

const CYCLE_DEBOUNCE_MS = 50

/**
 * Cycles to the next or previous tab, wrapping around.
 * Uses leading-edge debounce (~50ms) so rapid cycling only commits the final tab.
 * Returns the new active tab ID.
 */
export function cycleTab(mgr: TabManager, direction: 'next' | 'prev', cursorFilename: string | null): TabId {
  const currentIndex = mgr.tabs.findIndex((t) => t.id === mgr.activeTabId)
  if (currentIndex === -1) {
    return mgr.activeTabId
  }

  const nextIndex =
    direction === 'next' ? (currentIndex + 1) % mgr.tabs.length : (currentIndex - 1 + mgr.tabs.length) % mgr.tabs.length

  const targetTabId = mgr.tabs[nextIndex].id

  // Leading-edge debounce: first call fires immediately, subsequent calls within
  // the debounce window are batched, and the final one fires after the timeout
  if (cycleDebounceTimer === null) {
    // First call: fire immediately
    switchTab(mgr, targetTabId, cursorFilename)
    cycleDebounceActiveTabId = targetTabId

    cycleDebounceTimer = setTimeout(() => {
      // After timeout, commit the last stored tab if it differs from what was already applied
      if (cycleDebounceActiveTabId !== null && cycleDebounceActiveTabId !== mgr.activeTabId) {
        switchTab(mgr, cycleDebounceActiveTabId, cursorFilename)
      }
      cycleDebounceTimer = null
      cycleDebounceActiveTabId = null
    }, CYCLE_DEBOUNCE_MS)
  } else {
    // Subsequent call within debounce window: just store the target
    cycleDebounceActiveTabId = targetTabId
  }

  return targetTabId
}

/** Resets the cycle debounce state. Useful for testing. */
export function resetCycleDebounce(): void {
  if (cycleDebounceTimer !== null) {
    clearTimeout(cycleDebounceTimer)
    cycleDebounceTimer = null
  }
  cycleDebounceActiveTabId = null
}

/** Returns all tabs */
export function getAllTabs(mgr: TabManager): TabState[] {
  return mgr.tabs
}

/** Returns the number of tabs */
export function getTabCount(mgr: TabManager): number {
  return mgr.tabs.length
}
