// Search-side instantiation of the recent-items factory store (see
// `lib/query-ui/recent-items/recent-items-state.svelte.ts`). Search wires the factory
// with the search-history IPC family; Selection wires its own.
//
// We export the named functions the rest of `lib/search/` imports
// (`getRecentSearchesList`, `loadRecentSearches`, …) so call sites stay shallow.

import { getRecentSearches, type HistoryEntry } from '$lib/tauri-commands'
import { createRecentItemsState } from '$lib/query-ui/recent-items/recent-items-state.svelte'

// Wrap the IPC binding in a thunk so the factory doesn't deref the binding at module-init
// time. Test mocks (`vi.mock('$lib/tauri-commands', ...)`) that omit `getRecentSearches`
// would otherwise throw at import time; the thunk pushes the lookup to first call.
//
// Exported as `recentSearchesStore` so `SearchDialog.svelte` can hand the underlying
// reactive store straight to `QueryDialog`'s `historyStore` prop without re-wrapping
// the getter/setter surface. The named helpers below stay around because the rest of
// `lib/search/` still imports them through the named-export API.
export const recentSearchesStore = createRecentItemsState<HistoryEntry>({
  getRecent: () => getRecentSearches(),
})
const store = recentSearchesStore

/** Returns the in-memory recent-search list, newest first. */
export function getRecentSearchesList(): HistoryEntry[] {
  return store.getList()
}

/** Whether `loadRecentSearches()` has completed at least once this session. */
export function getRecentSearchesLoaded(): boolean {
  return store.getLoaded()
}

/**
 * Test-only reset hook. Restores the underlying store to its post-construction state so each
 * test starts from a clean slate (the module is a singleton).
 */
export function resetRecentSearchesForTests(): void {
  store.resetForTests()
}

/** Replaces the in-memory list, for example after a remove/clear hand-off from the IPC. */
export function setRecentSearchesList(next: HistoryEntry[]): void {
  store.setList(next)
}

/**
 * Loads the persisted recent searches from the backend. Idempotent: subsequent calls in the
 * same session are no-ops unless `force` is set. The dialog calls this on mount so the
 * footer renders immediately.
 */
export async function loadRecentSearches(force = false): Promise<void> {
  await store.load(force)
}
