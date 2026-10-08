/**
 * Tests for `volume-space.svelte.ts`, the file pane's live disk-space readout.
 *
 * ❗ The readout is keyed by the volume the pane is ON (`getSpaceVolume`, the
 * `paneVolumeOf` answer), and follows it by any route: a switcher pick, a walk-up
 * after an eject, a navigation into another drive. It never shows another
 * volume's figure: it clears the moment the volume changes, and an answer for a
 * volume the pane has since left is dropped (QA round 5, R4-1).
 *
 * The factory owns an `$effect`, so each case runs inside `$effect.root`.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { SpaceInfo, VolumeSpaceChanged } from '$lib/ipc/bindings'

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

import { createVolumeSpace, type SpaceVolume, type VolumeSpace } from './volume-space.svelte'

const bootDisk: SpaceInfo = { kind: 'bounded', totalBytes: 926, availableBytes: 253, usedBytes: 673 }
const share: SpaceInfo = { kind: 'bounded', totalBytes: 328, availableBytes: 232, usedBytes: 96 }

const root: SpaceVolume = { id: 'root', path: '/', isDiskImage: false, isLive: true }
const smb: SpaceVolume = { id: 'smb-p', path: '/Volumes/private', isDiskImage: false, isLive: true }

describe('createVolumeSpace', () => {
  let dispose: (() => void) | undefined
  let volume = $state<SpaceVolume | null>(root)

  function setup(): VolumeSpace {
    let ctl!: VolumeSpace
    dispose = $effect.root(() => {
      ctl = createVolumeSpace({
        paneId: 'left',
        getSpaceVolume: () => volume,
      })
    })
    flushSync()
    return ctl
  }

  beforeEach(() => {
    vi.clearAllMocks()
    volume = root
    ipc.getVolumeSpace.mockImplementation((path: string) =>
      Promise.resolve({ data: path.startsWith('/Volumes/private') ? share : bootDisk }),
    )
    ipc.onVolumeSpaceChanged.mockResolvedValue(vi.fn())
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('fetches and watches the space of the volume the pane is on', async () => {
    const ctl = setup()
    await vi.waitFor(() => {
      expect(ctl.volumeSpace).toEqual(bootDisk)
    })
    expect(ipc.getVolumeSpace).toHaveBeenCalledWith('/')
    expect(ipc.watchVolumeSpace).toHaveBeenCalledWith('left', 'root', '/')
  })

  /** ❗ An eject walked the pane to the boot disk and it kept the share's 232 of 328. */
  it('clears at once and re-fetches when the pane moves to another volume by any route', async () => {
    volume = smb
    const ctl = setup()
    await vi.waitFor(() => {
      expect(ctl.volumeSpace).toEqual(share)
    })

    volume = root
    flushSync()
    expect(ctl.volumeSpace, 'never the old volume’s figure').toBeNull()
    await vi.waitFor(() => {
      expect(ctl.volumeSpace).toEqual(bootDisk)
    })
    expect(ipc.unwatchVolumeSpace).toHaveBeenCalledWith('left')
    expect(ipc.watchVolumeSpace).toHaveBeenLastCalledWith('left', 'root', '/')
  })

  it('stays put, with no new fetch, while the pane stays on one volume', async () => {
    const ctl = setup()
    await vi.waitFor(() => {
      expect(ctl.volumeSpace).toEqual(bootDisk)
    })
    // A volume-list refresh hands over an equal row as a new object.
    volume = { ...root }
    flushSync()
    expect(ctl.volumeSpace).toEqual(bootDisk)
    expect(ipc.getVolumeSpace).toHaveBeenCalledTimes(1)
  })

  /**
   * ❗ A saved share and the live share it becomes are ONE volume id at one path:
   * keyed on those alone, the change from placeholder to live was no change, and
   * the readout stayed blank until the poller happened to emit (QA round 6, R5-1).
   */
  it('shows nothing for a saved placeholder, and fetches the moment the same volume goes live', async () => {
    volume = { ...smb, isLive: false }
    const ctl = setup()
    expect(ctl.volumeSpace).toBeNull()
    expect(ipc.getVolumeSpace, 'an unmounted share’s path is a boot-disk folder').not.toHaveBeenCalled()
    expect(ipc.watchVolumeSpace).not.toHaveBeenCalled()

    volume = { ...smb, isLive: true }
    flushSync()
    await vi.waitFor(() => {
      expect(ctl.volumeSpace).toEqual(share)
    })
    expect(ipc.watchVolumeSpace).toHaveBeenCalledWith('left', 'smb-p', '/Volumes/private')
  })

  it('drops an answer for a volume the pane has since left', async () => {
    let answerShare!: (value: { data: SpaceInfo }) => void
    ipc.getVolumeSpace.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          answerShare = resolve
        }),
    )
    volume = smb
    const ctl = setup()
    volume = root
    flushSync()
    answerShare({ data: share })
    await vi.waitFor(() => {
      expect(ctl.volumeSpace).toEqual(bootDisk)
    })
  })

  it('shows nothing, and watches nothing, on a disk image or a view with no volume', () => {
    volume = { id: 'dmg', path: '/Volumes/Installer', isDiskImage: true, isLive: true }
    const ctl = setup()
    expect(ctl.volumeSpace).toBeNull()
    expect(ipc.getVolumeSpace).not.toHaveBeenCalled()
    expect(ipc.watchVolumeSpace).not.toHaveBeenCalled()

    volume = null
    flushSync()
    expect(ctl.volumeSpace).toBeNull()
    expect(ipc.getVolumeSpace).not.toHaveBeenCalled()
  })

  /**
   * ❗ Asked by the VOLUME's path, never the pane's. An eject resolved the pane to
   * the boot disk while its path was still the dead `/Volumes/private`; the one
   * fetch for that volume asked the dead path, got nothing, and the readout stayed
   * blank on every folder after (live, QA round 5).
   */
  it('asks the volume’s own path, so a pane still on a dead path gets its new volume’s figure', async () => {
    volume = smb
    const ctl = setup()
    ipc.getVolumeSpace.mockImplementation((path: string) => Promise.resolve({ data: path === '/' ? bootDisk : null }))
    volume = root
    flushSync()
    await vi.waitFor(() => {
      expect(ctl.volumeSpace).toEqual(bootDisk)
    })
  })

  it('reads an archive’s space off the volume it sits on', async () => {
    setup()
    await vi.waitFor(() => {
      expect(ipc.getVolumeSpace).toHaveBeenCalledWith('/')
    })
  })

  it('takes live events for the volume the pane is on, and no other', async () => {
    let cb: ((p: VolumeSpaceChanged) => void) | undefined
    ipc.onVolumeSpaceChanged.mockImplementation((fn: typeof cb) => {
      cb = fn
      return Promise.resolve(vi.fn())
    })
    const ctl = setup()
    ctl.startListening()
    await vi.waitFor(() => {
      expect(ctl.volumeSpace).toEqual(bootDisk)
    })
    cb?.({ volumeId: 'smb-p', space: share })
    expect(ctl.volumeSpace).toEqual(bootDisk)
    const fresher: SpaceInfo = { kind: 'bounded', totalBytes: 926, availableBytes: 250, usedBytes: 676 }
    cb?.({ volumeId: 'root', space: fresher })
    expect(ctl.volumeSpace).toEqual(fresher)
  })

  /**
   * ❗ A remote volume's path (`sftp://…`) isn't in the mount table, so the fetch
   * answers `null`, and its figure comes only from the poller. A slow `null`
   * landing after that figure blanked the readout again.
   */
  it('keeps a live figure when the fetch for the same volume can’t tell', async () => {
    let cb: ((p: VolumeSpaceChanged) => void) | undefined
    ipc.onVolumeSpaceChanged.mockImplementation((fn: typeof cb) => {
      cb = fn
      return Promise.resolve(vi.fn())
    })
    let answerNull!: (value: { data: null }) => void
    ipc.getVolumeSpace.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          answerNull = resolve
        }),
    )
    volume = { id: 'sftp-nas', path: 'sftp://ada@nas:22/srv', isDiskImage: false, isLive: true }
    const ctl = setup()
    ctl.startListening()
    await vi.waitFor(() => {
      expect(cb).toBeDefined()
    })
    cb?.({ volumeId: 'sftp-nas', space: share })
    answerNull({ data: null })
    await Promise.resolve()
    await Promise.resolve()
    expect(ctl.volumeSpace).toEqual(share)
  })

  it('cleanup drops the listener and unwatches this pane', async () => {
    const unlisten = vi.fn()
    ipc.onVolumeSpaceChanged.mockResolvedValue(unlisten)
    const ctl = setup()
    ctl.startListening()
    await vi.waitFor(() => {
      expect(ipc.onVolumeSpaceChanged).toHaveBeenCalled()
    })
    await Promise.resolve()
    ctl.cleanup()
    expect(unlisten).toHaveBeenCalled()
    expect(ipc.unwatchVolumeSpace).toHaveBeenCalledWith('left')
  })
})
