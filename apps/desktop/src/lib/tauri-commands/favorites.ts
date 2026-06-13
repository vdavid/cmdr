// User-editable favorites: add, remove, rename, reorder.
//
// These mutate the backend `favorites.json` store. The favorites LIST is not fetched here: it rides
// the `list_volumes` IPC and `volumes-changed` event as `LocationInfo` entries with
// `category: "favorite"` and `id: "fav-<favoriteId>"` (see `volume-store.svelte.ts`). Every mutation
// re-emits `volumes-changed`, so the switcher refreshes live with no manual refresh.

import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

/** The `fav-` prefix the backend puts on a favorite's `LocationInfo.id`. */
const FAVORITE_ID_PREFIX = 'fav-'

/**
 * Recovers the bare favorite id from a switcher `LocationInfo.id`. Favorites arrive as
 * `"fav-<favoriteId>"`; the remove / rename / reorder commands take the bare id, so strip the
 * prefix first. Returns the input unchanged when it carries no prefix (defensive: a caller that
 * already holds a bare id won't double-strip).
 */
export function stripFavoritePrefix(locationId: string): string {
  return locationId.startsWith(FAVORITE_ID_PREFIX) ? locationId.slice(FAVORITE_ID_PREFIX.length) : locationId
}

/**
 * Favorites `path`. When `name` is null the backend defaults the label to the path's file name.
 * Deduping by normalized path: re-adding an existing path moves it to the end and keeps its id.
 */
export async function addFavorite(path: string, name: string | null): Promise<void> {
  const res = await commands.addFavorite(path, name)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Removes a favorite by its bare id (strip the `fav-` prefix first via `stripFavoritePrefix`). */
export async function removeFavorite(id: string): Promise<void> {
  const res = await commands.removeFavorite(id)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Renames a favorite by its bare id. */
export async function renameFavorite(id: string, name: string): Promise<void> {
  const res = await commands.renameFavorite(id, name)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Persists the full favorites order. Pass bare ids in the desired display order. */
export async function reorderFavorites(orderedIds: string[]): Promise<void> {
  const res = await commands.reorderFavorites(orderedIds)
  if (res.status === 'error') throwIpcError(res.error)
}
