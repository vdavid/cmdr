/**
 * Search's side of the shared recent-items dropdown: how a `HistoryEntry` renders as a
 * row, and what picking or removing one does.
 *
 * The adapter is the only seam where Search-specific fields (`scope`,
 * `excludeSystemDirs`, `caseSensitive`) leak into a row's meta line and tooltip;
 * Selection's wrapper passes its own against its narrower entry shape.
 */

import { removeRecentSearch, getRecentSearches, type HistoryEntry } from '$lib/tauri-commands'
import { tString } from '$lib/intl/messages.svelte'
import { chipTooltip, modeName, formatAge, rowMeta } from '$lib/query-ui/recent-items/recent-items-utils'
import type { RecentItemAdapter, RecentItemKey } from '$lib/query-ui/recent-items/recent-items-types'
import { getRecentSearchesList, setRecentSearchesList } from './recent-searches-state.svelte'
import { applyHistoryEntry } from './search-state.svelte'

export const searchRecentAdapter: RecentItemAdapter<HistoryEntry> = (entry) => ({
  label: entry.query,
  tooltip: chipTooltip(entry),
  mode: entry.mode,
  ageLabel: formatAge(entry.timestamp),
  metaLabel: rowMeta(entry),
  ariaLabel: tString('search.recent.runAria', { mode: modeName(entry.mode), query: entry.query }),
})

export const searchRecentKey: RecentItemKey<HistoryEntry> = (entry) => entry.id

/**
 * Recent-search pick: loads the history entry's query, mode, and filters into the live
 * dialog and stops there. It deliberately does NOT set `runOnMount`: picking is
 * navigation, so the user lands back in the field with the search ready to tweak, and
 * an AI entry never re-translates (and re-bills) on a keystroke. QueryDialog closes the
 * dropdown, refocuses the field, and hands `⏎` back to "run-search".
 */
export function activateHistoryEntry(entry: HistoryEntry): void {
  applyHistoryEntry(entry)
}

/** Removes a recent search entry; backend write is async, we update the cache eagerly. */
export function removeHistoryEntry(entry: HistoryEntry): void {
  setRecentSearchesList(getRecentSearchesList().filter((e) => e.id !== entry.id))
  void removeRecentSearch(entry.id).then(async () => {
    try {
      setRecentSearchesList(await getRecentSearches())
    } catch {
      // Already fell back to the optimistic snapshot; nothing to do.
    }
  })
}
