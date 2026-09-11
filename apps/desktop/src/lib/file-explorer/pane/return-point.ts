/**
 * A pane's return point: what it showed before a navigation committed ahead of its
 * listing, so a cancelled load can hand the pane back. `navigate.ts` owns the map of
 * these (one per pane, caller-owned like the transaction tokens) and the `{ returnTo }`
 * arm; this module keeps the shape and the pure rules. The cancel flow that reads it:
 * `pane/DETAILS.md` § "Escape during a load".
 */
import type { Location } from '$lib/tauri-commands'
import type { HistoryEntry, NavigationHistory } from '../navigation/navigation-history'
import type { TabId, TabState } from '../tabs/tab-types'

export interface ReturnPoint {
  /** The tab the commits ran in. A point never carries over into another tab. */
  tabId: TabId
  /** Where the tab was before the first commit, which is what the pane showed. */
  shown: Location
  /** The history entry that was current then, and its index, so the return lands on that same entry. */
  historyIndex: number
  entry: HistoryEntry
  /** Where the commits left the tab. Once the tab moves on without navigating (a pane swap), the point lapses. */
  ahead: Location
}

/** The point a commit about to run keeps: the one still in force, else the tab as it stands. */
export function pointBeforeCommit(tab: TabState, inForce: ReturnPoint | null): Omit<ReturnPoint, 'ahead'> {
  if (inForce) return inForce
  const { currentIndex, stack } = tab.history
  return {
    tabId: tab.id,
    shown: { volumeId: tab.volumeId, path: tab.path },
    historyIndex: currentIndex,
    entry: { ...stack[currentIndex] },
  }
}

/** `point` while `tab` still sits where the commits left it, else `null`. */
export function pointInForce(tab: TabState, point: ReturnPoint | undefined): ReturnPoint | null {
  if (!point || point.tabId !== tab.id) return null
  return point.ahead.volumeId === tab.volumeId && point.ahead.path === tab.path ? point : null
}

/**
 * Where the point's entry sits in `history` now, or `null` once it's gone. A walk leaves
 * the stack alone and a push only truncates after the current entry, so the entry
 * usually stays put. The history cap dropping entries off the front slides it down, and
 * a push after a Back can truncate it away.
 */
export function returnIndexIn(history: NavigationHistory, point: ReturnPoint): number | null {
  for (let i = Math.min(point.historyIndex, history.stack.length - 1); i >= 0; i--) {
    const entry = history.stack[i]
    if (entry.volumeId === point.entry.volumeId && entry.path === point.entry.path) return i
  }
  return null
}
