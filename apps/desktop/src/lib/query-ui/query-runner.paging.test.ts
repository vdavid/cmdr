/**
 * The run controller's paging: the list grows when its end comes into view, by asking the
 * last run's question again for more rows (`result-paging.ts`).
 *
 * What these pin is that a page only ever ADDS: rows on screen keep their order, the cursor
 * stays on its row, a page answering for a list that's gone is dropped, and a live walk's
 * own batches keep landing on top without duplicating what a page brought.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { SearchResultEntry } from '#lib/tauri-commands/index.js'
import { clearAllToasts, getToasts } from '#lib/ui/toast/toast-store.svelte.js'
import type { QueryDialogConfig } from './query-dialog-config'
import { createQueryRunner, type QueryRunner } from './query-runner.svelte'
import type { QueryStreamCallbacks, QueryStreamProgress, QueryStreamSource } from './query-stream'
import { RESULTS_PAGE_SIZE, RESULTS_PAGE_STEP, type QueryAnswer, type QueryPage } from './result-paging'
import { makeQueryDialogConfig, sampleEntries } from './test-helpers'

interface PagingHarness {
  runner: QueryRunner
  config: QueryDialogConfig
  runLimits: number[]
  pageLimits: number[]
}

/** A data source of `total` ranked rows that answers each ask with its top `limit`. */
function rankedSource(total: number) {
  const all = sampleEntries(total)
  return (page: QueryPage): Promise<QueryAnswer> =>
    Promise.resolve({ entries: all.slice(0, page.limit), totalCount: total })
}

function makePagingRunner(
  opts: {
    runQuery?: (page: QueryPage) => Promise<QueryAnswer>
    loadMore?: (page: QueryPage) => Promise<QueryAnswer | null>
    streamingSource?: QueryStreamSource
  } = {},
): PagingHarness {
  const harness = { runLimits: [], pageLimits: [] } as unknown as PagingHarness
  const source = rankedSource(1_000)
  const runQuery = opts.runQuery ?? source
  const loadMore = opts.loadMore ?? source
  const config = makeQueryDialogConfig({
    runQuery: (page) => {
      harness.runLimits.push(page.limit)
      return runQuery(page)
    },
    loadMore: (page) => {
      harness.pageLimits.push(page.limit)
      return loadMore(page)
    },
    streamingSource: opts.streamingSource,
  })
  config.state.setQuery('report')
  harness.config = config
  harness.runner = createQueryRunner({
    getConfig: () => config,
    isAutoApplyEnabled: () => true,
    scrollCursorIntoView: () => {},
  })
  return harness
}

const paths = (rows: SearchResultEntry[]): string[] => rows.map((row) => row.path)

beforeEach(() => {
  clearAllToasts()
})

describe('paging a one-shot run', () => {
  it('asks a run for one page, and one step more when the end comes into view', async () => {
    const h = makePagingRunner()
    await h.runner.executeQuery()
    expect(h.runLimits).toEqual([RESULTS_PAGE_SIZE])
    expect(h.config.state.getResults()).toHaveLength(RESULTS_PAGE_SIZE)

    h.runner.loadMoreResults()
    await vi.waitFor(() => {
      expect(h.config.state.getResults()).toHaveLength(RESULTS_PAGE_SIZE + RESULTS_PAGE_STEP)
    })
    expect(h.pageLimits).toEqual([RESULTS_PAGE_SIZE + RESULTS_PAGE_STEP])
    expect(h.config.state.getTotalCount()).toBe(1_000)
  })

  it('lists a path once when the answer names it twice', async () => {
    // The list keys its rows by path, and a repeated key throws the whole dialog away
    // (ERR-XVTB2). A stale index can answer with one path under two rows, so the answer
    // is held to the rule the live batches and the pages already keep.
    const all = sampleEntries(3)
    const h = makePagingRunner({
      runQuery: () => Promise.resolve({ entries: [all[0], all[1], { ...all[0] }, all[2]], totalCount: 4 }),
    })
    await h.runner.executeQuery()
    expect(paths(h.config.state.getResults())).toEqual(paths(all))
  })

  it('keeps the rows on screen in their order and the cursor on its row', async () => {
    // The page answers ranked from the top, in an order that differs from the screen's.
    const all = sampleEntries(100)
    const h = makePagingRunner({
      runQuery: () => Promise.resolve({ entries: all.slice(0, 3), totalCount: 100 }),
      loadMore: () => Promise.resolve({ entries: [all[2], all[0], all[1], ...all.slice(3)], totalCount: 100 }),
    })
    await h.runner.executeQuery()
    h.config.state.setCursorIndex(2)
    h.runner.loadMoreResults()
    await vi.waitFor(() => {
      expect(h.config.state.getResults()).toHaveLength(100)
    })
    expect(paths(h.config.state.getResults())).toEqual(paths(all))
    expect(h.config.state.getCursorIndex()).toBe(2)
  })

  it('asks once per page, however often the end is reported meanwhile', async () => {
    const h = makePagingRunner()
    await h.runner.executeQuery()
    h.runner.loadMoreResults()
    h.runner.loadMoreResults()
    h.runner.loadMoreResults()
    await vi.waitFor(() => {
      expect(h.config.state.getResults()).toHaveLength(RESULTS_PAGE_SIZE + RESULTS_PAGE_STEP)
    })
    expect(h.pageLimits).toHaveLength(1)
  })

  it('asks nothing once every match is on screen', async () => {
    const h = makePagingRunner({ runQuery: rankedSource(12), loadMore: rankedSource(12) })
    await h.runner.executeQuery()
    expect(h.runner.hasMoreResults).toBe(false)
    h.runner.loadMoreResults()
    await Promise.resolve()
    expect(h.pageLimits).toEqual([])
  })

  it('stops asking after a page that added nothing', async () => {
    const h = makePagingRunner({ loadMore: () => Promise.resolve(null) })
    await h.runner.executeQuery()
    h.runner.loadMoreResults()
    await vi.waitFor(() => {
      expect(h.runner.hasMoreResults).toBe(false)
    })
    h.runner.loadMoreResults()
    await Promise.resolve()
    expect(h.pageLimits).toHaveLength(1)
  })

  it('drops a page that answers after a newer run started', async () => {
    let answerPage: (answer: QueryAnswer) => void = () => {}
    const h = makePagingRunner({
      loadMore: () =>
        new Promise((resolve) => {
          answerPage = resolve
        }),
    })
    await h.runner.executeQuery()
    h.runner.loadMoreResults()
    await vi.waitFor(() => {
      expect(h.pageLimits).toHaveLength(1)
    })
    h.config.state.setQuery('report-2026')
    await h.runner.executeQuery()

    answerPage({ entries: sampleEntries(500).slice(400), totalCount: 500 })
    await Promise.resolve()
    expect(h.config.state.getResults()).toHaveLength(RESULTS_PAGE_SIZE)
    expect(h.config.state.getTotalCount()).toBe(1_000)
  })

  it('says why a page failed, once, and keeps the rows', async () => {
    const h = makePagingRunner({ loadMore: () => Promise.reject(new Error('Index went away')) })
    await h.runner.executeQuery()
    h.runner.loadMoreResults()
    await vi.waitFor(() => {
      expect(getToasts()).toHaveLength(1)
    })
    expect(h.config.state.getResults()).toHaveLength(RESULTS_PAGE_SIZE)
    h.runner.loadMoreResults()
    await Promise.resolve()
    expect(h.pageLimits).toHaveLength(1)
  })
})

describe('a one-shot answer that arrives late', () => {
  it('leaves a newer run’s list and spinner alone', async () => {
    let answerRun: (answer: QueryAnswer) => void = () => {}
    const started: QueryStreamCallbacks[] = []
    const h = makePagingRunner({
      runQuery: () =>
        new Promise((resolve) => {
          answerRun = resolve
        }),
      streamingSource: {
        start: (_runId, callbacks) => {
          started.push(callbacks)
          return Promise.resolve(() => {})
        },
        cancel: () => {},
      },
    })
    // The debounce's one-shot is out when the user presses Enter, which walks.
    const debounced = h.runner.executeQuery({ fromAutoApply: true })
    h.runner.run()
    await vi.waitFor(() => {
      expect(started).toHaveLength(1)
    })
    started[0].onProgress(liveBatch(sampleEntries(2), 2))

    answerRun({ entries: sampleEntries(30), totalCount: 30 })
    await debounced
    expect(h.config.state.getResults()).toHaveLength(2)
    expect(h.config.state.getIsSearching()).toBe(true)
  })

  it('cannot bring rows or a spinner back after the bar was emptied', async () => {
    let answerRun: (answer: QueryAnswer) => void = () => {}
    const h = makePagingRunner({
      runQuery: () =>
        new Promise((resolve) => {
          answerRun = resolve
        }),
    })
    const out = h.runner.executeQuery()
    h.config.state.setQuery('')
    h.runner.scheduleSearch()
    expect(h.config.state.getIsSearching()).toBe(false)

    answerRun({ entries: sampleEntries(30), totalCount: 30 })
    await out
    expect(h.config.state.getResults()).toEqual([])
    expect(h.config.state.getIsSearching()).toBe(false)
  })
})

function liveBatch(entries: SearchResultEntry[], matchCount: number): QueryStreamProgress {
  return { phase: 'walking', entries, matchCount, dirsFound: 1, currentPath: null, capped: false }
}

describe('paging under a live walk', () => {
  async function startLive(h: PagingHarness, started: QueryStreamCallbacks[]): Promise<QueryStreamCallbacks> {
    h.runner.run()
    await vi.waitFor(() => {
      expect(started).toHaveLength(1)
    })
    return started[0]
  }

  function liveRunner(loadMore: (page: QueryPage) => Promise<QueryAnswer | null>) {
    const started: QueryStreamCallbacks[] = []
    const h = makePagingRunner({
      loadMore,
      streamingSource: {
        start: (_runId, callbacks) => {
          started.push(callbacks)
          return Promise.resolve(() => {})
        },
        cancel: () => {},
        rankOnCompletion: (entries) => [...entries].reverse(),
      },
    })
    return { h, started }
  }

  it('appends the index’s page, keeps the run’s own count, and keeps the walk’s batches coming', async () => {
    const all = sampleEntries(300)
    const { h, started } = liveRunner(() => Promise.resolve({ entries: all.slice(0, 200), totalCount: 200 }))
    const run = await startLive(h, started)
    run.onProgress(liveBatch(all.slice(0, 30), 900))

    h.runner.loadMoreResults()
    await vi.waitFor(() => {
      expect(h.config.state.getResults()).toHaveLength(200)
    })
    // The index can't count what the walk hasn't written yet: the run's counter stays.
    expect(h.config.state.getTotalCount()).toBe(900)

    // A batch holding a row the page already brought adds only the new one.
    run.onProgress(liveBatch([all[150], all[250]], 901))
    expect(h.config.state.getResults()).toHaveLength(201)
    expect(h.config.state.getResults().at(-1)?.path).toBe(all[250].path)
  })

  it('leaves the order alone at the end once a page has grown the list', async () => {
    const all = sampleEntries(100)
    const { h, started } = liveRunner(() => Promise.resolve({ entries: all, totalCount: 100 }))
    const run = await startLive(h, started)
    run.onProgress(liveBatch(all.slice(0, 30), 100))
    h.runner.loadMoreResults()
    await vi.waitFor(() => {
      expect(h.config.state.getResults()).toHaveLength(100)
    })

    run.onEnd({ matchCount: 100, incomplete: false, walked: true, capped: false })
    expect(paths(h.config.state.getResults())).toEqual(paths(all))
  })
})
