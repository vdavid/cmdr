/**
 * The pane's half of opening a saved place: dial on landing, cancel while it
 * runs, reload once it is live, and say why when it isn't.
 *
 * ❗ The one-dial-per-landing rule is what these cells guard. The `$effect`
 * re-runs on every volume-list refresh, and a dial per refresh would be a dial
 * per second against a server the user opened once.
 *
 * Runes (`$effect.root` + `$state`), so the filename carries the `.svelte.`
 * infix.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { VolumeInfo } from '../types'

const { connectPlace, cancelPlaceConnect } = vi.hoisted(() => ({
  connectPlace: vi.fn(),
  cancelPlaceConnect: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('$lib/servers/connect-flow', () => ({ connectPlace, cancelPlaceConnect }))
vi.mock('$lib/servers/connect-refusals', () => ({
  wordConnectRefusal: (kind: string, subject: { host: string; username: string }) =>
    `${kind} for ${subject.username} at ${subject.host}`,
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createPlaceConnect, type PlaceConnect } from './place-connect.svelte'

const savedPlace: VolumeInfo = {
  id: 'sftp-nas-local-22-ada',
  name: 'Naspolya',
  path: 'sftp://ada@nas.local:22/srv/data',
  category: 'network',
  fsType: 'sftp',
  isEjectable: false,
  connectionState: 'saved',
}

describe('createPlaceConnect', () => {
  let dispose: (() => void) | undefined
  /** The pane's live `VolumeInfo`, reactive so a reassignment re-runs the factory's effect. */
  let info = $state<VolumeInfo | null>(null)

  function create(): { sub: PlaceConnect; onConnected: ReturnType<typeof vi.fn> } {
    const onConnected = vi.fn()
    let sub!: PlaceConnect
    dispose = $effect.root(() => {
      sub = createPlaceConnect({
        getVolumeId: () => info?.id ?? 'root',
        getCurrentVolumeInfo: () => info,
        onConnected,
      })
    })
    flushSync()
    return { sub, onConnected }
  }

  beforeEach(() => {
    vi.clearAllMocks()
    info = { ...savedPlace }
    connectPlace.mockResolvedValue({ kind: 'connected', volumeId: savedPlace.id })
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('shows the connecting view the moment the pane lands on a saved place', () => {
    const { sub } = create()
    expect(sub.state?.kind).toBe('connecting')
    expect(connectPlace).toHaveBeenCalledWith(expect.objectContaining({ volumeId: savedPlace.id }))
  })

  it('reloads the pane once the place is live, and drops the view', async () => {
    const { sub, onConnected } = create()
    await vi.waitFor(() => {
      expect(onConnected).toHaveBeenCalledWith(savedPlace.id)
    })
    expect(sub.state).toBeNull()
  })

  it('leaves a live or local volume alone', () => {
    info = { ...savedPlace, connectionState: 'direct' }
    const { sub } = create()
    expect(sub.state).toBeNull()
    expect(connectPlace).not.toHaveBeenCalled()

    info = { ...savedPlace, id: 'root', path: '/', category: 'main_volume', connectionState: undefined }
    dispose?.()
    const second = create()
    expect(second.sub.state).toBeNull()
    expect(connectPlace).not.toHaveBeenCalled()
  })

  it('dials ONCE per landing, however often the volume list refreshes', () => {
    const { sub } = create()
    // A refresh that changes nothing: a new object, same place, same state.
    info = { ...savedPlace }
    flushSync()
    info = { ...savedPlace }
    flushSync()
    expect(connectPlace).toHaveBeenCalledTimes(1)
    expect(sub.state?.kind).toBe('connecting')
  })

  it('arms Cancel with the attempt id the flow hands out', () => {
    connectPlace.mockImplementation((request: { onAttemptStarted?: (id: string) => void }) => {
      request.onAttemptStarted?.('server-connect-7')
      return new Promise(() => {
        // Never settles: the dial is still running while the user cancels.
      })
    })
    const { sub } = create()
    expect(sub.state?.kind).toBe('connecting')
    if (sub.state?.kind !== 'connecting') throw new Error('not connecting')
    sub.state.cancel()
    expect(cancelPlaceConnect).toHaveBeenCalledWith('server-connect-7')
  })

  it('words a refusal from the place’s own host and account, and offers Try again', async () => {
    connectPlace.mockResolvedValue({ kind: 'refused', refusal: 'unreachable' })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('refused')
    })
    if (sub.state?.kind !== 'refused') throw new Error('not refused')
    expect(sub.state.refusal).toBe('unreachable for ada at nas.local')
    // ❌ No Disconnect on a place with no session to drop.
    expect(sub.state.disconnect).toBeUndefined()

    connectPlace.mockResolvedValue({ kind: 'connected', volumeId: savedPlace.id })
    sub.state.retry()
    await vi.waitFor(() => {
      expect(sub.state).toBeNull()
    })
    expect(connectPlace).toHaveBeenCalledTimes(2)
  })

  it('says nothing when the user cancels: the view just goes', async () => {
    connectPlace.mockResolvedValue({ kind: 'cancelled' })
    const { sub, onConnected } = create()
    await vi.waitFor(() => {
      expect(sub.state).toBeNull()
    })
    expect(onConnected).not.toHaveBeenCalled()
  })

  it('keeps the spinner while the reconnect manager owns the recovery', async () => {
    connectPlace.mockResolvedValue({ kind: 'reconnecting' })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(connectPlace).toHaveBeenCalled()
    })
    expect(sub.state?.kind).toBe('connecting')
  })
})
