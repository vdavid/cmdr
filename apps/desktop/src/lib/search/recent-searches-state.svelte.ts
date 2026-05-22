// Module-level reactive store for the recent-searches footer and popover.
//
// Both the footer (6 most recent) and the popover (fuzzy over the full set) read the same
// in-memory list. Loading is owned here so multiple consumers don't double-fetch from the
// backend on mount.

import { getRecentSearches, type HistoryEntry } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('search')

// Newest first. Source of truth for the dialog while it's open.
let entries = $state<HistoryEntry[]>([])
let loaded = $state(false)
let loading = $state(false)

/** Returns the in-memory recent-search list, newest first. */
export function getRecentSearchesList(): HistoryEntry[] {
  return entries
}

/** Whether `loadRecentSearches()` has completed at least once this session. */
export function getRecentSearchesLoaded(): boolean {
  return loaded
}

/**
 * Test-only reset hook. Restores the module to its post-construction state so each test
 * starts from a clean slate (the module is a singleton).
 */
export function resetRecentSearchesForTests(): void {
  entries = []
  loaded = false
  loading = false
}

/** Replaces the in-memory list, for example after a remove/clear hand-off from the IPC. */
export function setRecentSearchesList(next: HistoryEntry[]): void {
  entries = next
  loaded = true
}

/**
 * Loads the persisted recent searches from the backend. Idempotent: subsequent calls in the
 * same session are no-ops unless `force` is set. The dialog calls this on mount so the
 * footer renders immediately.
 */
export async function loadRecentSearches(force = false): Promise<void> {
  if (loaded && !force) return
  if (loading) return
  loading = true
  try {
    entries = await getRecentSearches()
    loaded = true
  } catch (error) {
    log.warn('Failed to load recent searches: {error}', { error })
  } finally {
    loading = false
  }
}
