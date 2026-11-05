/**
 * Unit tests for the per-volume SMB reconnect manager.
 *
 * Covers: backoff progression on repeated failures, success path via the
 * `volume-connection-changed` event, "Retry now" reset semantics, "Cancel"
 * cleanup, refcounted subscriptions, give-up after exhausting the array,
 * and the pure display helpers (`ordinalCount`, `reconnectProgressMessage`).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type {
  SignInShape,
  VolumeConnection,
  VolumeConnectionChanged,
  VolumeReconnectRequested,
} from '#lib/ipc/bindings.js'

// Hoisted mocks: must run before importing the module under test.
const mockReconnect = vi.fn<(volumeId: string) => Promise<void>>()
const mockSignInState = vi.fn<(volumeId: string) => Promise<SignInShape>>()
const mockListen = vi.fn<(handler: (payload: VolumeConnectionChanged) => void) => Promise<() => void>>()
let lastEventHandler: ((payload: VolumeConnectionChanged) => void) | null = null
let lastReconnectRequest: ((payload: VolumeReconnectRequested) => void) | null = null

vi.mock('#lib/tauri-commands/index.js', () => ({
  reconnectVolume: (id: string) => mockReconnect(id),
  getVolumeSignInState: (id: string) => mockSignInState(id),
  // The manager subscribes to `volume-connection-changed` through this typed wrapper,
  // so the test captures its unwrapped-payload handler here.
  onVolumeConnectionChanged: (handler: (payload: VolumeConnectionChanged) => void) => {
    lastEventHandler = handler
    return mockListen(handler)
  },
  // Ask Cmdr's `connect_volume` asking the manager to take a dropped volume.
  onVolumeReconnectRequested: (handler: (payload: VolumeReconnectRequested) => void) => {
    lastReconnectRequest = handler
    return Promise.resolve(() => {})
  },
}))

const { warn, error } = vi.hoisted(() => ({ warn: vi.fn(), error: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, info: vi.fn(), debug: vi.fn(), error }),
}))

import { ReconnectFailure } from './reconnect-error'
import {
  smbReconnectManager,
  RECONNECT_DELAYS_MS,
  TOTAL_ATTEMPTS,
  TOTAL_DURATION_MS,
  ordinalCount,
  reconnectProgressMessage,
} from './smb-reconnect-manager.svelte'

/** Drives the listener as if the backend emitted the event. */
function emit(volumeId: string, state: VolumeConnection): void {
  if (!lastEventHandler) throw new Error("init() was not called or didn't install a listener")
  lastEventHandler({ volumeId, state })
}

describe('smbReconnectManager', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    mockReconnect.mockReset()
    mockSignInState.mockReset()
    mockSignInState.mockResolvedValue({ kind: 'password' })
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

  it('takes a dropped volume when the backend asks, the way a pane landing on it does', async () => {
    await smbReconnectManager.init()
    if (!lastReconnectRequest) throw new Error("init() didn't listen for reconnect requests")
    lastReconnectRequest({ volumeId: 'vol-asked' })

    expect(smbReconnectManager.getState('vol-asked')?.status).toBe('waiting')
    mockReconnect.mockResolvedValueOnce(undefined)
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    expect(mockReconnect).toHaveBeenCalledWith('vol-asked')
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

  it('stops the backoff and flips to needs-auth on a `needs_credentials` event', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-auth')
    mockReconnect.mockRejectedValue(new Error('STATUS_LOGON_FAILURE'))

    smbReconnectManager.startCycle('vol-auth')
    // First attempt fires after the first delay and fails; the backend would emit
    // needs_credentials alongside that failure.
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    emit('vol-auth', 'needs_credentials')

    expect(smbReconnectManager.getState('vol-auth')?.status).toBe('needs-auth')

    // The futile backoff must stop: advancing well past the full schedule fires no
    // further reconnect attempts (retrying the same stale password can't succeed).
    const callsAtFlip = mockReconnect.mock.calls.length
    for (const delay of RECONNECT_DELAYS_MS) {
      await vi.advanceTimersByTimeAsync(delay)
    }
    expect(mockReconnect.mock.calls.length).toBe(callsAtFlip)

    // A later successful sign-in (`connected`) clears the needs-auth state.
    emit('vol-auth', 'connected')
    expect(smbReconnectManager.getState('vol-auth')).toBeNull()

    smbReconnectManager.cancel('vol-auth')
    unsub()
  })

  /**
   * A named login the server answered with a guest session (#457): its own status, so the pane
   * names what Cmdr refused. The backoff stops like a stale password's (retrying gets the same
   * guest offer), and a later sign-in clears it.
   */
  it('stops the backoff and flips to guest-refused on a `guest_session_refused` event', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-guest')
    mockReconnect.mockRejectedValue(new Error('guest session refused'))

    smbReconnectManager.startCycle('vol-guest')
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    emit('vol-guest', 'guest_session_refused')
    expect(smbReconnectManager.getState('vol-guest')?.status).toBe('guest-refused')

    const callsAtFlip = mockReconnect.mock.calls.length
    for (const delay of RECONNECT_DELAYS_MS) {
      await vi.advanceTimersByTimeAsync(delay)
    }
    expect(mockReconnect.mock.calls.length).toBe(callsAtFlip)

    emit('vol-guest', 'connected')
    expect(smbReconnectManager.getState('vol-guest')).toBeNull()
    smbReconnectManager.cancel('vol-guest')
    unsub()
  })

  /**
   * "Always require SMB encryption" on, and a server that can't encrypt (#966): the same server
   * answers the same until it or the setting changes, so the backoff stops on the event.
   */
  it('stops the backoff and flips to encryption-required on an `encryption_required` event', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-enc')
    mockReconnect.mockRejectedValue(new Error('refused'))

    smbReconnectManager.startCycle('vol-enc')
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    emit('vol-enc', 'encryption_required')
    expect(smbReconnectManager.getState('vol-enc')?.status).toBe('encryption-required')

    const callsAtFlip = mockReconnect.mock.calls.length
    for (const delay of RECONNECT_DELAYS_MS) {
      await vi.advanceTimersByTimeAsync(delay)
    }
    expect(mockReconnect.mock.calls.length).toBe(callsAtFlip)
    smbReconnectManager.cancel('vol-enc')
    unsub()
  })

  /** The command's typed refusal says it too, so the cycle stops even before the event lands. */
  it('stops the backoff on a typed encryptionRequired refusal from the command', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-enc-typed')
    mockReconnect.mockRejectedValue(
      new ReconnectFailure({ type: 'volume', error: { type: 'encryptionRequired', data: '/Volumes/public' } }),
    )

    smbReconnectManager.startCycle('vol-enc-typed')
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    expect(smbReconnectManager.getState('vol-enc-typed')?.status).toBe('encryption-required')

    const callsAtFlip = mockReconnect.mock.calls.length
    for (const delay of RECONNECT_DELAYS_MS) {
      await vi.advanceTimersByTimeAsync(delay)
    }
    expect(mockReconnect.mock.calls.length).toBe(callsAtFlip)
    smbReconnectManager.cancel('vol-enc-typed')
    unsub()
  })

  /**
   * An unconfirmed negotiation and a share whose encryption the session couldn't agree on stop the
   * backoff too, from the event or the command's typed refusal: asking again asks the same server
   * (#931).
   */
  it.each([
    ['negotiation_invalid', 'negotiationInvalid', 'negotiation-invalid'],
    ['encryption_unavailable', 'encryptionUnavailable', 'encryption-unavailable'],
  ] as const)('stops the backoff on %s, by event or typed refusal', async (event, errorType, status) => {
    await smbReconnectManager.init()

    const byEvent = `vol-${event}-event`
    const unsubEvent = smbReconnectManager.subscribe(byEvent)
    mockReconnect.mockRejectedValue(new Error('refused'))
    smbReconnectManager.startCycle(byEvent)
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    emit(byEvent, event)
    expect(smbReconnectManager.getState(byEvent)?.status).toBe(status)
    smbReconnectManager.cancel(byEvent)
    unsubEvent()

    const byRefusal = `vol-${event}-typed`
    const unsubTyped = smbReconnectManager.subscribe(byRefusal)
    const error =
      errorType === 'negotiationInvalid'
        ? ({ type: 'negotiationInvalid', data: 'unconfirmed' } as const)
        : ({ type: 'encryptionUnavailable', data: '/Volumes/public' } as const)
    mockReconnect.mockRejectedValue(new ReconnectFailure({ type: 'volume', error }))
    smbReconnectManager.startCycle(byRefusal)
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    expect(smbReconnectManager.getState(byRefusal)?.status).toBe(status)

    const callsAtFlip = mockReconnect.mock.calls.length
    for (const delay of RECONNECT_DELAYS_MS) {
      await vi.advanceTimersByTimeAsync(delay)
    }
    expect(mockReconnect.mock.calls.length).toBe(callsAtFlip)
    smbReconnectManager.cancel(byRefusal)
    unsubTyped()
  })

  /**
   * The banner asks the backend what a sign-in would want, at the moment it has
   * something to ask about. Nothing renders this yet (the SFTP sign-in UI isn't
   * built), so the cell asserts on the stored value.
   */
  it('asks what a sign-in would want when a volume needs auth', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-prompt')
    mockSignInState.mockResolvedValue({ kind: 'key_passphrase' })

    emit('vol-prompt', 'needs_credentials')
    await vi.advanceTimersByTimeAsync(0)

    expect(mockSignInState).toHaveBeenCalledWith('vol-prompt')
    // Compared on `kind`: the shape is a tagged union, and a variant that grows a
    // field must not change how the others read.
    expect(smbReconnectManager.getSignInShape('vol-prompt')?.kind).toBe('key_passphrase')

    smbReconnectManager.cancel('vol-prompt')
    unsub()
  })

  it('logs an unreadable sign-in state once per volume, and again only after a read works', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-unreadable')
    warn.mockClear()

    // A flaky network flips the volume in and out of needs-auth, and every flip re-asks.
    mockSignInState.mockRejectedValue(new Error('keychain locked'))
    for (let flip = 0; flip < 3; flip++) {
      emit('vol-unreadable', 'connected')
      emit('vol-unreadable', 'needs_credentials')
      await vi.advanceTimersByTimeAsync(0)
    }
    expect(warn).toHaveBeenCalledOnce()

    mockSignInState.mockResolvedValue({ kind: 'password' })
    emit('vol-unreadable', 'connected')
    emit('vol-unreadable', 'needs_credentials')
    await vi.advanceTimersByTimeAsync(0)
    mockSignInState.mockRejectedValue(new Error('keychain locked'))
    emit('vol-unreadable', 'connected')
    emit('vol-unreadable', 'needs_credentials')
    await vi.advanceTimersByTimeAsync(0)
    expect(warn).toHaveBeenCalledTimes(2)

    smbReconnectManager.cancel('vol-unreadable')
    unsub()
  })

  /**
   * The answer is asked again every time, and never carried over: the rung a
   * remote volume comes back on is decided per dial, so the previous answer is a
   * statement about a session that has since ended.
   */
  it('re-asks on a later needs-auth rather than reusing the first answer', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-prompt-again')

    mockSignInState.mockResolvedValue({ kind: 'nothing' })
    emit('vol-prompt-again', 'needs_credentials')
    await vi.advanceTimersByTimeAsync(0)
    expect(smbReconnectManager.getSignInShape('vol-prompt-again')?.kind).toBe('nothing')

    emit('vol-prompt-again', 'connected')
    mockSignInState.mockResolvedValue({ kind: 'username_password', guestAllowed: true })
    emit('vol-prompt-again', 'needs_credentials')
    await vi.advanceTimersByTimeAsync(0)

    expect(mockSignInState).toHaveBeenCalledTimes(2)
    // A share answers `username_password` and carries whether guests are welcome,
    // so the sheet can render an editable username and a guest button off ONE read.
    expect(smbReconnectManager.getSignInShape('vol-prompt-again')).toEqual({
      kind: 'username_password',
      guestAllowed: true,
    })

    smbReconnectManager.cancel('vol-prompt-again')
    unsub()
  })

  it('clears state and notifies subscribers on a `connected` event', async () => {
    await smbReconnectManager.init()
    const onSuccess = vi.fn()
    const unsub = smbReconnectManager.subscribe('vol-ok', onSuccess)
    emit('vol-ok', 'disconnected')
    expect(smbReconnectManager.getState('vol-ok')?.status).toBe('waiting')

    emit('vol-ok', 'connected')
    expect(smbReconnectManager.getState('vol-ok')).toBeNull()
    expect(onSuccess).toHaveBeenCalledTimes(1)

    unsub()
  })

  // A throwing callback is a bug in Cmdr's own code, so it has to reach error reports.
  it('logs a throwing success callback at error and still notifies the others', async () => {
    await smbReconnectManager.init()
    const throwing = vi.fn(() => {
      throw new Error('subscriber bug')
    })
    const healthy = vi.fn()
    const unsubThrowing = smbReconnectManager.subscribe('vol-throw', throwing)
    const unsubHealthy = smbReconnectManager.subscribe('vol-throw', healthy)
    error.mockClear()

    emit('vol-throw', 'disconnected')
    emit('vol-throw', 'connected')

    expect(healthy).toHaveBeenCalledOnce()
    expect(error).toHaveBeenCalledOnce()

    unsubThrowing()
    unsubHealthy()
  })

  it('"Retry now" fires immediately and resumes backoff at attempt 2', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-retry')
    smbReconnectManager.startCycle('vol-retry')
    // We're in attempt-0 wait. Skip ahead a bit, then click Retry now.
    await vi.advanceTimersByTimeAsync(500)
    mockReconnect.mockRejectedValueOnce(new Error('still down'))
    smbReconnectManager.retryNow('vol-retry')
    // The retry runs synchronously through the awaited reconnect call.
    // Flush microtasks so the failure handler runs.
    await vi.advanceTimersByTimeAsync(0)
    expect(mockReconnect).toHaveBeenCalledTimes(1)
    // After the failure, we're scheduled for the SECOND attempt: index 1
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

  it('subscribe inside a $effect does not loop (untrack regression)', async () => {
    // Regression test: in Phase B v1, calling `subscribe` from a Svelte
    // `$effect` caused an infinite loop. `subscribe` did `map.get` (reactive
    // read) then `map.set` (reactive write) on the same key, which invalidated
    // the effect's tracked dep, the effect re-ran, etc. The runaway loop
    // pegged the main thread and stuck both panes on "Loading…". The fix
    // wraps the manager's mutating methods in `untrack` so callers from a
    // reactive context don't track our internal map reads.
    //
    // We can't easily run a real Svelte `$effect` from a vitest test, but we
    // can verify the contract: a single call to `subscribe` (regardless of
    // surrounding context) produces exactly one log entry's worth of work,
    // not a runaway. Concretely: the entry refcount is 1, not 1000+, after
    // a single subscribe call.
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-untrack')
    // The `subscribe` call returns synchronously; if it had been looping, we
    // wouldn't have reached this line. The state should have a single entry.
    smbReconnectManager.startCycle('vol-untrack')
    const state = smbReconnectManager.getState('vol-untrack')
    expect(state).not.toBeNull()
    expect(state?.attemptIndex).toBe(0)

    smbReconnectManager.cancel('vol-untrack')
    unsub()
  })

  it('handleDirect is idempotent: onSuccess fires exactly once per cycle', async () => {
    // Race scenario: both the `connected` event and the awaited `reconnectVolume`
    // success path could each trigger `handleConnected`. The idempotency guard
    // ensures `onSuccess` only fires once.
    await smbReconnectManager.init()
    const onSuccess = vi.fn()
    const unsub = smbReconnectManager.subscribe('vol-once', onSuccess)
    smbReconnectManager.startCycle('vol-once')
    // Resolve `reconnectVolume` AND emit the `connected` event (simulating both
    // paths racing to clean up). Only one should win and notify.
    mockReconnect.mockResolvedValueOnce(undefined)
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    emit('vol-once', 'connected')
    expect(onSuccess).toHaveBeenCalledTimes(1)

    unsub()
  })

  it('a reactive reader sees every transition, Cancel included (ERR-VZ4CV)', async () => {
    // Regression: each transition mutated the SAME entry object and re-set it,
    // and `SvelteMap.set` stays silent when the value is identical. The pane's
    // `$derived` froze on its first read: "Retry now" stayed disabled over a
    // cycle that had long given up, and Cancel left the view on screen.
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-reactive')
    let seen: string | null = null
    const dispose = $effect.root(() => {
      const status = $derived(smbReconnectManager.getState('vol-reactive')?.status ?? null)
      $effect(() => {
        seen = status
      })
    })
    smbReconnectManager.startCycle('vol-reactive')
    flushSync()
    expect(seen).toBe('waiting')

    let fail!: (e: Error) => void
    mockReconnect.mockImplementationOnce(() => new Promise<void>((_resolve, reject) => (fail = reject)))
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    flushSync()
    expect(seen).toBe('attempting')

    fail(new Error('still down'))
    await vi.advanceTimersByTimeAsync(0)
    flushSync()
    expect(seen).toBe('waiting')

    smbReconnectManager.cancel('vol-reactive')
    flushSync()
    expect(seen).toBeNull()

    dispose()
    unsub()
  })

  it('a Cancel during an attempt stays cancelled when that attempt fails', async () => {
    await smbReconnectManager.init()
    const unsub = smbReconnectManager.subscribe('vol-cancel-inflight')
    smbReconnectManager.startCycle('vol-cancel-inflight')
    let fail!: (e: Error) => void
    mockReconnect.mockImplementationOnce(() => new Promise<void>((_resolve, reject) => (fail = reject)))
    await vi.advanceTimersByTimeAsync(RECONNECT_DELAYS_MS[0])
    expect(smbReconnectManager.getState('vol-cancel-inflight')?.status).toBe('attempting')

    smbReconnectManager.cancel('vol-cancel-inflight')
    fail(new Error('still down'))
    await vi.advanceTimersByTimeAsync(TOTAL_DURATION_MS + 100)
    expect(smbReconnectManager.getState('vol-cancel-inflight')).toBeNull()
    expect(mockReconnect).toHaveBeenCalledTimes(1)

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
        'Retried 4 times, this is the final attempt. Connection drops if it fails.',
      )
    })
  })
})
