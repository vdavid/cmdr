/**
 * Navigation history management for browser-style back/forward navigation.
 * Each pane maintains its own independent history stack.
 *
 * The history works like browser history:
 * - push(): navigating to a new entry adds it to stack, truncates forward history
 * - back(): moves index backward in the stack
 * - forward(): moves index forward in the stack
 *
 * Each history entry contains the full navigation state: volume ID, path, and
 * optional network state (for the Network virtual volume).
 */

import type { NetworkHost } from '../types'
import type { Location } from '$lib/tauri-commands'

/**
 * Per-tab cap on the navigation stack. When `push()` would grow the stack past this
 * count, the oldest entries are dropped (FIFO) and returned in `droppedEntries` so
 * the caller can release any per-entry resources (search-results snapshot refs, in
 * the M8a wiring).
 *
 * 100 keeps a few sessions of casual browsing in reach while bounding memory: each
 * `HistoryEntry` is ~3 string fields, so even 100 entries per tab × 10 tabs × 2
 * panes is negligible. Tightening below 100 would start to hurt power users who
 * navigate deeply and rely on `⌘[` for orientation.
 */
export const MAX_HISTORY_PER_TAB = 100

/**
 * A single entry in the navigation history: a `Location` (volumeId + path) plus
 * the network host the entry was browsing, when on the network volume.
 * `HistoryEntry` is NOT persisted, so composing `Location` here has zero
 * serialization impact (the `networkHost` stays out of `Location`).
 */
export type HistoryEntry = Location & {
  /** For network volume: the network host (if browsing shares) */
  networkHost?: NetworkHost
}

export interface NavigationHistory {
  /** Stack of all visited entries */
  stack: HistoryEntry[]
  /** Current position in the stack (0 = oldest entry) */
  currentIndex: number
}

/**
 * Result of `push()`. `history` is the new stack; `droppedEntries` aggregates
 * everything `push()` removed in service of the call:
 *   - forward entries truncated when pushing after a `back()`
 *   - oldest entries evicted to honor `MAX_HISTORY_PER_TAB`
 *
 * Callers that don't care about released resources can ignore `droppedEntries`
 * (and most do; the search-results snapshot store is the only consumer today).
 * `pushPath` is one such caller — it discards the field and returns just the
 * history for backwards compatibility.
 */
export interface PushResult {
  history: NavigationHistory
  droppedEntries: HistoryEntry[]
}

/**
 * Creates a new history with the initial entry.
 */
export function createHistory(volumeId: string, path: string): NavigationHistory {
  return {
    stack: [{ volumeId, path }],
    currentIndex: 0,
  }
}

/** Compares two history entries for equality. */
function entriesEqual(a: HistoryEntry, b: HistoryEntry): boolean {
  if (a.volumeId !== b.volumeId || a.path !== b.path) {
    return false
  }
  // Compare network host if present
  const aHost = a.networkHost
  const bHost = b.networkHost
  if (!aHost && !bHost) return true
  if (!aHost || !bHost) return false
  return aHost.name === bHost.name && aHost.hostname === bHost.hostname
}

/**
 * Pushes a new entry to the history stack.
 *
 * - Truncates any forward history (entries after currentIndex) and reports the
 *   removed entries in `droppedEntries`.
 * - If the new entry equals the current entry, returns the unchanged history with
 *   an empty `droppedEntries` (reference-equal on `history` so callers using `===`
 *   dedup still work).
 * - Caps the stack at `MAX_HISTORY_PER_TAB`. Overflow evicts the **oldest** entries
 *   (front of the stack); evicted entries are included in `droppedEntries` and the
 *   `currentIndex` is adjusted so it still points at the same logical position.
 *
 * `droppedEntries` covers both kinds of removals; callers that care about released
 * resources (the search-results snapshot store, today) iterate the array once.
 */
export function push(history: NavigationHistory, entry: HistoryEntry): PushResult {
  const currentEntry = history.stack[history.currentIndex]
  if (entriesEqual(entry, currentEntry)) {
    return { history, droppedEntries: [] }
  }

  // 1. Truncate forward history (entries after currentIndex are dropped).
  const truncated = history.stack.slice(history.currentIndex + 1)
  // 2. Append the new entry.
  let newStack = [...history.stack.slice(0, history.currentIndex + 1), entry]
  // 3. Evict the oldest entries if the cap was exceeded.
  let evictedFromFront: HistoryEntry[] = []
  if (newStack.length > MAX_HISTORY_PER_TAB) {
    const overflow = newStack.length - MAX_HISTORY_PER_TAB
    evictedFromFront = newStack.slice(0, overflow)
    newStack = newStack.slice(overflow)
  }
  return {
    history: {
      stack: newStack,
      currentIndex: newStack.length - 1,
    },
    droppedEntries: [...evictedFromFront, ...truncated],
  }
}

/**
 * Convenience function to push just a path change (same volume).
 * Delegates to `push()` and returns only the new history (discards `droppedEntries`).
 * Callers that need to release per-entry resources should use `push()` directly.
 */
export function pushPath(history: NavigationHistory, path: string): NavigationHistory {
  const currentEntry = history.stack[history.currentIndex]
  return push(history, { volumeId: currentEntry.volumeId, path }).history
}

/**
 * Moves back in history. Returns the new history state.
 * If already at the oldest entry, returns unchanged history.
 */
export function back(history: NavigationHistory): NavigationHistory {
  if (!canGoBack(history)) {
    return history
  }
  return {
    ...history,
    currentIndex: history.currentIndex - 1,
  }
}

/**
 * Moves forward in history. Returns the new history state.
 * If already at the newest entry, returns unchanged history.
 */
export function forward(history: NavigationHistory): NavigationHistory {
  if (!canGoForward(history)) {
    return history
  }
  return {
    ...history,
    currentIndex: history.currentIndex + 1,
  }
}

/**
 * Gets the current entry in the history.
 */
export function getCurrentEntry(history: NavigationHistory): HistoryEntry {
  return history.stack[history.currentIndex]
}

/**
 * Gets the current path in the history (for backwards compatibility).
 */
export function getCurrentPath(history: NavigationHistory): string {
  return history.stack[history.currentIndex].path
}

/**
 * Gets the entry at a specific index in the history.
 * Returns undefined if index is out of bounds.
 */
export function getEntryAt(history: NavigationHistory, index: number): HistoryEntry | undefined {
  return history.stack[index]
}

/**
 * Returns true if there's history to go back to.
 */
export function canGoBack(history: NavigationHistory): boolean {
  return history.currentIndex > 0
}

/**
 * Returns true if there's history to go forward to.
 */
export function canGoForward(history: NavigationHistory): boolean {
  return history.currentIndex < history.stack.length - 1
}

/**
 * Sets the current index in the history. Used after resolving a path.
 * Clamps to valid range.
 */
export function setCurrentIndex(history: NavigationHistory, index: number): NavigationHistory {
  const clampedIndex = Math.max(0, Math.min(index, history.stack.length - 1))
  if (clampedIndex === history.currentIndex) {
    return history
  }
  return {
    ...history,
    currentIndex: clampedIndex,
  }
}
