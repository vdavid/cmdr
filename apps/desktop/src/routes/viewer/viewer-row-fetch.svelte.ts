/**
 * The viewer's fetch walk: turning "these rows are on screen" into `viewer_get_lines`
 * calls, and the answers back into cached rows.
 *
 * It owns the walk and nothing else. Geometry (how tall a row is, where the render
 * window sits) and the row store itself (the cache, its eviction) live in
 * `viewer-scroll.svelte.ts`, which creates this and reaches it through getters. The seam
 * is deliberate: a fetch is a sequence of ASKS, each shaped by what the last answer said,
 * and mixing it with pixel math is how the two stopped being separately testable.
 *
 * ❗ A SHORT ANSWER IS NORMAL. One `viewer_get_lines` carries at most
 * `CHUNK_BUDGET_BYTES`, so a range of long rows comes back in several pieces. Every step
 * of the walk reads what the chunk SAYS (`chunk.end`, `chunk.firstRowNumber`,
 * `chunk.endByteOffset`), ❌ never what its row count implies.
 */

import { viewerGetLines, asViewerError, type LineChunk, type ViewerRow } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { pluralize } from '$lib/utils/pluralize'

const log = getAppLogger('viewer')

/** The most rows one `viewer_get_lines` is ever asked for. */
export const FETCH_BATCH = 500

const FETCH_DEBOUNCE_MS = 100

/** Where a follow-up request picks the walk back up. */
interface ContinueFrom {
  /** The row the next chunk starts at, for the "is the range covered yet" test. */
  row: number
  /**
   * The exact source byte to resume at, or `null` to resume by row index.
   *
   * ❗ Which one is right depends on who owns the numbering. A backend that can seek by
   * ROW hands a row back as itself, so continuing by row keeps the cache's indexes
   * exactly where the earlier chunks put them. A backend that can't (`byteSeek` with no
   * index) derives a row index from a byte offset through its bytes-per-row sample, so
   * the BYTE is the fact and the row number is the estimate: resume from the byte and
   * cache wherever the answer says.
   */
  byteOffset: number | null
}

/**
 * What the walk needs from the scroll composable around it. All getters, so the walk
 * always reads live geometry rather than a value captured when it was created.
 */
export interface RowFetchDeps {
  getSessionId: () => string
  /** Counted rows, or `null` while only an estimate exists. */
  getTotalRows: () => number | null
  /** Counted rows, or the current estimate when there is no count yet. */
  getEstimatedTotalRows: () => number
  /** The render window's first row. */
  getVisibleFrom: () => number
  /** One past the render window's last row. */
  getVisibleTo: () => number
  /**
   * Rows to fetch beyond each end of the render window: the SAME pixel budget the render
   * window spends, so a file of tall rows doesn't ask for a hundred of them to sit
   * off-screen.
   */
  getPrefetchRows: () => number
  /** Whether the row store already holds a row. */
  hasRow: (row: number) => boolean
  /** The row store's writer: caches a chunk's rows and evicts what's now far away. */
  writeRows: (firstRow: number, rows: ViewerRow[]) => void
  /** Adopts a counted row total, preserving the scroll position across the change. */
  setTotalRows: (rows: number) => void
  onTimeoutError: () => void
}

export function createViewerRowFetch(deps: RowFetchDeps) {
  let fetchDebounceTimer: ReturnType<typeof setTimeout> | undefined
  let currentFetchId = 0

  /**
   * The first row the backend answered nothing for, with the row count it answered
   * under. Past that row there is nothing to fetch, so asking again would be the spin
   * this guards: `needsFetch` would stay true on a row that doesn't exist and a refetch
   * would fire every `FETCH_DEBOUNCE_MS` forever. The recorded total is the expiry: a
   * reload, an encoding switch, or a tail append moves it, and the row may exist then.
   *
   * ❗ Only a SUCCESSFUL answer of zero rows sets this. A read that failed throws, and a
   * failure must stay retryable.
   */
  let noRowsBeyond: { row: number; underTotal: number } | null = null

  /** One past the last row worth asking for: the backend has nothing at or after it. */
  function fetchableTo(): number {
    if (noRowsBeyond === null) return Infinity
    if (noRowsBeyond.underTotal !== deps.getEstimatedTotalRows()) {
      noRowsBeyond = null
      return Infinity
    }
    return noRowsBeyond.row
  }

  function needsFetch(from: number, to: number): boolean {
    const limit = fetchableTo()
    const samplesToCheck = [from, Math.floor((from + to) / 2), to - 1]
    for (const row of samplesToCheck) {
      if (row >= 0 && row < limit && !deps.hasRow(row)) {
        return true
      }
    }
    return false
  }

  /**
   * Drops what we knew about where the file ends. The row store calls it when it clears,
   * because a cleared cache and a remembered end-of-file describe different files.
   */
  function forgetFileEnd() {
    noRowsBeyond = null
  }

  function scheduleFetch(from: number, to: number) {
    if (fetchDebounceTimer) {
      clearTimeout(fetchDebounceTimer)
    }
    fetchDebounceTimer = setTimeout(() => {
      void fetchRows(from, to)
    }, FETCH_DEBOUNCE_MS)
  }

  /**
   * What to ask the backend for to fill the rendered range `[from, to)`, or `null` when that
   * range is empty (past the end of the file, or no rows yet): nothing to draw, so nothing to
   * fetch. The count is always positive, which `viewer_get_lines`'s `usize` insists on.
   *
   * `startAt` continues a range an earlier chunk left unfinished: it starts the request
   * where that chunk ended rather than at the range's own start, so the chain always moves
   * forward.
   */
  function rowRequest({
    from,
    to,
    startAt,
  }: {
    from: number
    to: number
    startAt?: ContinueFrom
  }): { seekType: 'row' | 'byte' | 'fraction'; seekValue: number; fetchFrom: number; fetchCount: number } | null {
    if (to <= from) return null
    const prefetch = deps.getPrefetchRows()
    const fetchFrom = startAt?.row ?? Math.max(0, from - prefetch)
    if (fetchFrom >= to + prefetch) return null
    const fetchCount = Math.min(FETCH_BATCH, to - fetchFrom + prefetch * 2)
    if (fetchCount <= 0) return null
    if (startAt?.byteOffset != null) {
      return { seekType: 'byte', seekValue: startAt.byteOffset, fetchFrom, fetchCount }
    }
    if (deps.getTotalRows() !== null) return { seekType: 'row', seekValue: fetchFrom, fetchFrom, fetchCount }
    // A 0 estimate would make the fraction division NaN (0/0) or Infinity (>0/0); both
    // serialize to JSON null, which the Rust f64 `targetValue` rejects. With no row count
    // to go on, seek to the start of the file (fraction 0).
    const estimated = deps.getEstimatedTotalRows()
    return { seekType: 'fraction', seekValue: estimated > 0 ? fetchFrom / estimated : 0, fetchFrom, fetchCount }
  }

  /**
   * Fills the rendered range `[from, to)` from the backend, continuing from `startAt` when
   * an earlier answer stopped short.
   */
  async function fetchRows(from: number, to: number, startAt?: ContinueFrom) {
    const sessionId = deps.getSessionId()
    if (!sessionId) return
    const request = rowRequest({ from, to, startAt })
    if (!request) return
    const { seekType, seekValue, fetchCount } = request

    const fetchId = ++currentFetchId

    try {
      log.debug('fetchRows[{fetchId}]: requesting {seekType}={seekValue} count={fetchCount}', {
        fetchId,
        seekType,
        seekValue,
        fetchCount,
      })

      const chunk = await viewerGetLines(sessionId, seekType, seekValue, fetchCount)

      if (fetchId !== currentFetchId) {
        log.debug('fetchRows[{fetchId}]: discarding stale response (current={currentFetchId})', {
          fetchId,
          currentFetchId,
        })
        return
      }

      const continueAt = cacheChunk({ chunk, request, fetchId })
      // `currentFetchId` again: a newer fetch may have started while this answer was in
      // flight (the user scrolled), and its range is the one worth continuing, not ours.
      if (continueAt !== null && continueAt.row < to && fetchId === currentFetchId) {
        await fetchRows(from, to, continueAt)
      }
    } catch (e) {
      reportFetchFailure(e, fetchId)
    }
  }

  /**
   * Caches a chunk's rows and says where a follow-up request should pick up, or `null` when
   * this answer finished the job.
   *
   * ❗ Two facts the chunk STATES, neither of which may be inferred:
   * - `chunk.firstRowNumber` is where the rows go. Caching at the row the request asked
   *   for instead put a `byteSeek` continuation's rows at indexes its own next answer
   *   disagreed with, so the same bytes could be drawn twice at different heights.
   * - `chunk.end` says whether to continue. "Fewer rows than I asked for" is NOT the same
   *   question: `CHUNK_BUDGET_BYTES` makes a short chunk ordinary, and a full-length chunk
   *   can still have stopped on the budget. Reading a short chunk as the end of the file
   *   silently truncates; reading a budget-capped chunk as finished leaves the range
   *   unfilled and the fetch effect re-firing on it every debounce.
   */
  function cacheChunk({
    chunk,
    request,
    fetchId,
  }: {
    chunk: LineChunk
    request: NonNullable<ReturnType<typeof rowRequest>>
    fetchId: number
  }): ContinueFrom | null {
    const received = chunk.rows.length
    const cacheStartRow = chunk.firstRowNumber

    log.debug(
      'fetchRows[{fetchId}]: received {rowCount} {rowsNoun} at row {firstRow} ({end}), asked from {askedFrom}',
      {
        fetchId,
        rowCount: received,
        rowsNoun: pluralize(received, 'row'),
        firstRow: cacheStartRow,
        end: chunk.end,
        askedFrom: request.fetchFrom,
      },
    )

    deps.writeRows(cacheStartRow, chunk.rows)

    if (chunk.totalRows.kind === 'exact' && chunk.totalRows.rows !== deps.getTotalRows()) {
      deps.setTotalRows(chunk.totalRows.rows)
    }

    if (received === 0) {
      // The backend has nothing here: the file ends before this row, whatever the row
      // count claims. Remember it so the effect stops asking for a row that isn't there.
      noRowsBeyond = { row: request.fetchFrom, underTotal: deps.getEstimatedTotalRows() }
      return null
    }
    if (chunk.end === 'endOfFile') {
      // ❗ The chunk just NAMED the end of the file, so the row past its last one is the
      // first that will never arrive. Recording it here is what stops `needsFetch` from
      // staying true on a row count that overshoots what the backend emits (the
      // `lineIndex` phantom trailing row, or a `byteSeek` estimate), which would refire
      // the fetch every debounce forever.
      noRowsBeyond = { row: cacheStartRow + received, underTotal: deps.getEstimatedTotalRows() }
      return null
    }
    if (chunk.end !== 'budgetReached') return null
    return {
      row: cacheStartRow + received,
      byteOffset: request.seekType === 'row' ? null : chunk.endByteOffset,
    }
  }

  function reportFetchFailure(e: unknown, fetchId: number) {
    if (fetchId !== currentFetchId) return
    // `viewerGetLines` throws the backend's typed `ViewerError` with its fields copied onto
    // the Error, so the timeout is a VARIANT, never a flag beside a sentence.
    const kind = asViewerError(e)?.kind
    if (kind === 'timedOut') {
      deps.onTimeoutError()
      // The window shows the timeout with Retry, so it's a handled outcome: a warn. An
      // error log counts toward an auto-sent error report.
      log.warn('fetchRows[{fetchId}]: timed out', { fetchId })
    } else {
      log.error("fetchRows[{fetchId}]: didn't come back ({reason})", { fetchId, reason: kind ?? String(e) })
    }
  }

  /** Fetches whatever the render window is missing. Runs as an effect on the page. */
  function runFetchEffect() {
    const from = deps.getVisibleFrom()
    const to = deps.getVisibleTo()
    const sessionId = deps.getSessionId()
    if (sessionId && needsFetch(from, to)) {
      scheduleFetch(from, to)
    }
  }

  /**
   * Force a fetch of the current visible range, bypassing the cache check. Used
   * when the cache was deliberately invalidated (encoding switch) so the next
   * render shows freshly-decoded rows without waiting for the user to scroll.
   */
  function fetchVisibleNow() {
    const sessionId = deps.getSessionId()
    if (!sessionId) return
    // Whatever we knew about where the file ends belongs to the old decoding of it.
    noRowsBeyond = null
    void fetchRows(deps.getVisibleFrom(), deps.getVisibleTo())
  }

  function destroy() {
    if (fetchDebounceTimer) clearTimeout(fetchDebounceTimer)
  }

  return {
    needsFetch,
    forgetFileEnd,
    runFetchEffect,
    fetchVisibleNow,
    destroy,
  }
}
