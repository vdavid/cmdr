/**
 * Builds the IPC payload for the `searchFiles` backend call.
 *
 * Search-only: Selection has its own JS matcher (no IPC round-trip) and an
 * independent helper. Layered on top of the core `QueryFilterState`'s
 * `buildBaseSearchQuery()` so the size + date predicates stay in one place;
 * this file only layers the `excludeSystemDirs` flag (Search-only field, lives
 * in the Search extras module).
 */

import type { SearchQuery } from '$lib/tauri-commands'
import type { QueryFilterState } from '$lib/query-ui/query-filter-state.svelte'
import type { SearchExtrasState } from './search-extras-state.svelte'

/**
 * Builds a `SearchQuery` from the current Search state. AI mode goes through
 * `translateSearchQuery` first and then populates state before this is called.
 * The backend `patternType` is derived from `mode`: filename maps to glob, regex
 * to regex. AI mode (which only reaches here after AI translation flipped the
 * mode) maps to glob as a safe default.
 */
export function buildSearchQuery(core: QueryFilterState, extras: SearchExtrasState): SearchQuery {
  const q = core.buildBaseSearchQuery()
  if (!extras.getExcludeSystemDirs()) {
    q.excludeSystemDirs = false
  }
  if (extras.getCountOnly()) {
    q.countOnly = true
  }
  return q
}
