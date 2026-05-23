/**
 * Selection-side instantiation of the recent-items factory store, plus the
 * pure `applySelectionHistoryEntry` helper that restores a recent entry into
 * the live Selection state.
 *
 * The store mirrors `lib/search/recent-searches-state.svelte.ts` — same shape,
 * different IPC family — so the Selection dialog drops into the same recent-items
 * footer + popover. Selection has no scope and no exclude-system-dirs, so the
 * apply helper writes a narrower set of fields than Search's
 * `applyHistoryEntry`.
 */

import { getRecentSelections, type SelectionHistoryEntry } from '$lib/tauri-commands'
import { createRecentItemsState } from '$lib/query-ui/recent-items/recent-items-state.svelte'
import type { QueryFilterState } from '$lib/query-ui/query-filter-state.svelte'

/**
 * Recent-selections store. Same shape as Search's `recentSearchesStore`. Wraps
 * the IPC binding in a thunk so test mocks that omit `getRecentSelections`
 * don't throw at import time.
 */
export const recentSelectionsStore = createRecentItemsState<SelectionHistoryEntry>({
  getRecent: () => getRecentSelections(),
})

const store = recentSelectionsStore

/** Returns the in-memory recent-selections list, newest first. */
export function getRecentSelectionsList(): SelectionHistoryEntry[] {
  return store.getList()
}

/** Replaces the in-memory list (e.g. after a remove/clear hand-off from IPC). */
export function setRecentSelectionsList(next: SelectionHistoryEntry[]): void {
  store.setList(next)
}

/**
 * Loads the persisted recent-selections from the backend. Idempotent: subsequent
 * calls in the same session are no-ops unless `force` is set. The dialog calls
 * this on mount so the footer renders immediately.
 */
export async function loadRecentSelections(force = false): Promise<void> {
  await store.load(force)
}

/**
 * Applies a recent-selection entry into the live core state. Pure (apart from
 * the state writes the caller asked for). Restores: `query`, `mode`,
 * `caseSensitive`, size + date filters via the core's `applyHistoryFilters`.
 *
 * Selection's entry shape lacks `scope` and `excludeSystemDirs` — those are
 * Search-only fields — so the helper doesn't write them.
 *
 * AI entries: `entry.query` carries the original natural-language prompt (the
 * Selection dialog persists the prompt as `query` for AI entries, matching
 * Search's convention). The dialog's recent-activate handler typically also
 * sets `runOnMount` so the dialog re-translates; that's the caller's job, not
 * this helper's.
 */
export function applySelectionHistoryEntry(state: QueryFilterState, entry: SelectionHistoryEntry): void {
  state.setMode(entry.mode)
  state.setQuery(entry.query)
  state.setCaseSensitive(entry.caseSensitive)
  state.applyHistoryFilters(entry.filters)
  // Mirror the typed query into the matching hand-typed buffer so a later
  // mode switch doesn't wipe it (Search's `applyHistoryEntry` does the same).
  state.clearHandTypedBuffers()
  state.setHandTypedBuffer(entry.mode, entry.query)
}
