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

import type { NetworkHost } from './types'

/** A single entry in the navigation history */
export interface HistoryEntry {
    /** The volume ID (e.g., 'root', 'network', '/Volumes/MyDrive') */
    volumeId: string
    /** The path within the volume (or 'smb://' for network root) */
    path: string
    /** For network volume: the selected host (if browsing shares) */
    networkHost?: NetworkHost
}

export interface NavigationHistory {
    /** Stack of all visited entries */
    stack: HistoryEntry[]
    /** Current position in the stack (0 = oldest entry) */
    currentIndex: number
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
 * Truncates any forward history (entries after currentIndex).
 * If the new entry is the same as the current entry, returns unchanged history.
 */
export function push(history: NavigationHistory, entry: HistoryEntry): NavigationHistory {
    const currentEntry = history.stack[history.currentIndex]
    if (entriesEqual(entry, currentEntry)) {
        return history
    }

    // Truncate forward history and add the new entry
    const newStack = [...history.stack.slice(0, history.currentIndex + 1), entry]
    return {
        stack: newStack,
        currentIndex: newStack.length - 1,
    }
}

/**
 * Convenience function to push just a path change (same volume).
 */
export function pushPath(history: NavigationHistory, path: string): NavigationHistory {
    const currentEntry = history.stack[history.currentIndex]
    return push(history, { volumeId: currentEntry.volumeId, path })
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
