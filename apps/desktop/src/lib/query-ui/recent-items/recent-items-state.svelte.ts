// Factory for the recent-items reactive store. Replaces the M2-era module singleton
// `lib/search/recent-searches-state.svelte.ts`.
//
// Both the footer (6 most recent) and the popover (fuzzy over the full set) read from the
// same in-memory list. Loading is owned by the store so multiple consumers don't double-fetch
// from the backend on mount.
//
// Two consumers (Search + Selection) each construct their own instance with their own set
// of IPC functions, so one dialog's history can't leak into the other.

import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('search')

/**
 * IPC surface a consumer must provide to construct a recent-items store. The shapes mirror
 * Search's existing tauri-commands (`getRecentSearches`, `addRecentSearch`, …) so Search's
 * wrapper can pass them through unchanged.
 */
export interface RecentItemsIPC<E> {
  getRecent: () => Promise<E[]>
}

/**
 * The reactive store returned by `createRecentItemsState`. Getters surface state for `$derived`
 * consumers; mutators are the hand-off path for remove/clear flows so the local list stays in
 * sync without round-tripping through `getRecent`.
 */
export interface RecentItemsStore<E> {
  /** Returns the in-memory list, newest first. */
  getList: () => E[]
  /** Whether `load()` has completed at least once this session. */
  getLoaded: () => boolean
  /** Test-only reset hook so tests start from a clean slate. */
  resetForTests: () => void
  /** Replace the in-memory list, e.g. after a remove/clear hand-off from the IPC. */
  setList: (next: E[]) => void
  /**
   * Loads the persisted entries from the backend. Idempotent: subsequent calls in the same
   * session are no-ops unless `force` is set.
   */
  load: (force?: boolean) => Promise<void>
}

/**
 * Constructs a recent-items store bound to a specific set of IPC functions. Search wires the
 * `getRecentSearches`-family; Selection (M5+) will wire the `getRecentSelections`-family.
 */
export function createRecentItemsState<E>(ipc: RecentItemsIPC<E>): RecentItemsStore<E> {
  let entries = $state<E[]>([])
  let loaded = $state(false)
  let loading = $state(false)

  return {
    getList: () => entries,
    getLoaded: () => loaded,
    resetForTests: () => {
      entries = []
      loaded = false
      loading = false
    },
    setList: (next) => {
      entries = next
      loaded = true
    },
    load: async (force = false) => {
      if (loaded && !force) return
      if (loading) return
      loading = true
      try {
        entries = await ipc.getRecent()
        loaded = true
      } catch (error) {
        log.warn('Failed to load recent items: {error}', { error })
      } finally {
        loading = false
      }
    },
  }
}
