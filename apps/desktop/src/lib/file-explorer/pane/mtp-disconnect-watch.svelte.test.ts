/**
 * Tests for `mtp-disconnect-watch.svelte.ts`, the pane's fallback when the phone
 * it's browsing is unplugged. They pin:
 * - the device id is the volume id minus its trailing storage segment, split on
 *   the LAST colon (a serial-based device id can itself contain one),
 * - only THIS device's disconnect triggers the fallback,
 * - a non-MTP pane, and a device-only id with no storage segment, register no
 *   listener at all,
 * - switching volume re-registers the listener against the new device (the
 *   stale-closure bug the reactive capture exists to prevent),
 * - teardown unlistens.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest'
import { flushSync } from 'svelte'

const { ipc } = vi.hoisted<{ ipc: { onMtpDeviceDisconnected: Mock } }>(() => ({
  ipc: { onMtpDeviceDisconnected: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({ onMtpDeviceDisconnected: ipc.onMtpDeviceDisconnected }))
vi.mock('$lib/mtp', () => ({ isMtpVolumeId: (id: string) => id.startsWith('mtp-') }))

import { createMtpDisconnectWatch } from './mtp-disconnect-watch.svelte'

type Handler = (event: { deviceId: string }) => void

describe('createMtpDisconnectWatch', () => {
  let dispose: (() => void) | undefined
  let handlers: Handler[]
  let unlistens: Mock[]
  let onFatal: Mock

  beforeEach(() => {
    vi.clearAllMocks()
    handlers = []
    unlistens = []
    onFatal = vi.fn()
    ipc.onMtpDeviceDisconnected.mockImplementation((cb: Handler) => {
      handlers.push(cb)
      const unlisten = vi.fn()
      unlistens.push(unlisten)
      return Promise.resolve(unlisten)
    })
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  function create(volumeId: string) {
    let id = $state(volumeId)
    dispose = $effect.root(() => {
      createMtpDisconnectWatch({ getVolumeId: () => id, onFatal })
    })
    flushSync()
    return {
      setVolumeId: (v: string) => {
        id = v
        flushSync()
      },
    }
  }

  it('falls back when this device disconnects', async () => {
    create('mtp-2097152:65537')
    await vi.waitFor(() => {
      expect(handlers).toHaveLength(1)
    })
    handlers[0]?.({ deviceId: 'mtp-2097152' })
    expect(onFatal).toHaveBeenCalledTimes(1)
  })

  it('ignores another device disconnecting', async () => {
    create('mtp-2097152:65537')
    await vi.waitFor(() => {
      expect(handlers).toHaveLength(1)
    })
    handlers[0]?.({ deviceId: 'mtp-999' })
    expect(onFatal).not.toHaveBeenCalled()
  })

  it('splits on the LAST colon, so a serial-based device id survives', async () => {
    create('mtp-serial:AB:CD:65537')
    await vi.waitFor(() => {
      expect(handlers).toHaveLength(1)
    })
    handlers[0]?.({ deviceId: 'mtp-serial:AB:CD' })
    expect(onFatal).toHaveBeenCalledTimes(1)
  })

  it('registers nothing on a non-MTP pane', () => {
    create('root')
    expect(ipc.onMtpDeviceDisconnected).not.toHaveBeenCalled()
  })

  it('registers nothing for a device-only id (no storage segment yet)', () => {
    create('mtp-2097152')
    expect(ipc.onMtpDeviceDisconnected).not.toHaveBeenCalled()
  })

  it('re-registers against the new device when the pane switches volume', async () => {
    const created = create('mtp-1:5')
    await vi.waitFor(() => {
      expect(handlers).toHaveLength(1)
    })

    created.setVolumeId('mtp-2:5')
    await vi.waitFor(() => {
      expect(handlers).toHaveLength(2)
    })
    await vi.waitFor(() => {
      expect(unlistens[0]).toHaveBeenCalled()
    })

    handlers[1]?.({ deviceId: 'mtp-2' })
    expect(onFatal).toHaveBeenCalledTimes(1)
  })

  it('unlistens on teardown', async () => {
    create('mtp-1:5')
    await vi.waitFor(() => {
      expect(handlers).toHaveLength(1)
    })
    dispose?.()
    dispose = undefined
    await vi.waitFor(() => {
      expect(unlistens[0]).toHaveBeenCalled()
    })
  })
})
