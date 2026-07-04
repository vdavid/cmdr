/**
 * Tests for `volume-space.svelte.ts`, the file pane's live disk-space readout.
 * They pin:
 * - refresh fetches for the current path but clears (no fetch) on a disk image,
 * - the live-event listener updates only for the pane's volume, skipping mismatched
 *   ids and disk images,
 * - watch / unwatch / cleanup register and tear down keyed by the pane's id.
 *
 * The factory holds `$state` but creates no `$effect`, so it's driven directly
 * (no `$effect.root`); the `.svelte.` infix is for the rune support.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { VolumeSpaceInfo } from '$lib/tauri-commands'

const { ipc } = vi.hoisted(() => ({
  ipc: {
    getVolumeSpace: vi.fn(),
    watchVolumeSpace: vi.fn(),
    unwatchVolumeSpace: vi.fn(),
    onVolumeSpaceChanged: vi.fn(),
  },
}))

vi.mock('$lib/tauri-commands', () => ({
  getVolumeSpace: ipc.getVolumeSpace,
  watchVolumeSpace: ipc.watchVolumeSpace,
  unwatchVolumeSpace: ipc.unwatchVolumeSpace,
  onVolumeSpaceChanged: ipc.onVolumeSpaceChanged,
}))

import { createVolumeSpace, type VolumeSpaceDeps } from './volume-space.svelte'

const space: VolumeSpaceInfo = { totalBytes: 1000, availableBytes: 400 }

function setup(over: Partial<VolumeSpaceDeps> = {}) {
  const deps: VolumeSpaceDeps = {
    paneId: 'left',
    getVolumeId: () => 'vol-1',
    getCurrentPath: () => '/vol-1/dir',
    getIsDiskImage: () => false,
    ...over,
  }
  return createVolumeSpace(deps)
}

describe('createVolumeSpace', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.getVolumeSpace.mockResolvedValue({ data: space })
    ipc.onVolumeSpaceChanged.mockResolvedValue(vi.fn())
  })

  it('refresh fetches space for the current path', async () => {
    const ctl = setup()
    await ctl.refresh()
    expect(ipc.getVolumeSpace).toHaveBeenCalledWith('/vol-1/dir')
    expect(ctl.volumeSpace).toEqual(space)
  })

  it('refresh clears the readout without fetching on a disk image', async () => {
    const ctl = setup({ getIsDiskImage: () => true })
    await ctl.refresh()
    expect(ipc.getVolumeSpace).not.toHaveBeenCalled()
    expect(ctl.volumeSpace).toBeNull()
  })

  it('the live event updates the readout for the pane volume', () => {
    let cb: ((p: { volumeId: string; totalBytes: number; availableBytes: number }) => void) | undefined
    ipc.onVolumeSpaceChanged.mockImplementation((fn: typeof cb) => {
      cb = fn
      return Promise.resolve(vi.fn())
    })
    const ctl = setup()
    ctl.startListening()
    cb?.({ volumeId: 'vol-1', totalBytes: 2000, availableBytes: 900 })
    expect(ctl.volumeSpace).toEqual({ totalBytes: 2000, availableBytes: 900 })
  })

  it('the live event ignores a mismatched volume id', () => {
    let cb: ((p: { volumeId: string; totalBytes: number; availableBytes: number }) => void) | undefined
    ipc.onVolumeSpaceChanged.mockImplementation((fn: typeof cb) => {
      cb = fn
      return Promise.resolve(vi.fn())
    })
    const ctl = setup()
    ctl.startListening()
    cb?.({ volumeId: 'other', totalBytes: 2000, availableBytes: 900 })
    expect(ctl.volumeSpace).toBeNull()
  })

  it('the live event is ignored on a disk image', () => {
    let cb: ((p: { volumeId: string; totalBytes: number; availableBytes: number }) => void) | undefined
    ipc.onVolumeSpaceChanged.mockImplementation((fn: typeof cb) => {
      cb = fn
      return Promise.resolve(vi.fn())
    })
    const ctl = setup({ getIsDiskImage: () => true })
    ctl.startListening()
    cb?.({ volumeId: 'vol-1', totalBytes: 2000, availableBytes: 900 })
    expect(ctl.volumeSpace).toBeNull()
  })

  it('watch and unwatch register keyed by the pane id', () => {
    const ctl = setup()
    ctl.watch('vol-2', '/vol-2')
    expect(ipc.watchVolumeSpace).toHaveBeenCalledWith('left', 'vol-2', '/vol-2')
    ctl.unwatch()
    expect(ipc.unwatchVolumeSpace).toHaveBeenCalledWith('left')
  })

  it('clear nulls the readout', async () => {
    const ctl = setup()
    await ctl.refresh()
    expect(ctl.volumeSpace).toEqual(space)
    ctl.clear()
    expect(ctl.volumeSpace).toBeNull()
  })

  it('cleanup drops the listener and unwatches this pane', async () => {
    const unlisten = vi.fn()
    ipc.onVolumeSpaceChanged.mockResolvedValue(unlisten)
    const ctl = setup()
    ctl.startListening()
    await vi.waitFor(() => { expect(ipc.onVolumeSpaceChanged).toHaveBeenCalled(); })
    // Let the `.then(fn => unlisten = fn)` microtask settle.
    await Promise.resolve()
    ctl.cleanup()
    expect(unlisten).toHaveBeenCalled()
    expect(ipc.unwatchVolumeSpace).toHaveBeenCalledWith('left')
  })
})
