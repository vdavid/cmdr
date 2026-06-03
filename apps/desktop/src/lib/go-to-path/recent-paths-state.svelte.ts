/**
 * `$state` mirror of the backend recent-paths store. The Go-to-path dialog
 * reads `getList()` to render up to 10 recent rows, and calls `add` / `remove`
 * to mutate both the backend (via IPC) and the local list in lock-step so the
 * UI updates without re-fetching.
 *
 * The backend is the source of truth for dedupe (by resolved path), move-to-top,
 * and the cap of 10. To keep the mirror honest, `add` and `remove` re-read the
 * authoritative list from the backend after the write rather than guessing the
 * new order locally.
 *
 * Modeled on `search/recent-searches-state.svelte.ts`, but simpler: a single
 * singleton store (only one Go-to-path dialog ever exists) with its own
 * add/remove mutators instead of the generic recent-items factory.
 */

import { commands, type RecentPathEntry } from '$lib/ipc/bindings'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('go-to-path')

let entries = $state<RecentPathEntry[]>([])
let loaded = $state(false)
let loading = $state(false)

/** Returns the in-memory recent-path list, newest first. */
export function getRecentPathsList(): RecentPathEntry[] {
  return entries
}

/** Whether `loadRecentPaths()` has completed at least once this session. */
export function getRecentPathsLoaded(): boolean {
  return loaded
}

/**
 * Loads the persisted recent paths from the backend. Idempotent: subsequent
 * calls in the same session are no-ops unless `force` is set. The dialog calls
 * this on open so the rows render immediately.
 */
export async function loadRecentPaths(force = false): Promise<void> {
  if (loaded && !force) return
  if (loading) return
  loading = true
  try {
    entries = await commands.getRecentPaths()
    loaded = true
  } catch (error) {
    log.warn('Failed to load recent paths: {error}', { error })
  } finally {
    loading = false
  }
}

/**
 * Adds a resolved target to recents (backend dedupes by path, moves to top,
 * caps at 10), then refreshes the local mirror. Best-effort: a failed write
 * leaves the mirror untouched.
 */
export async function addRecentPath(entry: RecentPathEntry): Promise<void> {
  const result = await commands.addRecentPath(entry)
  if (result.status === 'error') {
    log.warn('Failed to add recent path {path}: {error}', { path: entry.path, error: result.error })
    return
  }
  entries = await commands.getRecentPaths()
  loaded = true
}

/**
 * Removes a recent entry by id (the `[x]` button), then refreshes the local
 * mirror. Best-effort: a failed write leaves the mirror untouched.
 */
export async function removeRecentPath(id: string): Promise<void> {
  const result = await commands.removeRecentPath(id)
  if (result.status === 'error') {
    log.warn('Failed to remove recent path {id}: {error}', { id, error: result.error })
    return
  }
  entries = await commands.getRecentPaths()
  loaded = true
}

/**
 * Test-only reset hook. Restores the singleton to its post-construction state
 * so each test starts from a clean slate.
 */
export function resetRecentPathsForTests(): void {
  entries = []
  loaded = false
  loading = false
}
