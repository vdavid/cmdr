/**
 * Tests for `device-connect.svelte.ts`, the pane's side of opening a phone.
 *
 * They pin the four answers that are easy to get wrong: a phone waiting for its
 * Allow tap is NOT dialed (the answer is known in advance), the same phone
 * turning ready dials WITHOUT anyone pressing anything, one landing means one
 * dial, and Cancel aims at the id minted before the dial started.
 *
 * Runes, so the filename carries the `.svelte.` infix: the factory creates its
 * `$effect` in a reactive root.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { DeviceReadiness } from '$lib/ipc/bindings'
import type { VolumeBackendCapabilities, VolumeInfo } from '../types'

const { ipc } = vi.hoisted(() => ({
  ipc: {
    // eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the real `connectAdbDevice(serial, attemptId)` IPC signature, which this test asserts on by position
    connectAdbDevice: vi.fn<(serial: string, attemptId: string) => Promise<string>>(),
    cancelAdbConnect: vi.fn<(attemptId: string) => Promise<boolean>>(),
    openSettingsWindow: vi.fn(),
  },
}))

vi.mock('$lib/tauri-commands', () => ({
  connectAdbDevice: ipc.connectAdbDevice,
  cancelAdbConnect: ipc.cancelAdbConnect,
  newAdbAttemptId: () => 'adb-attempt-1',
  // The thrown value is `AdbConnectFailure`; the real reader unwraps it.
  asAdbConnectError: (e: unknown) => (e as { failure?: unknown }).failure ?? null,
}))
vi.mock('$lib/settings/settings-window', () => ({ openSettingsWindow: ipc.openSettingsWindow }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createDeviceConnect, type DeviceConnectDeps } from './device-connect.svelte'

const ADB_VOLUME_ID = 'adb-pixel-7-a1b2c3d'
const SERIAL = 'R58M12345'

function phone(readiness: DeviceReadiness | null): VolumeInfo {
  return {
    id: ADB_VOLUME_ID,
    name: 'Pixel 7',
    path: `adb://${SERIAL}`,
    category: 'mobile_device',
    isEjectable: true,
    fsType: 'adb',
    deviceReadiness: readiness,
  }
}

describe('createDeviceConnect', () => {
  let dispose: (() => void) | undefined

  function create(opts: { volumeId?: string; info?: VolumeInfo | null } = {}) {
    let volumeId = $state(opts.volumeId ?? ADB_VOLUME_ID)
    let info = $state<VolumeInfo | null>(opts.info === undefined ? phone({ kind: 'ready' }) : opts.info)
    const onConnected = vi.fn()
    const deps: DeviceConnectDeps = {
      getVolumeId: () => volumeId,
      getCurrentVolumeInfo: () => info,
      onConnected,
    }
    let sub!: ReturnType<typeof createDeviceConnect>
    dispose = $effect.root(() => {
      sub = createDeviceConnect(deps)
    })
    flushSync()
    return {
      sub,
      onConnected,
      // Arrow properties, not shorthand methods: the tests destructure them, and
      // an unbound method off an object is what `@typescript-eslint/unbound-method`
      // is about.
      setInfo: (next: VolumeInfo | null) => {
        info = next
        flushSync()
      },
      setVolumeId: (next: string) => {
        volumeId = next
        flushSync()
      },
    }
  }

  beforeEach(() => {
    vi.clearAllMocks()
    ipc.connectAdbDevice.mockResolvedValue(ADB_VOLUME_ID)
    ipc.cancelAdbConnect.mockResolvedValue(true)
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('stays out of the way of a volume that is not a phone', () => {
    const { sub } = create({ volumeId: 'root', info: null })
    expect(sub.state).toBeNull()
    expect(sub.holdsListing).toBe(false)
    expect(ipc.connectAdbDevice).not.toHaveBeenCalled()
  })

  it('dials a ready phone once, and holds the pane while it does', () => {
    const { sub } = create()
    expect(ipc.connectAdbDevice).toHaveBeenCalledWith(SERIAL, 'adb-attempt-1')
    expect(sub.state?.kind).toBe('connecting')
    // ❗ The listing is held: the phone has no volume until this dial lands, so a
    // `list_directory` here could only come back refused.
    expect(sub.holdsListing).toBe(true)
  })

  it('reloads the pane once the phone is open, and lets go of the listing', async () => {
    const { sub, onConnected } = create()
    await vi.waitFor(() => {
      expect(onConnected).toHaveBeenCalledWith(ADB_VOLUME_ID)
    })
    expect(sub.state).toBeNull()
    expect(sub.holdsListing).toBe(false)
  })

  it('never dials a phone still showing its "Allow USB debugging?" prompt', () => {
    // ❗ The dial's answer is known in advance (`unauthorized`), so making the
    // round-trip only spends a second before saying what the row already said.
    const { sub } = create({ info: phone({ kind: 'waiting_for_authorization' }) })
    expect(ipc.connectAdbDevice).not.toHaveBeenCalled()
    expect(sub.state?.kind).toBe('waiting_for_device')
  })

  it('walks in by itself the moment the phone turns ready, with nothing pressed', async () => {
    const { sub, setInfo, onConnected } = create({ info: phone({ kind: 'waiting_for_authorization' }) })
    expect(sub.state?.kind).toBe('waiting_for_device')

    // What a `volumes-changed` broadcast does to the pane's own volume lookup.
    setInfo(phone({ kind: 'ready' }))

    expect(ipc.connectAdbDevice).toHaveBeenCalledWith(SERIAL, 'adb-attempt-1')
    await vi.waitFor(() => {
      expect(onConnected).toHaveBeenCalledWith(ADB_VOLUME_ID)
    })
  })

  it('aims Cancel at the id the dial was given, and leaves a way back in', () => {
    const { sub } = create()
    const state = sub.state
    expect(state?.kind).toBe('connecting')
    if (state?.kind !== 'connecting') return
    state.cancel()
    expect(ipc.cancelAdbConnect).toHaveBeenCalledWith('adb-attempt-1')

    // ❗ The pane is still HELD, so a `null` state renders NOTHING: no listing,
    // no message, no button, and no way out but switching volumes. A cancel that
    // says nothing has to leave something to press.
    expect(sub.holdsListing).toBe(true)
    expect(sub.state?.kind).toBe('refused')
    if (sub.state?.kind !== 'refused') return
    sub.state.retry?.()
    expect(ipc.connectAdbDevice).toHaveBeenCalledTimes(2)
  })

  it('leaves the same way back in when the dial itself comes back cancelled', async () => {
    ipc.connectAdbDevice.mockRejectedValueOnce({ failure: { type: 'cancelled' } })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('refused')
    })
    if (sub.state?.kind !== 'refused') return
    expect(sub.state.retry).toBeTypeOf('function')
    expect(sub.holdsListing).toBe(true)
  })

  it('leaves a way back in when the dial breaks down without a typed reason', async () => {
    // Not an `AdbConnectError` at all: the IPC transport itself. Its text is
    // untranslated diagnostics, so the log gets it and the pane gets the one
    // sentence that is true either way, with a Try again beside it.
    ipc.connectAdbDevice.mockRejectedValueOnce(new Error('ipc went away'))
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('refused')
    })
    if (sub.state?.kind !== 'refused') return
    expect(sub.state.refusal).toBe('adb.connect.transport')
    expect(sub.holdsListing).toBe(true)
    sub.state.retry?.()
    expect(ipc.connectAdbDevice).toHaveBeenCalledTimes(2)
  })

  it('leaves a way back in when the waiting state is cancelled', () => {
    const { sub } = create({ info: phone({ kind: 'waiting_for_authorization' }) })
    const state = sub.state
    expect(state?.kind).toBe('waiting_for_device')
    if (state?.kind !== 'waiting_for_device') return
    state.cancel()
    expect(sub.state?.kind).toBe('refused')
    expect(sub.holdsListing).toBe(true)
  })

  it('renders the waiting state when the dial itself comes back unauthorized', async () => {
    ipc.connectAdbDevice.mockRejectedValueOnce({ failure: { type: 'unauthorized', serial: SERIAL } })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('waiting_for_device')
    })
  })

  it('offers a retry that really re-dials', async () => {
    ipc.connectAdbDevice.mockRejectedValueOnce({ failure: { type: 'timedOut' } })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('refused')
    })
    const state = sub.state
    if (state?.kind !== 'refused') return
    expect(state.retry).toBeTypeOf('function')
    state.retry?.()
    expect(ipc.connectAdbDevice).toHaveBeenCalledTimes(2)
  })

  it('offers Settings, and no retry, when there are no platform tools to talk to', async () => {
    ipc.connectAdbDevice.mockRejectedValueOnce({ failure: { type: 'adbNotInstalled' } })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('refused')
    })
    const state = sub.state
    if (state?.kind !== 'refused') return
    expect(state.retry).toBeUndefined()
    state.openSettings?.()
    expect(ipc.openSettingsWindow).toHaveBeenCalled()
  })

  it('offers nothing at all for a phone that left', async () => {
    ipc.connectAdbDevice.mockRejectedValueOnce({ failure: { type: 'deviceGone', serial: SERIAL } })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('refused')
    })
    const state = sub.state
    if (state?.kind !== 'refused') return
    expect(state.retry).toBeUndefined()
    expect(state.openSettings).toBeUndefined()
  })

  it('holds the listing and dials again when the phone stays listed but its volume was ejected', async () => {
    const { sub, setInfo, onConnected } = create()
    await vi.waitFor(() => {
      expect(onConnected).toHaveBeenCalledTimes(1)
    })
    // The `volumes-changed` after the dial: enrichment fills `capabilities` only
    // for a volume the registry holds, so this is what "dialed" looks like.
    const registered: VolumeBackendCapabilities = { backendCanWrite: true, canExport: true, canBeIndexed: false }
    setInfo({ ...phone({ kind: 'ready' }), capabilities: registered })
    expect(sub.holdsListing).toBe(false)

    // Eject unregisters the volume, but `adb` has no per-client detach, so the
    // phone stays listed: same id, same readiness, no capabilities.
    setInfo(phone({ kind: 'ready' }))

    // ❗ A listing now could only come back refused, so the pane is held and the
    // phone is dialed again.
    expect(sub.holdsListing).toBe(true)
    expect(ipc.connectAdbDevice).toHaveBeenCalledTimes(2)
    await vi.waitFor(() => {
      expect(onConnected).toHaveBeenCalledTimes(2)
    })
  })

  it('lets go of everything when the pane leaves the phone', () => {
    const { sub, setVolumeId } = create()
    expect(sub.state?.kind).toBe('connecting')
    setVolumeId('root')
    expect(sub.state).toBeNull()
    expect(sub.holdsListing).toBe(false)
  })
})
