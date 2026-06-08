/**
 * Unit tests for the volume-busy store.
 *
 * Covers: bootstrap via `getBusyVolumeIds`, live updates via the
 * `volumes-busy-changed` event, the bootstrap-vs-event race (an event that
 * arrives before the bootstrap resolves must win), and cleanup.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Hoisted mocks: must run before importing the module under test.
const mockGetBusyVolumeIds = vi.fn<() => Promise<string[]>>()
let lastEventHandler: ((payload: { volumeIds: string[] }) => void) | null = null
const mockUnlisten = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  getBusyVolumeIds: () => mockGetBusyVolumeIds(),
  onVolumesBusyChanged: (handler: (payload: { volumeIds: string[] }) => void) => {
    lastEventHandler = handler
    return Promise.resolve(mockUnlisten)
  },
}))

import { initVolumeBusyStore, cleanupVolumeBusyStore, isVolumeBusy } from './volume-busy-store.svelte'

/** Drives the listener as if the backend emitted `volumes-busy-changed`. */
function emit(ids: string[]): void {
  if (!lastEventHandler) throw new Error("init() didn't install a listener")
  lastEventHandler({ volumeIds: ids })
}

describe('volume-busy-store', () => {
  beforeEach(() => {
    mockGetBusyVolumeIds.mockReset()
    mockUnlisten.mockReset()
    lastEventHandler = null
    cleanupVolumeBusyStore()
  })

  afterEach(() => {
    cleanupVolumeBusyStore()
  })

  it('bootstraps the busy set from getBusyVolumeIds', async () => {
    mockGetBusyVolumeIds.mockResolvedValue(['mtp-1:65537'])
    await initVolumeBusyStore()

    expect(isVolumeBusy('mtp-1:65537')).toBe(true)
    expect(isVolumeBusy('root')).toBe(false)
  })

  it('updates live on volumes-busy-changed', async () => {
    mockGetBusyVolumeIds.mockResolvedValue([])
    await initVolumeBusyStore()
    expect(isVolumeBusy('usb-drive')).toBe(false)

    emit(['usb-drive'])
    expect(isVolumeBusy('usb-drive')).toBe(true)

    emit([])
    expect(isVolumeBusy('usb-drive')).toBe(false)
  })

  it('lets an event that arrives before bootstrap resolves win', async () => {
    // The event fires (and we subscribe) before the bootstrap IPC resolves; the
    // bootstrap must not clobber the fresher event payload.
    let resolveBootstrap: (ids: string[]) => void = () => {}
    mockGetBusyVolumeIds.mockReturnValue(
      new Promise<string[]>((resolve) => {
        resolveBootstrap = resolve
      }),
    )

    const initPromise = initVolumeBusyStore()
    emit(['device-x'])
    // Now the (stale) bootstrap resolves with an empty set.
    resolveBootstrap([])
    await initPromise

    expect(isVolumeBusy('device-x')).toBe(true)
  })

  it('clears state and unlistens on cleanup', async () => {
    mockGetBusyVolumeIds.mockResolvedValue(['usb-drive'])
    await initVolumeBusyStore()
    expect(isVolumeBusy('usb-drive')).toBe(true)

    cleanupVolumeBusyStore()
    expect(mockUnlisten).toHaveBeenCalledOnce()
    expect(isVolumeBusy('usb-drive')).toBe(false)
  })
})
