/**
 * The status-corner badge: seeded once, then moved only by being told.
 *
 * The seed and the subscription are both load-bearing and neither replaces the other. Without
 * the seed a suggestion made in a previous session stays invisible until something changes,
 * because suggestions never expire. Without the subscription the badge freezes at whatever the
 * seed found.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SuggestedSweepView, SuggestionsChanged } from '$lib/tauri-commands'

const listMock = vi.fn<() => Promise<SuggestedSweepView[]>>()
const listenMock = vi.fn<(cb: (p: SuggestionsChanged) => void) => Promise<() => void>>()
const unlistenMock = vi.fn()

/** The subscriber the module handed us, so a test can push an event at it. */
let emit: ((payload: SuggestionsChanged) => void) | null = null

vi.mock('$lib/tauri-commands', () => ({
  listSuggestedOps: () => listMock(),
  onSuggestionsChanged: (cb: (p: SuggestionsChanged) => void) => listenMock(cb),
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { startSuggestedOpsBadge, stopSuggestedOpsBadge, suggestedOpsBadge } from './suggested-ops-badge.svelte'

function sweep(groups: { liveOpCount: number }[]): SuggestedSweepView {
  return { sweepId: 1, createdAt: 0, rationale: null, groups } as unknown as SuggestedSweepView
}

beforeEach(() => {
  // Tear down BEFORE clearing, so the previous test's unsubscribe isn't counted against this
  // one's expectations.
  stopSuggestedOpsBadge()
  vi.clearAllMocks()
  suggestedOpsBadge.pendingGroupCount = 0
  suggestedOpsBadge.pendingOpCount = 0
  emit = null
  listMock.mockResolvedValue([])
  listenMock.mockImplementation((cb) => {
    emit = cb
    return Promise.resolve(unlistenMock)
  })
})

describe('seeding', () => {
  it('counts what was already waiting from an earlier session', async () => {
    listMock.mockResolvedValue([sweep([{ liveOpCount: 5 }, { liveOpCount: 12 }])])

    await startSuggestedOpsBadge()

    expect(suggestedOpsBadge.pendingGroupCount).toBe(2)
    expect(suggestedOpsBadge.pendingOpCount).toBe(17)
  })

  it('stays at zero rather than guessing when the read fails', async () => {
    listMock.mockRejectedValueOnce(new Error('no store'))

    await startSuggestedOpsBadge()

    expect(suggestedOpsBadge.pendingGroupCount).toBe(0)
  })

  it('subscribes once however many times it is started', async () => {
    await startSuggestedOpsBadge()
    await startSuggestedOpsBadge()

    expect(listenMock).toHaveBeenCalledTimes(1)
  })
})

describe('being told', () => {
  it('takes the counts off the event, with no follow-up query', async () => {
    await startSuggestedOpsBadge()
    listMock.mockClear()

    emit?.({ pendingGroupCount: 3, pendingOpCount: 61, groupId: null, reason: 'proposed' })

    expect(suggestedOpsBadge.pendingGroupCount).toBe(3)
    expect(suggestedOpsBadge.pendingOpCount).toBe(61)
    expect(listMock).not.toHaveBeenCalled()
  })

  it('follows an approval back down to nothing waiting', async () => {
    listMock.mockResolvedValue([sweep([{ liveOpCount: 4 }])])
    await startSuggestedOpsBadge()

    emit?.({ pendingGroupCount: 0, pendingOpCount: 0, groupId: 7, reason: 'approved' })

    expect(suggestedOpsBadge.pendingGroupCount).toBe(0)
  })

  it('stops listening when torn down, so a reload cannot double-count', async () => {
    await startSuggestedOpsBadge()

    stopSuggestedOpsBadge()

    expect(unlistenMock).toHaveBeenCalledTimes(1)
  })
})
