/**
 * Unit tests for the per-volume SMB reconnect manager.
 *
 * Covers: backoff progression on repeated failures, success path via the
 * `smb-connection-changed` event, "Retry now" reset semantics, "Cancel"
 * cleanup, refcounted subscriptions, give-up after exhausting the array,
 * and the pure display helpers (`ordinalCount`, `reconnectProgressMessage`).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Hoisted mocks: must run before importing the module under test.
const mockReconnect = vi.fn<(volumeId: string) => Promise<void>>()
const mockListen = vi.fn<(event: string, handler: (e: { payload: unknown }) => void) => Promise<() => void>>()
let lastEventHandler: ((e: { payload: unknown }) => void) | null = null

vi.mock('$lib/tauri-commands', () => ({
  reconnectSmbVolume: (id: string) => mockReconnect(id),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, handler: (e: { payload: unknown }) => void) => {
    lastEventHandler = handler
    return mockListen(event, handler)
  },
}))

import {
  smbReconnectManager,
  RECONNECT_DELAYS_MS,
  TOTAL_ATTEMPTS,
  ordinalCount,
  reconnectProgressMessage,
} from './smb-reconnect-manager.svelte'

/** Drives the listener as if the backend emitted the event. */
function emit(volumeId: string, state: 'direct' | 'disconnected'): void {
  if (!lastEventHandler) throw new Error("init() was not called or didn't install a listener")
  lastEventHandler({ payload: { volumeId, state } })
}

describe('smbReconnectManager', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    mockReconnect.mockReset()
    mockListen.mockReset()
    mockListen.mockResolvedValue(() => {
      lastEventHandler = null
    })
    // The manager's internal map is a singleton across tests; the tests use a
    // fresh volumeId each so leftover entries don't interfere.
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('does not start a cycle without subscribers', async () => {
    await smbReconnectManager.init()
    emit('vol-no-subs', 'disconnected')
    // Advance past every backoff delay; nothing should have fired.
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS.reduce((a, b) => a + b, 0) + 100)
    expect(mockReconnect).not.toHaveBeenCalled()
    expect(smbReconnectManager.getState('vol-no-subs')).toBeNull()
  })

  it('starts a cycle on disconnected event when subscribed', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-1')
    emit('vol-1', 'disconnected')
    // Right after the event we're in the first "waiting" phase.
    const state1 = smbReconnectManager.getState('vol-1')
    expect(state1?.status).toBe('waiting')
    expect(state1?.attemptIndex).toBe(0)
    expect(state1?.currentDelayMs).toBe(RECONNECT_DELAYS_MS[0])

    // Wait through the first delay → first attempt fires.
    mockReconnect.mockRejectedValueOnce(new Error('still down'))
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    expect(mockReconnect).toHaveBeenCalledTimes(1)
    // Failed → next delay scheduled.
    const state2 = smbReconnectManager.getState('vol-1')
    expect(state2?.status).toBe('waiting')
    expect(state2?.attemptIndex).toBe(1)
    expect(state2?.currentDelayMs).toBe(RECONNECT_DELAYS_MS[1])

    smbReconnectManager.cancel('vol-1')
    unsub()
  })

  it('gives up after exhausting the backoff array', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-giveup')
    mockReconnect.mockRejectedValue(new Error('still down'))

    smbReconnectManager.startCycle('vol-giveup')
    // Advance through all attempts. Each iteration: wait the delay, then fire.
    for (const delay of RECONNECT_DELAYS_MS) {
      await vi.advanceTimersByTimeAsync(delay)
    }
    const state = smbReconnectManager.getState('vol-giveup')
    expect(state?.status).toBe('gave-up')
    expect(mockReconnect).toHaveBeenCalledTimes(TOTAL_ATTEMPTS)

    smbReconnectManager.cancel('vol-giveup')
    unsub()
  })

  it('clears state and notifies subscribers on a `direct` event', async () => {
    await smbReconnectManager.init()
    const onSuccess = vi.fn()
    const unsub = smbReconnectManager.subscribe('vol-ok', onSuccess)
    emit('vol-ok', 'disconnected')
    expect(smbReconnectManager.getState('vol-ok')?.status).toBe('waiting')

    emit('vol-ok', 'direct')
    expect(smbReconnectManager.getState('vol-ok')).toBeNull()
    expect(onSuccess).toHaveBeenCalledTimes(1)

    unsub()
  })

  it('"Retry now" fires immediately and resumes backoff at attempt 2', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-retry')
    smbReconnectManager.startCycle('vol-retry')
    // We're in attempt-0 wait. Skip ahead a bit, then click Retry now.
    await vi.advanceTimersByTimeAsync(500)
    mockReconnect.mockRejectedValueOnce(new Error('still down'))
    smbReconnectManager.retryNow('vol-retry')
    // The retry runs synchronously through the awaited reconnect call —
    // flush microtasks so the failure handler runs.
    await vi.advanceTimersByTimeAsync(0)
    expect(mockReconnect).toHaveBeenCalledTimes(1)
    // After the failure, we're scheduled for the SECOND attempt — index 1
    // (with delay RECONNECT_DELAYS_MS[1]), not back to index 0.
    const state = smbReconnectManager.getState('vol-retry')
    expect(state?.status).toBe('waiting')
    expect(state?.attemptIndex).toBe(1)
    expect(state?.currentDelayMs).toBe(RECONNECT_DELAYS_MS[1])

    smbReconnectManager.cancel('vol-retry')
    unsub()
  })

  it('"Cancel" stops the timer and clears state', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-cancel')
    smbReconnectManager.startCycle('vol-cancel')
    expect(smbReconnectManager.getState('vol-cancel')?.status).toBe('waiting')

    smbReconnectManager.cancel('vol-cancel')
    expect(smbReconnectManager.getState('vol-cancel')).toBeNull()

    // Even after waiting through every delay, no attempt fires.
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS.reduce((a, b) => a + b, 0) + 100)
    expect(mockReconnect).not.toHaveBeenCalled()

    unsub()
  })

  it('refcounts subscriptions: cycle stops when the last subscriber leaves', async () => {
    await smbReconnectManager.init()
    const unsub1 = smbReconnectManager.subscribe('vol-refcount')
    const unsub2 = smbReconnectManager.subscribe('vol-refcount')
    smbReconnectManager.startCycle('vol-refcount')
    expect(smbReconnectManager.getState('vol-refcount')?.status).toBe('waiting')

    unsub1()
    // Still subscribed → cycle continues, state preserved.
    expect(smbReconnectManager.getState('vol-refcount')?.status).toBe('waiting')

    unsub2()
    // Last subscriber left → entry cleared.
    expect(smbReconnectManager.getState('vol-refcount')).toBeNull()
  })

  it('handleDirect is idempotent — onSuccess fires exactly once per cycle', async () => {
    // Race scenario: both the `direct` event and the awaited `reconnectSmbVolume`
    // success path could each trigger `handleDirect`. The idempotency guard
    // ensures `onSuccess` only fires once.
    await smbReconnectManager.init()
    const onSuccess = vi.fn()
    const unsub = smbReconnectManager.subscribe('vol-once', onSuccess)
    smbReconnectManager.startCycle('vol-once')
    // Resolve `reconnectSmbVolume` AND emit the `direct` event (simulating both
    // paths racing to clean up). Only one should win and notify.
    mockReconnect.mockResolvedValueOnce(undefined)
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    emit('vol-once', 'direct')
    expect(onSuccess).toHaveBeenCalledTimes(1)

    unsub()
  })

  it('two subscribers see the same state object (one cycle, both panes)', async () => {
    await smbReconnectManager.init()
    const unsub1 = smbReconnectManager.subscribe('vol-shared')
    const unsub2 = smbReconnectManager.subscribe('vol-shared')
    smbReconnectManager.startCycle('vol-shared')
    const a = smbReconnectManager.getState('vol-shared')
    const b = smbReconnectManager.getState('vol-shared')
    expect(a).not.toBeNull()
    expect(b).toEqual(a)

    smbReconnectManager.cancel('vol-shared')
    unsub1()
    unsub2()
  })
})

describe('reconnect display helpers', () => {
  describe('ordinalCount', () => {
    it.each([
      [1, 'once'],
      [2, 'twice'],
      [3, '3 times'],
      [10, '10 times'],
    ])('formats %i → %s', (n, expected) => {
      expect(ordinalCount(n)).toBe(expected)
    })
  })

  describe('reconnectProgressMessage', () => {
    it('returns null for the very first attempt (no body 2)', () => {
      expect(reconnectProgressMessage(0)).toBeNull()
    })

    it('formats the body 2 message for each attempt index', () => {
      // With the default 5-attempt array:
      // attemptIndex=1 → upcoming attempt 2, 3 more after this (3, 4, 5)
      expect(reconnectProgressMessage(1)).toBe('Retried once, will try it 3 times more after this.')
      // attemptIndex=2 → upcoming attempt 3, 2 more after this (4, 5)
      expect(reconnectProgressMessage(2)).toBe('Retried twice, will try it twice more after this.')
      // attemptIndex=3 → upcoming attempt 4, 1 more after this (5)
      expect(reconnectProgressMessage(3)).toBe('Retried 3 times, will try it once more after this.')
      // attemptIndex=4 → final attempt
      expect(reconnectProgressMessage(4)).toBe(
        'Retried 4 times, this is the final attempt — will drop the connection if it fails.',
      )
    })
  })
})
