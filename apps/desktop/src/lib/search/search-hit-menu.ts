/**
 * What a search hit contributes to the native context menu, on both surfaces that
 * show hits: the Search dialog's rows and a search-results pane.
 *
 * Every hit is a real file the drive index walked, so the items that act through
 * the row's path work on it: Share hands macOS file URLs, and a tag color writes
 * an xattr. A file pane asks `rowIsOsVisible` per row because its volume might be
 * a phone or an archive's insides; a hit never is.
 */
import type { PaneContextMenuFacts } from '$lib/tauri-commands'

export const SEARCH_HIT_MENU_FACTS = { canShare: true, canTag: true } as const satisfies PaneContextMenuFacts

/**
 * The Search dialog's facts for one hit. A folder hit is also favoritable: the index
 * only walks drives a favorite may point at.
 */
export function searchDialogRowMenuFacts(entry: { isDirectory: boolean }): PaneContextMenuFacts {
  return { ...SEARCH_HIT_MENU_FACTS, canFavorite: entry.isDirectory }
}
