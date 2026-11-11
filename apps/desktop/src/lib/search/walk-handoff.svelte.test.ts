/**
 * The walk that outlives its dialog, and the toast that is the only thing on screen
 * saying so.
 *
 * "Open in pane" is the one case where a search keeps running with its dialog gone.
 * From there the pane is the answer, and the toast is the whole interface: it says
 * the search is still going, it counts what's arriving, and it's the way back in.
 * The state machine below is what stops it lying — a toast that stays "still
 * searching" over a finished walk, or vanishes while one is running, is worse than
 * no toast at all.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { SearchQuery, SearchResultEntry, SearchRunCoverage } from '#lib/tauri-commands/index.js'
import { clearAllToasts, getToasts } from '#lib/ui/toast/toast-store.svelte.js'
import type { LiveRunView } from '#lib/query-ui/query-stream.js'
import type { LiveRunHandlers } from './live-run-events'
import { WALK_HANDOFF_TOAST_ID, getWalkHandoff } from './walk-handoff-state.svelte'
import {
  _resetWalkHandoffForTesting,
  handOffWalk,
  resumeHandedOffWalk,
  supersedeHandedOffWalk,
} from './walk-handoff.svelte'
import {
  _resetForTesting as resetSnapshots,
  getOrCreate,
  getSnapshot,
  incrementRef,
  SNAPSHOT_ENTRIES_CAP,
} from './snapshot-store.svelte'

/** The run the module is listening to, captured so a test can drive it. */
let observed: { runId: string; handlers: LiveRunHandlers } | null = null
const cancelled: string[] = []

vi.mock('./live-run-events', () => ({
  observeSearchRun: (runId: string, handlers: LiveRunHandlers) => {
    observed = { runId, handlers }
    return Promise.resolve(() => {
      observed = null
    })
  },
}))

/** What the index answers the post-walk top-up with (`snapshot-fill.ts`). */
let indexAnswer: { entries: SearchResultEntry[]; totalCount: number; targetVolumeId?: string } = {
  entries: [],
  totalCount: 0,
}
const indexAsks: unknown[] = []
/** What the backend says when asked whether the run will still report its ending. */
let runIsLive: boolean | Error = true

vi.mock('#lib/tauri-commands/index.js', () => ({
  cancelSearch: (runId: string) => {
    cancelled.push(runId)
    return Promise.resolve(true)
  },
  searchFiles: (query: unknown) => {
    indexAsks.push(query)
    return Promise.resolve(indexAnswer)
  },
  isSearchRunLive: () => (runIsLive instanceof Error ? Promise.reject(runIsLive) : Promise.resolve(runIsLive)),
}))

vi.mock('./snapshot-sort.svelte', () => ({ resortSnapshotIfSorted: () => Promise.resolve() }))

function entry(name: string): SearchResultEntry {
  return {
    name,
    path: `/w/${name}`,
    parentPath: '/w',
    isDirectory: false,
    size: 1,
    modifiedAt: 1,
    iconId: 'ext:txt',
  }
}

function view(overrides: Partial<LiveRunView> = {}): LiveRunView {
  return {
    phase: 'walking',
    matchCount: 3,
    dirsFound: 12,
    currentPath: '/w/deep',
    capped: false,
    phaseSince: 0,
    running: true,
    incomplete: false,
    ...overrides,
  }
}

function coverage(overrides: Partial<SearchRunCoverage> = {}): SearchRunCoverage {
  return {
    walk: 'completed',
    kind: 'live',
    permissionDenied: [],
    declined: [],
    stillCovering: [],
    unresolvedScopes: [],
    abandonedGround: false,
    capped: false,
    targetVolumeId: 'root',
    ...overrides,
  }
}

/** A snapshot in a pane, refcounted so it survives the way a real open one does. */
function openSnapshot(id = 'sr-1'): void {
  getOrCreate(id, {
    id,
    query: 'report',
    mode: 'filename',
    filters: {},
    scope: '',
    volumeId: 'root',
    caseSensitive: false,
    excludeSystemDirs: true,
    entries: [entry('first.pdf')],
    totalCount: 1,
    createdAt: 0,
    label: 'report',
    sort: null,
  })
  incrementRef(id)
}

/**
 * Hand off a running walk into an open snapshot, and wait for the subscription.
 * `refillQuery` is the run's own query, which the ending uses to top the pane up.
 */
async function handOff(runId = 'run-1', snapshotId = 'sr-1'): Promise<void> {
  openSnapshot(snapshotId)
  handOffWalk({
    runId,
    snapshotId,
    label: 'report',
    view: view(),
    refillQuery: { namePattern: 'report', patternType: 'glob', limit: 30 } as unknown as SearchQuery,
  })
  await settleHandoff()
}

/** The subscription, then the backend's word on whether the run is still live. */
async function settleHandoff(): Promise<void> {
  for (let i = 0; i < 3; i++) await new Promise((resolve) => setTimeout(resolve, 0))
}

/** The one running toast, or `undefined` when it isn't up. */
function runningToast() {
  return getToasts().find((toast) => toast.id === WALK_HANDOFF_TOAST_ID)
}

/** Toasts that aren't the running one: the module's last words. */
function otherToasts() {
  return getToasts().filter((toast) => toast.id !== WALK_HANDOFF_TOAST_ID)
}

beforeEach(() => {
  _resetWalkHandoffForTesting()
  resetSnapshots()
  clearAllToasts()
  observed = null
  cancelled.length = 0
  indexAnswer = { entries: [], totalCount: 0 }
  indexAsks.length = 0
  runIsLive = true
})

describe('handing off only a run that will still report its ending', () => {
  it('raises no toast and holds nothing for a run that already ended', async () => {
    // The walk ended (or was superseded) before the pane's listeners went in, so its
    // terminal event is gone. Waiting on it was the "Still searching" toast that never
    // stopped.
    runIsLive = false
    await handOff()
    expect(runningToast()).toBeUndefined()
    expect(otherToasts()).toEqual([])
    expect(getWalkHandoff()).toBeNull()
    expect(observed).toBeNull()
  })

  it('stops a walk it could not confirm, rather than leave it walking for nobody', async () => {
    runIsLive = new Error('IPC gone')
    await handOff('run-unconfirmed')
    expect(runningToast()).toBeUndefined()
    expect(getWalkHandoff()).toBeNull()
    expect(cancelled).toEqual(['run-unconfirmed'])
  })

  it('takes the ending it hears while it asks, and raises no running toast after it', async () => {
    openSnapshot()
    handOffWalk({ runId: 'run-fast', snapshotId: 'sr-1', label: 'report', view: view(), refillQuery: null })
    await Promise.resolve()
    await Promise.resolve()
    // Listeners in; the walk ends before the backend's answer lands.
    runIsLive = false
    observed?.handlers.onSettled(3, coverage())
    await settleHandoff()
    expect(runningToast()).toBeUndefined()
    expect(otherToasts().map((toast) => toast.level)).toEqual(['success'])
  })
})

describe('the running state', () => {
  it('says the search is still going, on the numbers it was handed', async () => {
    await handOff()
    expect(getWalkHandoff()?.view.matchCount).toBe(3)
    expect(getWalkHandoff()?.view.running).toBe(true)
    // Persistent: a search that runs for minutes must not have its only signal fade
    // out after four seconds.
    expect(runningToast()?.dismissal).toBe('persistent')
  })

  it('grows the pane and the counters as the walk finds things', async () => {
    await handOff()
    observed?.handlers.onProgress({
      phase: 'walking',
      targetVolumeId: 'root',
      entries: [entry('second.pdf'), entry('third.pdf')],
      matchCount: 9,
      dirsFound: 40,
      currentPath: '/w/deeper',
      capped: false,
    })

    expect(getSnapshot('sr-1')?.entries.map((e) => e.name)).toEqual(['first.pdf', 'second.pdf', 'third.pdf'])
    expect(getWalkHandoff()?.view.matchCount).toBe(9)
    expect(getWalkHandoff()?.view.dirsFound).toBe(40)
  })
})

describe('the way it ends', () => {
  it('tops the pane up from the index, since the walk streamed at the dialog row limit', async () => {
    await handOff()
    // What the walk wrote goes into the index as it goes, so the finished run is the
    // moment the pane can have everything rather than the rows the stream stopped at.
    indexAnswer = {
      entries: [entry('first.pdf'), entry('second.pdf'), entry('third.pdf')],
      totalCount: 3,
      targetVolumeId: 'root',
    }
    observed?.handlers.onSettled(3, coverage())
    await Promise.resolve()
    await Promise.resolve()

    expect((indexAsks[0] as { limit: number }).limit).toBe(SNAPSHOT_ENTRIES_CAP)
    expect(getSnapshot('sr-1')?.entries.map((e) => e.name)).toEqual(['first.pdf', 'second.pdf', 'third.pdf'])
  })

  it('leaves the pane alone when the index answers with no more than it already shows', async () => {
    await handOff()
    // An index that hasn't caught up with the walk must never take rows off a pane the
    // user is looking at and acting on.
    indexAnswer = { entries: [], totalCount: 0 }
    observed?.handlers.onSettled(1, coverage())
    await Promise.resolve()
    await Promise.resolve()

    expect(getSnapshot('sr-1')?.entries.map((e) => e.name)).toEqual(['first.pdf'])
  })

  it('swaps to an auto-hiding toast when the walk covered everything', async () => {
    await handOff()
    observed?.handlers.onSettled(9, coverage())

    expect(runningToast()).toBeUndefined()
    expect(getWalkHandoff()).toBeNull()
    const last = otherToasts()
    expect(last).toHaveLength(1)
    expect(last[0].dismissal).toBe('transient')
    expect(last[0].level).toBe('success')
  })

  it('warns instead when the walk finished having given up on folders', async () => {
    // The list in that pane is short and reads as exhaustive. This toast is the only
    // place that says otherwise until the dialog is reopened (Accepted difference 9).
    await handOff()
    observed?.handlers.onSettled(9, coverage({ abandonedGround: true }))

    expect(otherToasts()[0].level).toBe('warn')
  })

  it('warns when the walk was stopped or the drive went away', async () => {
    await handOff()
    observed?.handlers.onSettled(4, coverage({ walk: 'cancelled' }))

    expect(otherToasts()[0].level).toBe('warn')
  })

  it('stops waiting when a new search takes the run over', async () => {
    // Starting a search supersedes every other run backend side, so no terminal event
    // is ever coming for this one. Without this the toast would spin forever.
    await handOff()
    supersedeHandedOffWalk()

    expect(runningToast()).toBeUndefined()
    expect(getWalkHandoff()).toBeNull()
    expect(otherToasts()).toHaveLength(1)
  })

  it('stops the walk when the pane it was feeding goes away', async () => {
    // Nobody is waiting on those rows any more, and a walk still reading a disk for
    // nobody is exactly the resource waste the app promises not to be.
    await handOff()
    resetSnapshots()
    observed?.handlers.onProgress({
      phase: 'walking',
      targetVolumeId: 'root',
      entries: [entry('orphan.pdf')],
      matchCount: 5,
      dirsFound: 20,
      currentPath: null,
      capped: false,
    })

    expect(cancelled).toEqual(['run-1'])
    expect(getWalkHandoff()).toBeNull()
    expect(runningToast()).toBeUndefined()
  })
})

describe('handing the run back to a reopened dialog', () => {
  it('offers nothing when no walk is running', () => {
    expect(resumeHandedOffWalk({ onProgress: vi.fn(), onSettled: vi.fn(), onFailed: vi.fn() })).toBeNull()
  })

  it('hands over the run, where it got to, and the rows the dialog missed', async () => {
    await handOff()
    observed?.handlers.onProgress({
      phase: 'walking',
      targetVolumeId: 'root',
      entries: [entry('while-away.pdf')],
      matchCount: 9,
      dirsFound: 40,
      currentPath: '/w/deeper',
      capped: false,
    })

    const resumed = resumeHandedOffWalk({ onProgress: vi.fn(), onSettled: vi.fn(), onFailed: vi.fn() })
    expect(resumed?.runId).toBe('run-1')
    expect(resumed?.view.matchCount).toBe(9)
    // Without these the reopened dialog would show 9 matches over one row.
    expect(resumed?.missedEntries.map((e) => e.name)).toEqual(['while-away.pdf'])
    // The dialog's own progress strip says all of this in more detail, so the toast
    // stands down while it's up.
    expect(runningToast()).toBeUndefined()
  })

  it('feeds the reopened dialog instead of the buffer, and buffers again once it detaches', async () => {
    await handOff()
    const onProgress = vi.fn()
    const resumed = resumeHandedOffWalk({ onProgress, onSettled: vi.fn(), onFailed: vi.fn() })

    observed?.handlers.onProgress({
      phase: 'walking',
      targetVolumeId: 'root',
      entries: [entry('live.pdf')],
      matchCount: 4,
      dirsFound: 21,
      currentPath: null,
      capped: false,
    })
    expect(onProgress).toHaveBeenCalledTimes(1)

    resumed?.stop()
    // The dialog closed again with the walk still going, so the toast is the only
    // signal left and has to come back.
    expect(runningToast()).toBeDefined()

    observed?.handlers.onProgress({
      phase: 'walking',
      targetVolumeId: 'root',
      entries: [entry('after.pdf')],
      matchCount: 5,
      dirsFound: 22,
      currentPath: null,
      capped: false,
    })
    expect(onProgress).toHaveBeenCalledTimes(1)
    const second = resumeHandedOffWalk({ onProgress: vi.fn(), onSettled: vi.fn(), onFailed: vi.fn() })
    expect(second?.missedEntries.map((e) => e.name)).toEqual(['after.pdf'])
  })

  it('tells a resumed dialog how the run ended', async () => {
    await handOff()
    const onSettled = vi.fn()
    resumeHandedOffWalk({ onProgress: vi.fn(), onSettled, onFailed: vi.fn() })
    observed?.handlers.onSettled(9, coverage())

    expect(onSettled).toHaveBeenCalledWith(9, expect.objectContaining({ walk: 'completed' }))
  })
})
