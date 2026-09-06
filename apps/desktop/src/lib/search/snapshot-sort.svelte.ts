/**
 * Ordering a search-results pane's rows: the tri-state a column header cycles
 * through, and the round trip that turns it into an order.
 *
 * ## Why the comparator lives in Rust
 *
 * A snapshot pane's rows sort the same way a directory listing's do, and the only
 * way to guarantee that is to run the SAME comparator
 * (`file_system::listing::sorting::entry_comparator`, reached through the
 * `sort_search_results` command). Natural number ordering, case folding,
 * directories first, and the user's `directorySortMode` come along for free, and
 * there is no second copy to drift. A frontend comparator would have had to
 * reproduce `alphanumeric_sort`'s leading-zero and non-ASCII rules by hand, which
 * is exactly the kind of near-copy that goes quietly wrong.
 *
 * ## Why the store's mutator stays synchronous
 *
 * The round trip lives here; `snapshot-store::applySnapshotSort` is synchronous.
 * Every consumer of a snapshot pane resolves the index the user sees against
 * `snapshot.entries[i]`, so that array must be complete at every moment, never
 * half-reordered.
 *
 * See `DETAILS.md` § "The snapshot pane's row order".
 */

import { sortSearchResults } from '$lib/tauri-commands'
import { getDirectorySortMode } from '$lib/settings/reactive-settings.svelte'
import type { SearchResultEntry, SortColumn } from '$lib/ipc/bindings'
import { applySnapshotSort, getRankedEntries, getSnapshot, type SnapshotSort } from './snapshot-store.svelte'

/**
 * The order a column takes on its FIRST click, mirroring what a normal pane does
 * (`file-explorer/types.ts::defaultSortOrders`). Duplicated rather than imported
 * so this module stays a leaf of the search subsystem; `snapshot-sort.svelte.test.ts`
 * pins the values, and `search-i18n-parity`-style drift here would show as a
 * changed first click, not a silent wrong order.
 */
const DEFAULT_ORDERS: Record<SortColumn, SnapshotSort['order']> = {
  name: 'ascending',
  extension: 'ascending',
  size: 'descending',
  modified: 'descending',
  created: 'descending',
}

/**
 * The next state when the user asks to sort by `column`: a tri-state cycle.
 *
 * A fresh column takes its default order, the same column flips, and a third
 * click on it goes back to `null`, the engine's ranked order. Ranked is a real
 * state rather than a corner the user can't get back to: for an AI-mode result
 * set that ordering IS the answer, so losing it to a stray header click would
 * cost the user the thing they searched for.
 *
 * Pure, and shared by the header click and the keyboard sort commands, so the two
 * cycle identically.
 */
export function nextSnapshotSort(current: SnapshotSort | null, column: SortColumn): SnapshotSort | null {
  if (current === null || current.column !== column) return { column, order: DEFAULT_ORDERS[column] }
  if (current.order === DEFAULT_ORDERS[column]) {
    return { column, order: current.order === 'ascending' ? 'descending' : 'ascending' }
  }
  return null
}

/** The subset of a row that decides its place. See Rust's `SearchSortRow`. */
function sortRowsFor(entries: readonly SearchResultEntry[]) {
  return entries.map((e) => ({
    name: e.name,
    isDirectory: e.isDirectory,
    size: e.size,
    modifiedAt: e.modifiedAt,
  }))
}

/**
 * Tracks which sort request is the live one per snapshot, so a slow answer to an
 * earlier click can't land on top of a later one. Plain module state: a snapshot
 * id is per-session and the map only ever holds ids a sort was asked for.
 */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping for in-flight requests; nothing renders from it
const latestRequest = new Map<string, number>()
let nextRequest = 1

/**
 * Puts snapshot `id` in `sort`'s order, or back in the engine's ranked order when
 * `sort` is `null` (which needs no round trip at all — the ranked rows are kept).
 *
 * Two things can go stale between asking and answering, and each has its own
 * response. A LATER sort request supersedes this one, so this answer is dropped.
 * The rows themselves CHANGING (a still-running walk appending, a delete purging)
 * invalidates the order rather than the intent, so the request is re-run against
 * the rows that exist now. That loop turns over only while the rows keep moving,
 * and a walk that stops ends it.
 */
export async function sortSnapshot(id: string, sort: SnapshotSort | null): Promise<void> {
  const request = nextRequest++
  latestRequest.set(id, request)
  if (sort === null) {
    applySnapshotSort(id, null, [])
    return
  }
  for (;;) {
    const ranked = getRankedEntries(id)
    if (!ranked) return
    const order = await sortSearchResults(sortRowsFor(ranked), sort.column, sort.order, getDirectorySortMode())
    if (latestRequest.get(id) !== request) return
    // Reference identity is the generation token: every store mutator REPLACES
    // the array, so an unchanged reference proves the rows held still.
    if (getRankedEntries(id) !== ranked) continue
    applySnapshotSort(id, sort, order)
    return
  }
}

/**
 * Re-places a sorted snapshot's rows after new ones arrived, and does nothing at
 * all when the pane is showing the ranked order (those rows already landed at the
 * tail).
 *
 * ❗ Every caller of `appendSnapshotEntries` must follow it with this, or a sorted
 * pane silently stops growing. It lives here rather than inside the append so the
 * store keeps no dependency on the IPC layer, and so the append itself stays
 * synchronous.
 */
export async function resortSnapshotIfSorted(id: string): Promise<void> {
  const sort = getSnapshot(id)?.sort
  if (!sort) return
  await sortSnapshot(id, sort)
}
