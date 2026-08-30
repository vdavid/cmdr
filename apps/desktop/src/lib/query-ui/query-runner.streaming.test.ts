/**
 * The run controller's streaming path: a query whose answer arrives over time.
 *
 * The one that matters most is the GENERATION GUARD. A refined query supersedes its
 * predecessor without cancelling the work behind it, so a batch belonging to the old
 * run can still land after the new one started. Appending it would splice results
 * from a query the user has moved on from into the list they're reading: no error, no
 * warning, just wrong rows. Everything else here (append, the cursor held by path,
 * the completion re-rank and its suppression, cancel) is what makes that list usable
 * while it grows.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { SearchResultEntry } from '$lib/tauri-commands'
import { clearAllToasts, getToasts } from '$lib/ui/toast/toast-store.svelte'
import type { QueryDialogConfig } from './query-dialog-config'
import { createQueryRunner, type QueryRunner } from './query-runner.svelte'
import type { QueryStreamCallbacks, QueryStreamEnd, QueryStreamProgress, QueryStreamSource } from './query-stream'
import { makeQueryDialogConfig, sampleEntries } from './test-helpers'

interface StreamHarness {
  runner: QueryRunner
  config: QueryDialogConfig
  /** The callbacks the runner handed the source, per run id, in start order. */
  started: { runId: string; callbacks: QueryStreamCallbacks }[]
  cancelled: string[]
  stopped: number
  rankCalls: number
}

function entriesNamed(...names: string[]): SearchResultEntry[] {
  return names.map((name) => ({
    name,
    path: `/Users/test/${name}`,
    parentPath: '/Users/test',
    isDirectory: false,
    size: 10,
    modifiedAt: 1_700_000_000,
    iconId: 'ext:txt',
  }))
}

function progress(overrides: Partial<QueryStreamProgress> = {}): QueryStreamProgress {
  return {
    phase: 'walking',
    entries: [],
    matchCount: 0,
    dirsFound: 0,
    currentPath: null,
    capped: false,
    ...overrides,
  }
}

function ended(overrides: Partial<QueryStreamEnd> = {}): QueryStreamEnd {
  return { matchCount: 0, incomplete: false, walked: true, capped: false, ...overrides }
}

function makeStreamRunner(
  sourceOverrides: Partial<QueryStreamSource> = {},
  configOverrides: Partial<QueryDialogConfig> = {},
): StreamHarness {
  const harness: StreamHarness = { started: [], cancelled: [], stopped: 0, rankCalls: 0 } as unknown as StreamHarness

  const streamingSource: QueryStreamSource = {
    start: (runId, callbacks) => {
      harness.started.push({ runId, callbacks })
      return Promise.resolve(() => {
        harness.stopped += 1
      })
    },
    cancel: (runId) => {
      harness.cancelled.push(runId)
    },
    rankOnCompletion: (entries) => {
      harness.rankCalls += 1
      return [...entries].reverse()
    },
    ...sourceOverrides,
  }

  const config = makeQueryDialogConfig({ streamingSource, ...configOverrides })
  config.state.setQuery('report')
  harness.config = config
  harness.runner = createQueryRunner({
    getConfig: () => config,
    isAutoApplyEnabled: () => true,
    scrollCursorIntoView: () => {},
  })
  return harness
}

/** Starts a run and waits for the source's `start` to have been handed its callbacks. */
async function startRun(h: StreamHarness): Promise<QueryStreamCallbacks> {
  const before = h.started.length
  h.runner.run()
  await vi.waitFor(() => {
    expect(h.started.length).toBe(before + 1)
  })
  return h.started[h.started.length - 1].callbacks
}

beforeEach(() => {
  clearAllToasts()
})

describe('the generation guard', () => {
  it('drops a superseded run’s batch instead of splicing it into the new run', async () => {
    const h = makeStreamRunner()
    const first = await startRun(h)
    first.onProgress(progress({ entries: entriesNamed('old-a.txt'), matchCount: 1 }))
    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['old-a.txt'])

    // The user refines the query. The backend keeps the old walk going (superseding is
    // not cancelling), so its batches are still in flight.
    h.config.state.setQuery('report-2026')
    const second = await startRun(h)
    expect(h.started[0].runId).not.toBe(h.started[1].runId)

    first.onProgress(progress({ entries: entriesNamed('old-b.txt'), matchCount: 2 }))
    second.onProgress(progress({ entries: entriesNamed('new-a.txt'), matchCount: 1 }))

    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['new-a.txt'])
    expect(h.config.state.getTotalCount()).toBe(1)
  })

  it('drops a superseded run’s terminal event, count and all', async () => {
    const h = makeStreamRunner()
    const first = await startRun(h)
    h.config.state.setQuery('report-2026')
    const second = await startRun(h)
    second.onProgress(progress({ entries: entriesNamed('new-a.txt'), matchCount: 1 }))

    first.onEnd(ended({ matchCount: 999, incomplete: true }))

    expect(h.config.state.getTotalCount()).toBe(1)
    expect(h.runner.live?.running).toBe(true)
    expect(h.runner.live?.incomplete).toBe(false)
  })

  it('unsubscribes the run it replaced', async () => {
    const h = makeStreamRunner()
    await startRun(h)
    expect(h.stopped).toBe(0)
    await startRun(h)
    expect(h.stopped).toBe(1)
    // ❌ Superseding must not cancel: the walk carries on, and its ground reaches the
    // index for the very next query.
    expect(h.cancelled).toEqual([])
  })
})

describe('appending, and the cursor', () => {
  it('appends batches in arrival order and keeps the count live', async () => {
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onProgress(progress({ phase: 'readingIndex', entries: entriesNamed('a.txt', 'b.txt'), matchCount: 2 }))
    run.onProgress(progress({ entries: entriesNamed('c.txt'), matchCount: 3 }))

    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['a.txt', 'b.txt', 'c.txt'])
    expect(h.config.state.getTotalCount()).toBe(3)
  })

  it('holds the cursor on its own row as rows arrive under it', async () => {
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onProgress(progress({ entries: entriesNamed('a.txt', 'b.txt'), matchCount: 2 }))
    h.config.state.setCursorIndex(1)
    h.config.state.setLastDialogEvent('cursor-moved')

    run.onProgress(progress({ entries: entriesNamed('c.txt'), matchCount: 3 }))

    expect(h.config.state.getCursorIndex()).toBe(1)
    expect(h.config.state.getResults()[1].name).toBe('b.txt')
  })

  it('re-ranks once on completion, and carries the cursor to its row’s new index', async () => {
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onProgress(progress({ entries: entriesNamed('a.txt', 'b.txt', 'c.txt'), matchCount: 3 }))
    run.onEnd(ended({ matchCount: 3 }))

    expect(h.rankCalls).toBe(1)
    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['c.txt', 'b.txt', 'a.txt'])
    // The cursor sat on the first row, which the re-rank moved to the end.
    expect(h.config.state.getCursorIndex()).toBe(2)
  })

  it('leaves the order alone once the user has moved the cursor', async () => {
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onProgress(progress({ entries: entriesNamed('a.txt', 'b.txt'), matchCount: 2 }))
    h.config.state.setLastDialogEvent('cursor-moved')
    run.onEnd(ended({ matchCount: 2 }))

    expect(h.rankCalls).toBe(0)
    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['a.txt', 'b.txt'])
  })

  it('leaves the index’s own ranking alone when nothing was walked', async () => {
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onProgress(progress({ phase: 'readingIndex', entries: entriesNamed('a.txt', 'b.txt'), matchCount: 2 }))
    run.onEnd(ended({ matchCount: 2, walked: false }))

    expect(h.rankCalls).toBe(0)
    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['a.txt', 'b.txt'])
  })
})

describe('the live view the dialog renders', () => {
  it('starts on the coverage phase and follows the run through', async () => {
    const h = makeStreamRunner()
    expect(h.runner.live).toBeNull()
    const run = await startRun(h)
    expect(h.runner.live).toMatchObject({ phase: 'resolvingCoverage', running: true, matchCount: 0 })

    run.onProgress(progress({ phase: 'walking', matchCount: 9, dirsFound: 120, currentPath: '/Volumes/naspi' }))
    expect(h.runner.live).toMatchObject({
      phase: 'walking',
      matchCount: 9,
      dirsFound: 120,
      currentPath: '/Volumes/naspi',
    })

    run.onEnd(ended({ matchCount: 9 }))
    expect(h.runner.live).toMatchObject({ running: false, incomplete: false, matchCount: 9 })
    expect(h.config.state.getIsSearching()).toBe(false)
  })

  it('keeps the rows and marks the answer a lower bound when the run is stopped', async () => {
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onProgress(progress({ entries: entriesNamed('a.txt'), matchCount: 1 }))

    expect(h.runner.cancelLive()).toBe(true)
    expect(h.cancelled).toEqual([h.started[0].runId])
    // The end state is the backend's word, not an optimistic local flip.
    run.onEnd(ended({ matchCount: 1, incomplete: true }))

    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['a.txt'])
    expect(h.runner.live).toMatchObject({ running: false, incomplete: true })
    expect(h.config.state.getTotalCount()).toBe(1)
  })

  it('has nothing to cancel once the run is over', async () => {
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onEnd(ended())
    expect(h.runner.cancelLive()).toBe(false)
    expect(h.cancelled).toEqual([])
  })

  it('has nothing left to stop on a second ask, so a run that never answers cannot trap the dialog', async () => {
    const h = makeStreamRunner()
    await startRun(h)
    const runId = h.started[0].runId

    // A run that never reaches a terminal event: no progress, no end, no failure.
    // The stop reaches the backend, but nothing comes back to flip `running`.
    expect(h.runner.cancelLive()).toBe(true)
    expect(h.cancelled).toEqual([runId])

    // The second ask reports "nothing left to stop", which is what lets Escape's
    // two-step move on to closing. Answering `true` forever left the dialog
    // un-closable by keyboard, and in the E2E suite one such run cascaded into
    // every later test on the shard.
    expect(h.runner.cancelLive()).toBe(false)
    expect(h.cancelled).toEqual([runId])
  })

  it('stops the NEXT run on its own first ask', async () => {
    const h = makeStreamRunner()
    await startRun(h)
    expect(h.runner.cancelLive()).toBe(true)
    expect(h.runner.cancelLive()).toBe(false)

    await startRun(h)
    expect(h.runner.cancelLive()).toBe(true)
    expect(h.cancelled).toEqual([h.started[0].runId, h.started[1].runId])
  })

  it('surfaces a run that could not run at all, and clears the spinner', async () => {
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onFailed('Query too broad. Add a filename pattern, size, date, or type filter.')

    expect(String(getToasts()[0].content)).toContain('Query too broad')
    expect(h.config.state.getIsSearching()).toBe(false)
    expect(h.runner.live).toBeNull()
  })

  it('surfaces a refused start (a scope spanning two volumes) the same way', async () => {
    const h = makeStreamRunner({
      start: () => Promise.reject(new Error('A search covers one volume at a time.')),
    })
    h.runner.run()
    await vi.waitFor(() => {
      expect(getToasts()).toHaveLength(1)
    })
    expect(String(getToasts()[0].content)).toContain('one volume at a time')
    expect(h.config.state.getIsSearching()).toBe(false)
    expect(h.runner.live).toBeNull()
  })
})

describe('auto-apply never walks (Decision 7)', () => {
  it('takes the plain one-shot path on the debounce, and the streaming one on Enter', async () => {
    let plainRuns = 0
    const h = makeStreamRunner(
      {},
      {
        runQuery: () => {
          plainRuns += 1
          return Promise.resolve({ entries: sampleEntries(2), totalCount: 2 })
        },
      },
    )

    await h.runner.executeQuery({ fromAutoApply: true })
    expect(plainRuns).toBe(1)
    expect(h.started).toHaveLength(0)
    expect(h.runner.live).toBeNull()

    await startRun(h)
    expect(plainRuns).toBe(1)
  })

  it('clears the spinner when the bar is emptied mid-walk', async () => {
    // A live run has no promise whose `finally` clears `isSearching`: dropping its
    // subscription is the last anyone hears of it. Without the reset the dialog sits on
    // "Searching…" forever, with nothing coming and no way back.
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onProgress(progress({ entries: entriesNamed('a.txt'), matchCount: 1 }))
    expect(h.config.state.getIsSearching()).toBe(true)

    h.config.state.setQuery('')
    h.runner.scheduleSearch()

    expect(h.config.state.getIsSearching()).toBe(false)
    expect(h.runner.live).toBeNull()
    expect(h.config.state.getResults()).toEqual([])
  })

  it('drops a live run’s view when a plain run replaces it', async () => {
    const h = makeStreamRunner()
    const run = await startRun(h)
    run.onProgress(progress({ entries: entriesNamed('a.txt'), matchCount: 1 }))

    await h.runner.executeQuery({ fromAutoApply: true })

    expect(h.runner.live).toBeNull()
    expect(h.stopped).toBe(1)
    expect(h.cancelled).toEqual([])
  })
})

describe('adopting a run that outlived the last dialog', () => {
  it('reports nothing to adopt when the source has no run to hand over', () => {
    const h = makeStreamRunner()
    expect(h.runner.resumeLive()).toBe(false)
    expect(h.runner.live).toBeNull()
  })

  it('picks the run up where it was, rows included', () => {
    // Search's "Open in pane" leaves a walk feeding that pane. Reopening the dialog
    // has to show THAT search: starting a fresh one would supersede it, and the pane
    // would quietly stop growing.
    // Collected rather than assigned to a `let`: the compiler can't see a callback
    // run, so a nullable local would narrow to `null` at every later read.
    const resumed: QueryStreamCallbacks[] = []
    let stops = 0
    const h = makeStreamRunner({
      resume: (callbacks) => {
        resumed.push(callbacks)
        return {
          runId: 'handed-over',
          view: {
            phase: 'walking',
            matchCount: 12,
            dirsFound: 400,
            currentPath: '/w/deep',
            capped: false,
            phaseSince: 0,
            running: true,
            incomplete: false,
          },
          missedEntries: entriesNamed('found-while-closed.txt'),
          stop: () => {
            stops += 1
          },
        }
      },
    })

    expect(h.runner.resumeLive()).toBe(true)
    expect(h.runner.live?.running).toBe(true)
    expect(h.runner.live?.dirsFound).toBe(400)
    // The count and the list have to tell the same story: without the missed rows
    // the dialog would claim 12 matches over an empty list.
    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['found-while-closed.txt'])
    expect(h.config.state.getTotalCount()).toBe(12)
    expect(h.config.state.getIsSearching()).toBe(true)
    expect(h.runner.hasSearched).toBe(true)

    // And it's a real adoption, not a copy: later batches land, and Escape can stop it.
    const adopted = resumed.at(-1)
    if (!adopted) throw new Error('resume was never handed the callbacks')
    adopted.onProgress(progress({ entries: entriesNamed('later.txt'), matchCount: 13 }))
    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['found-while-closed.txt', 'later.txt'])
    expect(h.runner.cancelLive()).toBe(true)
    expect(h.cancelled).toEqual(['handed-over'])

    h.runner.dispose()
    expect(stops).toBe(1)
  })

  it('drops the adopted run when the user asks a new question', async () => {
    const h = makeStreamRunner({
      resume: (callbacks) => ({
        runId: 'handed-over',
        view: {
          phase: 'walking',
          matchCount: 1,
          dirsFound: 1,
          currentPath: null,
          capped: false,
          phaseSince: 0,
          running: true,
          incomplete: false,
        },
        missedEntries: [],
        stop: () => {
          void callbacks
        },
      }),
    })
    h.runner.resumeLive()
    const fresh = await startRun(h)
    expect(h.started[0].runId).not.toBe('handed-over')
    fresh.onProgress(progress({ entries: entriesNamed('new.txt'), matchCount: 1 }))
    expect(h.config.state.getResults().map((e) => e.name)).toEqual(['new.txt'])
  })
})
