/**
 * Two contracts of Search's live transport:
 *
 * - The order a live run carries to the backend, which silences every dialog run older
 *   than the one registering. Two starts can reach it in either order, so the order has
 *   to say which question came later even when both starts land in the same clock tick.
 * - `onRunState`, the run "Open in pane" may hand off. It must name the run the dialog
 *   is FOLLOWING and nothing else: a run the dialog stopped listening to (a newer run, an
 *   index answer replacing it) never reports its ending here, so a stale name is a pane
 *   waiting forever on a run nobody follows.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { LiveRunHandlers } from './live-run-events'

vi.mock('#lib/tauri-commands/index.js', () => ({
  cancelSearch: vi.fn(),
  searchFilesStreaming: vi.fn(),
}))

/** Every run the source is listening to, by id, so a test can speak for the backend. */
const { listening, resumeMock } = vi.hoisted(() => ({
  listening: new Map<string, LiveRunHandlers>(),
  resumeMock: vi.fn(),
}))

vi.mock('./live-run-events', () => ({
  liveViewOf: (event: { matchCount: number }) => ({ matchCount: event.matchCount, running: true }),
  observeSearchRun: (runId: string, handlers: LiveRunHandlers) => {
    listening.set(runId, handlers)
    return Promise.resolve(() => listening.delete(runId))
  },
}))

vi.mock('./walk-handoff.svelte', () => ({
  resumeHandedOffWalk: resumeMock,
  supersedeHandedOffWalk: vi.fn(),
}))

import { createLiveSearchSource, nextRunOrder } from './live-search-source'
import type { LiveRunView, QueryStreamCallbacks } from '#lib/query-ui/query-stream.js'
import type { SearchRunCoverage } from '#lib/tauri-commands/index.js'

describe('nextRunOrder', () => {
  it('gives two starts in the same clock tick different, increasing orders', () => {
    // Far past any clock an earlier test read, so the stamp starts from this tick.
    const tick = 4_000_000_000_000
    const first = nextRunOrder(tick)
    const second = nextRunOrder(tick)

    expect(first).toBe(tick * 1000)
    expect(second).toBe(first + 1)
  })

  it('follows the clock once it moves past the last order handed out', () => {
    const earlier = nextRunOrder(5_000_000_000_000)
    const later = nextRunOrder(5_000_000_000_001)

    expect(later).toBe(5_000_000_000_001 * 1000)
    expect(later).toBeGreaterThan(earlier)
  })
})

type RunState = { runId: string; view: Partial<LiveRunView> } | null

const callbacks: QueryStreamCallbacks = { onProgress: () => {}, onEnd: () => {}, onFailed: () => {} }

function progress(matchCount: number) {
  return {
    phase: 'walking' as const,
    targetVolumeId: 'root',
    entries: [],
    matchCount,
    dirsFound: 0,
    currentPath: null,
    capped: false,
  }
}

function sourceRecordingRunState(): { source: ReturnType<typeof createLiveSearchSource>; states: RunState[] } {
  const states: RunState[] = []
  const source = createLiveSearchSource({
    buildQuery: () => Promise.resolve({} as never),
    onCoverage: () => {},
    rank: (entries) => entries,
    onRowsVolume: () => {},
    onRunState: (state) => {
      states.push(state as RunState)
    },
  })
  return { source, states }
}

describe('onRunState names only the run the dialog follows', () => {
  beforeEach(() => {
    listening.clear()
    resumeMock.mockReset().mockReturnValue(null)
  })

  it('clears when the dialog stops listening, though the run never said it ended', async () => {
    const { source, states } = sourceRecordingRunState()
    const stop = await source.start('run-a', callbacks)
    expect(states.at(-1)?.runId).toBe('run-a')

    stop()
    expect(states.at(-1)).toBeNull()
  })

  it('clears when the run ends', async () => {
    const { source, states } = sourceRecordingRunState()
    await source.start('run-a', callbacks)
    listening.get('run-a')?.onProgress(progress(4))
    expect(states.at(-1)?.view.matchCount).toBe(4)

    listening.get('run-a')?.onSettled(4, { walk: 'completed' } as SearchRunCoverage)
    expect(states.at(-1)).toBeNull()
  })

  it('ignores a run it has moved on from, including its teardown', async () => {
    const { source, states } = sourceRecordingRunState()
    const stopA = await source.start('run-a', callbacks)
    await source.start('run-b', callbacks)

    listening.get('run-a')?.onProgress(progress(9))
    listening.get('run-a')?.onSettled(9, { walk: 'completed' } as SearchRunCoverage)
    stopA()
    expect(states.at(-1)?.runId).toBe('run-b')
  })

  it('forgets the run when an index answer replaces it, before its start has even resolved', async () => {
    const { source, states } = sourceRecordingRunState()
    const starting = source.start('run-a', callbacks)
    source.forgetRun()
    expect(states.at(-1)).toBeNull()

    // The run's own words after that change nothing: its rows aren't on screen.
    await starting
    listening.get('run-a')?.onProgress(progress(2))
    expect(states.at(-1)).toBeNull()
  })

  it('follows a walk a reopened dialog adopts, until it detaches', () => {
    const resumedStop = vi.fn()
    resumeMock.mockReturnValue({ runId: 'run-h', view: { matchCount: 7 }, missedEntries: [], stop: resumedStop })
    const { source, states } = sourceRecordingRunState()

    const resumed = source.resume?.(callbacks)
    expect(states.at(-1)).toEqual({ runId: 'run-h', view: { matchCount: 7 } })

    resumed?.stop()
    expect(resumedStop).toHaveBeenCalledTimes(1)
    expect(states.at(-1)).toBeNull()
  })
})
