/**
 * How a query dialog's list grows: the first page, the step, the ceiling, and the one merge
 * rule that keeps rows already on screen where they are.
 *
 * Every `QueryDialog` consumer pages: a run asks for `RESULTS_PAGE_SIZE` rows, and when the
 * list's end comes into view the runner asks the SAME question again for `RESULTS_PAGE_STEP`
 * more (`query-runner.svelte.ts`). The answer comes back ranked from the top, so it's merged
 * rather than applied: rows already shown keep their order and only unseen ones are
 * appended. Reordering a list under somebody scrolling through it is the thing Decision 8
 * rules out, and a page must never take a row off the screen.
 */

import type { SearchResultEntry } from '#lib/tauri-commands/index.js'

/** The rows a run asks for first: what fills the dialog's list with some to spare. */
export const RESULTS_PAGE_SIZE = 30

/** How many more rows each page asks for once the list's end is in view. */
export const RESULTS_PAGE_STEP = 200

/**
 * The most rows a dialog list grows to. Mirrors the backend's `MAX_RESULT_ROWS`
 * (`crates/cmdr-search/src/types.rs`), which clamps every search's `limit` and matches the
 * pane's `SNAPSHOT_ENTRIES_CAP`; past it a page could only come back the same size.
 */
export const MAX_LISTED_RESULTS = 10_000

/** What a consumer is asked for: the run's question, answered with up to `limit` rows. */
export interface QueryPage {
  limit: number
}

/** A consumer's answer: the rows (ranked, from the top) and how many match in all. */
export interface QueryAnswer {
  entries: SearchResultEntry[]
  totalCount: number
}

/** The limit the next page asks for, given how many rows the list already shows. */
export function nextPageLimit(shown: number): number {
  return Math.min(shown + RESULTS_PAGE_STEP, MAX_LISTED_RESULTS)
}

/**
 * `rows` with every row of `incoming` it doesn't hold yet appended, in `incoming`'s order,
 * matched by PATH. Returns `rows` itself when nothing is new, so a caller can tell a page
 * that added nothing by identity.
 */
export function appendUnseen(rows: SearchResultEntry[], incoming: readonly SearchResultEntry[]): SearchResultEntry[] {
  if (incoming.length === 0) return rows
  const seen = new Set(rows.map((row) => row.path))
  const fresh: SearchResultEntry[] = []
  for (const entry of incoming) {
    if (seen.has(entry.path)) continue
    seen.add(entry.path)
    fresh.push(entry)
  }
  return fresh.length === 0 ? rows : [...rows, ...fresh]
}
