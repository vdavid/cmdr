/**
 * Unit tests for the volume-busy store.
 *
 * Covers: bootstrap via `getBusyVolumeIds`, live updates via the
 * `volumes-busy-changed` event, the bootstrap-vs-event race (an event that
 * arrives before the bootstrap resolves must win), cleanup, and the ejecting
 * set that rides beside the busy one on `volumes-ejecting-changed`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Hoisted mocks: must run before importing the module under test.
const mockGetBusyVolumeIds = vi.fn<() => Promise<string[]>>()
let lastEventHandler: ((payload: { volumeIds: string[] }) => void) | null = null
const mockUnlisten = vi.fn()
const mockGetEjectingVolumeIds = vi.fn<() => Promise<string[]>>()
let lastEjectingHandler: ((payload: { volumeIds: string[] }) => void) | null = null
const mockUnlistenEjecting = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  getBusyVolumeIds: () => mockGetBusyVolumeIds(),
  onVolumesBusyChanged: (handler: (payload: { volumeIds: string[] }) => void) => {
    lastEventHandler = handler
    return Promise.resolve(mockUnlisten)
  },
  getEjectingVolumeIds: () => mockGetEjectingVolumeIds(),
  onVolumesEjectingChanged: (handler: (payload: { volumeIds: string[] }) => void) => {
    lastEjectingHandler = handler
    return Promise.resolve(mockUnlistenEjecting)
  },
}))

import { initVolumeBusyStore, cleanupVolumeBusyStore, isVolumeBusy, isVolumeEjecting } from './volume-busy-store.svelte'

/** Drives the listener as if the backend emitted `volumes-busy-changed`. */
function emit(ids: string[]): void {
  if (!lastEventHandler) throw new Error("init() didn't install a listener")
  lastEventHandler({ volumeIds: ids })
}

/** Drives the listener as if the backend emitted `volumes-ejecting-changed`. */
function emitEjecting(ids: string[]): void {
  if (!lastEjectingHandler) throw new Error("init() didn't install an ejecting listener")
  lastEjectingHandler({ volumeIds: ids })
}

describe('volume-busy-store', () => {
  beforeEach(() => {
    mockGetBusyVolumeIds.mockReset()
    mockUnlisten.mockReset()
    lastEventHandler = null
    mockGetEjectingVolumeIds.mockReset()
    mockGetEjectingVolumeIds.mockResolvedValue([])
    mockUnlistenEjecting.mockReset()
    lastEjectingHandler = null
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

  it('tracks the ejecting set on its own event, apart from the busy set', async () => {
    mockGetBusyVolumeIds.mockResolvedValue([])
    mockGetEjectingVolumeIds.mockResolvedValue(['usb-drive'])
    await initVolumeBusyStore()

    expect(isVolumeEjecting('usb-drive')).toBe(true)
    expect(isVolumeBusy('usb-drive')).toBe(false)

    emitEjecting([])
    expect(isVolumeEjecting('usb-drive')).toBe(false)

    emitEjecting(['sd-card'])
    expect(isVolumeEjecting('sd-card')).toBe(true)
    expect(isVolumeBusy('sd-card')).toBe(false)
  })

  it('lets an ejecting event that arrives before its bootstrap resolves win', async () => {
    mockGetBusyVolumeIds.mockResolvedValue([])
    let resolveBootstrap: (ids: string[]) => void = () => {}
    mockGetEjectingVolumeIds.mockReturnValue(
      new Promise<string[]>((resolve) => {
        resolveBootstrap = resolve
      }),
    )

    const initPromise = initVolumeBusyStore()
    await vi.waitFor(() => {
      expect(lastEjectingHandler).not.toBeNull()
    })
    emitEjecting(['usb-drive'])
    resolveBootstrap([])
    await initPromise

    expect(isVolumeEjecting('usb-drive')).toBe(true)
  })

  it('clears the ejecting set and unlistens on cleanup', async () => {
    mockGetBusyVolumeIds.mockResolvedValue([])
    mockGetEjectingVolumeIds.mockResolvedValue(['usb-drive'])
    await initVolumeBusyStore()
    expect(isVolumeEjecting('usb-drive')).toBe(true)

    cleanupVolumeBusyStore()
    expect(mockUnlistenEjecting).toHaveBeenCalledOnce()
    expect(isVolumeEjecting('usb-drive')).toBe(false)
  })
})
