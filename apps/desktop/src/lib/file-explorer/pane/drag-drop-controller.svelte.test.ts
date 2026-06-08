/**
 * Characterization tests for `drag-drop-controller.svelte.ts`, the native
 * drag-and-drop band lifted out of `DualPaneExplorer`. They pin the
 * headless-testable handler logic — overlay file-info assembly, target-path /
 * display-name resolution, the `handleDragOver` state transitions (invalid
 * self-drop, folder vs pane vs null, self-pane suppression), the
 * `pushSelfDragOpIfChanged` dedupe, and the `handleDrop` guard chain — so the
 * verbatim move from the component is provably behavior-preserving.
 *
 * The three Tauri listener registrations (`init()`) and the folder-highlight
 * `$effect` need a real webview / DOM and aren't exercised here; coverage comes
 * from the handlers, which is where the hard-won native-drag behavior lives.
 *
 * This file uses Svelte runes (`$effect.root`), so the filename carries the
 * `.svelte.` infix vite-plugin-svelte's compile-module looks for: the factory
 * creates the folder-highlight `$effect` in its body and must run in a reactive
 * root.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { DropTarget } from '../drag/drop-target-hit-testing'
import type { DragFileInfo } from '../drag/drag-drop'
import {
  type DragDropPayload,
  type AccessConfig,
  SAME_VOL_PATH_A,
  SAME_VOL_PATH_B,
  EXT_VOL_PATH,
  ROOT_VOLUME,
  EXT_VOLUME,
  SD_CARD_VOLUME,
  MTP_VOLUME,
  SMB_VOLUME,
  buildAccess,
  buildDialogs,
  flushDrop,
  paneTarget,
  folderTarget,
} from './drag-drop-controller.test-fixtures'

const {
  resolveDropTargetSpy,
  getIsDraggingFromSelfSpy,
  getSelfDragFileInfosSpy,
  getSelfDragIdentitySpy,
  clearSelfDragIdentitySpy,
  setSelfDragResolvedOperationSpy,
  getCachedIconSpy,
  showOverlaySpy,
  updateOverlaySpy,
  hideOverlaySpy,
  startModifierTrackingSpy,
  stopModifierTrackingSpy,
  getModifierStateSpy,
  statPathsKindsSpy,
  addToastSpy,
  listenHandlers,
  dragDropHandlerRef,
} = vi.hoisted(() => ({
  resolveDropTargetSpy: vi.fn<() => DropTarget | null>(),
  getIsDraggingFromSelfSpy: vi.fn<() => boolean>(),
  getSelfDragFileInfosSpy: vi.fn<() => DragFileInfo[] | null>(),
  getSelfDragIdentitySpy: vi.fn<() => { sourceVolumeId: string; sourcePaths: string[]; startedAt: number } | null>(),
  clearSelfDragIdentitySpy: vi.fn(),
  setSelfDragResolvedOperationSpy: vi.fn<() => Promise<void>>(),
  statPathsKindsSpy: vi.fn<(paths: string[]) => Promise<(boolean | null)[]>>(),
  getCachedIconSpy: vi.fn<(iconId: string) => string | undefined>(),
  showOverlaySpy: vi.fn(),
  updateOverlaySpy: vi.fn(),
  hideOverlaySpy: vi.fn(),
  startModifierTrackingSpy: vi.fn(),
  stopModifierTrackingSpy: vi.fn(),
  getModifierStateSpy: vi.fn<() => { altHeld: boolean; cmdHeld: boolean; shiftHeld: boolean }>(),
  addToastSpy: vi.fn(),
  // Captured event-name → handler map, for driving the native listeners in `init()`.
  listenHandlers: new Map<string, (event: { payload: unknown }) => void>(),
  dragDropHandlerRef: { current: null as ((event: { payload: DragDropPayload }) => void) | null },
}))

vi.mock('$lib/tauri-commands', () => ({
  DEFAULT_VOLUME_ID: 'root',
  setSelfDragResolvedOperation: setSelfDragResolvedOperationSpy,
  statPathsKinds: statPathsKindsSpy,
  // `resolvePathVolume` is the controller's default fallback. Tests that want it
  // to fire inject a per-test spy via `create(..., resolvePathVolume)`; this stub
  // keeps the import resolvable for the default path (no registered-root miss in
  // the common test volumes, so it's never actually called).
  resolvePathVolume: vi.fn(() => Promise.resolve({ volume: null, timedOut: false })),
  listen: vi.fn((eventName: string, handler: (event: { payload: unknown }) => void) => {
    listenHandlers.set(eventName, handler)
    return Promise.resolve(vi.fn())
  }),
  onDragImageSize: vi.fn((handler: (payload: unknown) => void) => {
    listenHandlers.set('drag-image-size', (event) => {
      handler(event.payload)
    })
    return Promise.resolve(vi.fn())
  }),
  onDragModifiers: vi.fn((handler: (payload: unknown) => void) => {
    listenHandlers.set('drag-modifiers', (event) => {
      handler(event.payload)
    })
    return Promise.resolve(vi.fn())
  }),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: addToastSpy }))

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: vi.fn((handler: (event: { payload: DragDropPayload }) => void) => {
      dragDropHandlerRef.current = handler
      return Promise.resolve(vi.fn())
    }),
  }),
}))

vi.mock('../drag/drag-position', () => ({ toViewportPosition: (p: { x: number; y: number }) => p }))

vi.mock('../drag/drop-target-hit-testing', () => ({ resolveDropTarget: resolveDropTargetSpy }))

vi.mock('../drag/drag-drop', () => ({
  getIsDraggingFromSelf: getIsDraggingFromSelfSpy,
  resetDraggingFromSelf: vi.fn(),
  matchesSelfDragFingerprint: vi.fn(() => false),
  markAsSelfDrag: vi.fn(),
  storeSelfDragFingerprint: vi.fn(),
  clearSelfDragFingerprint: vi.fn(),
  getSelfDragFileInfos: getSelfDragFileInfosSpy,
  getSelfDragIdentity: getSelfDragIdentitySpy,
  clearSelfDragIdentity: clearSelfDragIdentitySpy,
  endSelfDragSession: vi.fn(() => Promise.resolve()),
}))

vi.mock('../drag/drag-overlay.svelte.js', () => ({
  showOverlay: showOverlaySpy,
  updateOverlay: updateOverlaySpy,
  hideOverlay: hideOverlaySpy,
}))

vi.mock('$lib/icon-cache', () => ({ getCachedIcon: getCachedIconSpy }))

vi.mock('../modifier-key-tracker.svelte', () => ({
  startModifierTracking: startModifierTrackingSpy,
  stopModifierTracking: stopModifierTrackingSpy,
  getModifierState: getModifierStateSpy,
  setModifiers: vi.fn(),
}))

// Keep `drop-operation` and `drop-target-validation` real (pure helpers), and
// `transfer-operations` real so `handleFileDrop`'s props assembly is exercised
// end-to-end through the actual builder.

import { createDragDropController } from './drag-drop-controller.svelte'

/** Returns the args of the most recent `updateOverlay` call: [x, y, targetName, canDrop, operation]. */
function lastOverlayArgs(): [number, number, string | null, boolean, 'copy' | 'move'] {
  const calls = updateOverlaySpy.mock.calls
  expect(calls.length).toBeGreaterThan(0)
  return calls[calls.length - 1] as [number, number, string | null, boolean, 'copy' | 'move']
}

describe('drag-drop-controller', () => {
  let dispose: (() => void) | undefined

  function create(
    config: AccessConfig = {},
    paneWrapperEls?: Record<'left' | 'right', HTMLDivElement | undefined>,
    resolvePathVolume?: Parameters<typeof createDragDropController>[0]['resolvePathVolume'],
  ) {
    const access = buildAccess(config)
    const { dialogs, showTransfer, showAlert } = buildDialogs()
    let controller!: ReturnType<typeof createDragDropController>
    dispose = $effect.root(() => {
      controller = createDragDropController({
        access,
        dialogs,
        getPaneWrapperEls: () => paneWrapperEls ?? { left: undefined, right: undefined },
        resolvePathVolume,
      })
    })
    return { controller, showTransfer, showAlert }
  }

  beforeEach(() => {
    vi.clearAllMocks()
    listenHandlers.clear()
    dragDropHandlerRef.current = null
    getModifierStateSpy.mockReturnValue({ altHeld: false, cmdHeld: false, shiftHeld: false })
    getIsDraggingFromSelfSpy.mockReturnValue(false)
    getSelfDragFileInfosSpy.mockReturnValue(null)
    getSelfDragIdentitySpy.mockReturnValue(null)
    getCachedIconSpy.mockReturnValue(undefined)
    // Default: kinds unknown so the props builder uses today's approximate
    // shape unless a test opts into a specific split.
    statPathsKindsSpy.mockResolvedValue([])
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  describe('extractFolderName', () => {
    it('returns the last path segment', () => {
      const { controller } = create()
      expect(controller.extractFolderName('/Users/x/Projects')).toBe('Projects')
    })

    it('falls back to the whole path when there is no last segment', () => {
      const { controller } = create()
      expect(controller.extractFolderName('/')).toBe('/')
    })
  })

  describe('buildOverlayFileInfos', () => {
    it('uses stored self-drag infos (with cached icons) when dragging from self', () => {
      getIsDraggingFromSelfSpy.mockReturnValue(true)
      getSelfDragFileInfosSpy.mockReturnValue([
        { name: 'a.txt', isDirectory: false, iconId: 'ext:txt' },
        { name: 'sub', isDirectory: true, iconId: 'folder' },
      ])
      getCachedIconSpy.mockImplementation((id) => `icon:${id}`)
      const { controller } = create()

      const infos = controller.buildOverlayFileInfos(['/ignored'])
      expect(infos).toEqual([
        { name: 'a.txt', iconUrl: 'icon:ext:txt', isDirectory: false },
        { name: 'sub', iconUrl: 'icon:folder', isDirectory: true },
      ])
    })

    it('derives names and extension icons from raw paths for external drags', () => {
      getIsDraggingFromSelfSpy.mockReturnValue(false)
      getCachedIconSpy.mockImplementation((id) => `icon:${id}`)
      const { controller } = create()

      const infos = controller.buildOverlayFileInfos(['/a/photo.png', '/a/noext'])
      expect(infos).toEqual([
        { name: 'photo.png', iconUrl: 'icon:ext:png', isDirectory: false },
        { name: 'noext', iconUrl: undefined, isDirectory: false },
      ])
    })

    it('caps external overlay infos at 20 entries', () => {
      getIsDraggingFromSelfSpy.mockReturnValue(false)
      const { controller } = create()
      const paths = Array.from({ length: 25 }, (_, i) => `/a/f${String(i)}.txt`)
      expect(controller.buildOverlayFileInfos(paths)).toHaveLength(20)
    })
  })

  describe('targetPathOf', () => {
    it('returns null for a null target', () => {
      const { controller } = create()
      expect(controller.targetPathOf(null)).toBeNull()
    })

    it('returns the folder path for a folder target', () => {
      const { controller } = create()
      expect(controller.targetPathOf(folderTarget('/Users/x/sub'))).toBe('/Users/x/sub')
    })

    it("returns the pane's current path for a pane target", () => {
      const { controller } = create({ paths: { right: '/right/here' } })
      expect(controller.targetPathOf(paneTarget('right'))).toBe('/right/here')
    })
  })

  describe('resolveTargetDisplayName', () => {
    it('returns null for a null target', () => {
      const { controller } = create()
      expect(controller.resolveTargetDisplayName(null, null)).toBeNull()
    })

    it('returns the folder basename for a folder target with a path', () => {
      const { controller } = create()
      expect(controller.resolveTargetDisplayName(folderTarget('/a/b'), '/a/b')).toBe('b')
    })

    it("returns the pane path's basename for a pane target", () => {
      const { controller } = create({ paths: { left: '/Users/x/Downloads' } })
      expect(controller.resolveTargetDisplayName(paneTarget('left'), null)).toBe('Downloads')
    })
  })

  describe('handleDragOver state transitions', () => {
    it('highlights a folder target (no pane highlight)', () => {
      resolveDropTargetSpy.mockReturnValue(folderTarget('/Users/x/sub'))
      const { controller } = create()

      controller.handleDragOver({ x: 5, y: 5 })

      expect(controller.getDropTargetPane()).toBeNull()
      const lastCall = lastOverlayArgs()
      expect(lastCall[2]).toBe('sub')
      expect(lastCall[3]).toBe(true)
    })

    it('highlights a pane target', () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller } = create()

      controller.handleDragOver({ x: 5, y: 5 })

      expect(controller.getDropTargetPane()).toBe('right')
    })

    it('suppresses the highlight when a self-drag targets the source pane', () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('left'))
      getIsDraggingFromSelfSpy.mockReturnValue(true)
      const { controller } = create({ focusedPane: 'left' })

      controller.handleDragOver({ x: 5, y: 5 })

      expect(controller.getDropTargetPane()).toBeNull()
      const lastCall = lastOverlayArgs()
      expect(lastCall[3]).toBe(false) // self-pane no-op disallows drop
    })

    it('clears targets when nothing resolves', () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller } = create()
      controller.handleDragOver({ x: 5, y: 5 })
      expect(controller.getDropTargetPane()).toBe('right')

      resolveDropTargetSpy.mockReturnValue(null)
      controller.handleDragOver({ x: 999, y: 999 })
      expect(controller.getDropTargetPane()).toBeNull()
    })
  })

  describe('pushSelfDragOpIfChanged', () => {
    it('does nothing when not dragging from self', () => {
      getIsDraggingFromSelfSpy.mockReturnValue(false)
      const { controller } = create()
      controller.pushSelfDragOpIfChanged('move')
      expect(setSelfDragResolvedOperationSpy).not.toHaveBeenCalled()
    })

    it('pushes once and dedupes repeats of the same op', () => {
      getIsDraggingFromSelfSpy.mockReturnValue(true)
      const { controller } = create()

      controller.pushSelfDragOpIfChanged('move')
      controller.pushSelfDragOpIfChanged('move')
      expect(setSelfDragResolvedOperationSpy).toHaveBeenCalledTimes(1)
      expect(setSelfDragResolvedOperationSpy).toHaveBeenLastCalledWith('move')

      controller.pushSelfDragOpIfChanged('copy')
      expect(setSelfDragResolvedOperationSpy).toHaveBeenCalledTimes(2)
      expect(setSelfDragResolvedOperationSpy).toHaveBeenLastCalledWith('copy')
    })
  })

  describe('handleDrop guard chain', () => {
    it('bails when nothing resolves (no transfer dialog)', () => {
      resolveDropTargetSpy.mockReturnValue(null)
      const { controller, showTransfer } = create()
      controller.handleDrop(['/a/file'], { x: 1, y: 1 })
      expect(showTransfer).not.toHaveBeenCalled()
      // It still tears down overlay + modifier tracking.
      expect(hideOverlaySpy).toHaveBeenCalled()
      expect(stopModifierTrackingSpy).toHaveBeenCalled()
    })

    it('bails on a same-pane self-drop (pane-level, dragging from self)', () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('left'))
      getIsDraggingFromSelfSpy.mockReturnValue(true)
      const { controller, showTransfer } = create({ focusedPane: 'left' })
      controller.handleDrop(['/a/file'], { x: 1, y: 1 })
      expect(showTransfer).not.toHaveBeenCalled()
    })

    it('bails on a descendant drop', () => {
      resolveDropTargetSpy.mockReturnValue(folderTarget('/src/child'))
      const { controller, showTransfer } = create()
      controller.handleDrop(['/src'], { x: 1, y: 1 })
      expect(showTransfer).not.toHaveBeenCalled()
    })

    it('opens the transfer dialog with a copy op for a cross-volume pane drop', async () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        paths: { right: EXT_VOL_PATH },
        volumes: [ROOT_VOLUME, EXT_VOLUME],
      })

      controller.handleDrop([SAME_VOL_PATH_A], { x: 1, y: 1 })
      await flushDrop()

      expect(showTransfer).toHaveBeenCalledTimes(1)
      const props = showTransfer.mock.calls[0][0]
      expect(props.operationType).toBe('copy')
      expect(props.sourcePaths).toEqual([SAME_VOL_PATH_A])
      expect(props.destinationPath).toBe(EXT_VOL_PATH)
      expect(props.direction).toBe('right')
    })

    it('picks move for a same-volume drop into a folder target', async () => {
      resolveDropTargetSpy.mockReturnValue(folderTarget(SAME_VOL_PATH_B, 'left'))
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumes: [ROOT_VOLUME],
      })

      // handleDragOver runs first in the real flow and sets `dropTargetFolderPath`,
      // which handleDrop reads to address the folder row rather than the pane path.
      controller.handleDragOver({ x: 1, y: 1 })
      controller.handleDrop([SAME_VOL_PATH_A], { x: 1, y: 1 })
      await flushDrop()

      expect(showTransfer).toHaveBeenCalledTimes(1)
      const props = showTransfer.mock.calls[0][0]
      expect(props.operationType).toBe('move')
      expect(props.destinationPath).toBe(SAME_VOL_PATH_B)
    })

    it('forces copy when Alt is held even on a same-volume drop', async () => {
      resolveDropTargetSpy.mockReturnValue(folderTarget(SAME_VOL_PATH_B, 'left'))
      getModifierStateSpy.mockReturnValue({ altHeld: true, cmdHeld: false, shiftHeld: false })
      const { controller, showTransfer } = create()

      controller.handleDragOver({ x: 1, y: 1 })
      controller.handleDrop([SAME_VOL_PATH_A], { x: 1, y: 1 })
      await flushDrop()

      expect(showTransfer.mock.calls[0][0].operationType).toBe('copy')
    })
  })

  describe('handleFileDrop', () => {
    it('no-ops on an empty path list', async () => {
      const { controller, showTransfer } = create()
      await controller.handleFileDrop([], 'left')
      expect(showTransfer).not.toHaveBeenCalled()
    })

    it('targets the folder path when one is supplied, else the pane path', async () => {
      const { controller, showTransfer } = create({ paths: { right: '/right/dir' } })

      await controller.handleFileDrop(['/a/f'], 'right', '/right/dir/sub', 'copy')
      expect(showTransfer.mock.calls[0][0].destinationPath).toBe('/right/dir/sub')

      await controller.handleFileDrop(['/a/f'], 'right', undefined, 'move')
      expect(showTransfer.mock.calls[1][0].destinationPath).toBe('/right/dir')
      expect(showTransfer.mock.calls[1][0].operationType).toBe('move')
    })

    it('threads the real file/folder split when statPathsKinds resolves all-known (3 folders)', async () => {
      statPathsKindsSpy.mockResolvedValue([true, true, true])
      const { controller, showTransfer } = create({ paths: { right: '/right/dir' } })

      await controller.handleFileDrop(['/a/one', '/a/two', '/a/three'], 'right', undefined, 'copy')

      expect(statPathsKindsSpy).toHaveBeenCalledWith(['/a/one', '/a/two', '/a/three'])
      const props = showTransfer.mock.calls[0][0]
      expect(props.fileCount).toBe(0)
      expect(props.folderCount).toBe(3)
    })

    it('falls back to the approximate shape when statPathsKinds rejects', async () => {
      statPathsKindsSpy.mockRejectedValue(new Error('stat failed'))
      const { controller, showTransfer } = create({ paths: { right: '/right/dir' } })

      await controller.handleFileDrop(['/a/x', '/a/y'], 'right', undefined, 'copy')

      const props = showTransfer.mock.calls[0][0]
      expect(props.fileCount).toBe(2)
      expect(props.folderCount).toBe(0)
    })
  })

  describe('shared destination guard (a drop must hit the same read-only guard F5 does)', () => {
    it('refuses a drop onto a read-only volume with the exact "Read-only device" alert and no transfer dialog', async () => {
      const { controller, showTransfer, showAlert } = create({
        focusedPane: 'left',
        // Dropping onto the right pane, which is the read-only MTP SD card.
        volumeIds: { left: 'root', right: 'mtp-dev:65538' },
        paths: { right: 'mtp://dev/65538/DCIM' },
        volumes: [ROOT_VOLUME, SD_CARD_VOLUME],
      })

      await controller.handleFileDrop(['/Users/x/photo.jpg'], 'right', undefined, 'copy')

      expect(showAlert).toHaveBeenCalledWith(
        'Read-only device',
        '"Virtual Pixel 9 - SD Card" is read-only. You can copy files from it, but not to it.',
      )
      expect(showTransfer).not.toHaveBeenCalled()
      // The guard short-circuits before any stat / volume-resolution work.
      expect(statPathsKindsSpy).not.toHaveBeenCalled()
    })

    it('allows a drop onto a writable volume (guard passes, dialog opens)', async () => {
      const { controller, showTransfer, showAlert } = create({
        focusedPane: 'left',
        volumeIds: { left: 'root', right: 'ext' },
        paths: { right: EXT_VOL_PATH },
        volumes: [ROOT_VOLUME, EXT_VOLUME],
      })

      await controller.handleFileDrop(['/Users/x/photo.jpg'], 'right', undefined, 'copy')

      expect(showAlert).not.toHaveBeenCalled()
      expect(showTransfer).toHaveBeenCalledTimes(1)
    })
  })

  describe('resolved source volume (a wrong source volume id makes the preview report zeros)', () => {
    it('resolves an MTP source dropped onto a local dest to the MTP volume (not the dest)', async () => {
      // Drop an MTP-shaped path onto a local destination. The source volume must
      // be the MTP volume so the scan preview stats the right shape; the old
      // `sourceVolumeId = destVolumeId` placeholder reported the local dest and
      // the counters came back empty.
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumeIds: { left: 'root', right: 'root' },
        paths: { right: '/Users/x/dest' },
        volumes: [ROOT_VOLUME, MTP_VOLUME],
      })

      await controller.handleFileDrop(['mtp://dev/65537/DCIM/IMG_0001.JPG'], 'right', undefined, 'copy')

      expect(showTransfer).toHaveBeenCalledTimes(1)
      const props = showTransfer.mock.calls[0][0]
      expect(props.sourceVolumeId).toBe('mtp-dev:65537')
      expect(props.destVolumeId).toBe('root')
    })

    it('resolves a local source dropped onto an MTP dest to the local volume', async () => {
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumeIds: { left: 'root', right: 'mtp-dev:65537' },
        paths: { right: 'mtp://dev/65537/DCIM' },
        volumes: [ROOT_VOLUME, MTP_VOLUME],
      })

      await controller.handleFileDrop(['/Users/x/photo.jpg'], 'right', undefined, 'copy')

      const props = showTransfer.mock.calls[0][0]
      expect(props.sourceVolumeId).toBe('root')
      expect(props.destVolumeId).toBe('mtp-dev:65537')
    })

    it('falls back to the backend resolver when the dropped path matches no registered root', async () => {
      const resolvePathVolume = vi.fn(() => Promise.resolve({ volume: EXT_VOLUME, timedOut: false }))
      const { controller, showTransfer } = create(
        {
          focusedPane: 'left',
          volumeIds: { left: 'root', right: 'root' },
          paths: { right: '/Users/x/dest' },
          // No `/`-rooted volume registered, so the FE longest-prefix can't match
          // the SMB path; the controller asks the backend.
          volumes: [EXT_VOLUME, MTP_VOLUME],
        },
        undefined,
        resolvePathVolume,
      )

      await controller.handleFileDrop(['smb://server/share/file.txt'], 'right', undefined, 'copy')

      expect(resolvePathVolume).toHaveBeenCalledWith('smb://server/share')
      expect(showTransfer.mock.calls[0][0].sourceVolumeId).toBe('ext')
    })

    it('reports the honest unknown (root) when sources span volumes', async () => {
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumeIds: { left: 'root', right: 'root' },
        paths: { right: '/Users/x/dest' },
        volumes: [ROOT_VOLUME, EXT_VOLUME],
      })

      // One path on Ext, one on root → spanning → honest-unknown source (root).
      await controller.handleFileDrop(['/Volumes/Ext/a.txt', '/Users/x/b.txt'], 'right', undefined, 'copy')

      expect(showTransfer.mock.calls[0][0].sourceVolumeId).toBe('root')
    })
  })

  describe('self-drag identity (recorded at drag start, consumed on drop — never the pasteboard round-trip)', () => {
    it('an MTP self-drag onto a local pane builds the transfer from the recorded MTP identity, ignoring the pasteboard paths', async () => {
      // The live failure: in-app drag from the virtual-MTP pane. The MTP listing's
      // RELATIVE path (`/photos/sunset.jpg`) lands on the pasteboard and round-trips
      // through wry's drop event looking exactly like a local absolute path. The
      // resolver can't match it to the MTP volume and falls back to local, so the
      // dialog showed 0 bytes / 0 files. With a recorded identity, the drop builds
      // the request from the MTP volume id + the relative paths the volume knows.
      getIsDraggingFromSelfSpy.mockReturnValue(true)
      getSelfDragIdentitySpy.mockReturnValue({
        sourceVolumeId: 'mtp-dev:65537',
        sourcePaths: ['/photos/sunset.jpg'],
        startedAt: 1000,
      })
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumeIds: { left: 'mtp-dev:65537', right: 'root' },
        paths: { right: '/Users/x/dest' },
        volumes: [ROOT_VOLUME, MTP_VOLUME],
      })

      // The pasteboard carries the bare relative path (the bug's exact shape).
      controller.handleDrop(['/photos/sunset.jpg'], { x: 1, y: 1 })
      await flushDrop()

      expect(showTransfer).toHaveBeenCalledTimes(1)
      const props = showTransfer.mock.calls[0][0]
      // Built from the RECORDED identity, not the resolver (which would say root).
      expect(props.sourceVolumeId).toBe('mtp-dev:65537')
      expect(props.sourcePaths).toEqual(['/photos/sunset.jpg'])
      expect(props.destVolumeId).toBe('root')
      // The resolver/stat path must NOT run for a recorded self-drag.
      expect(statPathsKindsSpy).not.toHaveBeenCalled()
    })

    it('an SMB-native self-drag onto local uses the recorded SMB identity (same class — no local paths)', async () => {
      getIsDraggingFromSelfSpy.mockReturnValue(true)
      getSelfDragIdentitySpy.mockReturnValue({
        sourceVolumeId: 'smb-server-share',
        sourcePaths: ['/dir/report.pdf', '/dir/notes.txt'],
        startedAt: 2000,
      })
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumeIds: { left: 'smb-server-share', right: 'root' },
        paths: { right: '/Users/x/dest' },
        volumes: [ROOT_VOLUME, SMB_VOLUME],
      })

      controller.handleDrop(['/dir/report.pdf', '/dir/notes.txt'], { x: 1, y: 1 })
      await flushDrop()

      const props = showTransfer.mock.calls[0][0]
      expect(props.sourceVolumeId).toBe('smb-server-share')
      expect(props.sourcePaths).toEqual(['/dir/report.pdf', '/dir/notes.txt'])
    })

    it('a search-results self-drag (virtual id, real absolute paths) falls through to the resolver, not the recorded identity', async () => {
      // Search-results drags carry the snapshot's REAL absolute paths, which may
      // span volumes. The recorded `sourceVolumeId: 'search-results'` is a virtual
      // volume the backend can't dispatch against and isn't in the volume
      // registry, so the consume must decline it and let the resolver match each
      // absolute path to its real volume.
      getIsDraggingFromSelfSpy.mockReturnValue(true)
      getSelfDragIdentitySpy.mockReturnValue({
        sourceVolumeId: 'search-results',
        sourcePaths: ['/Users/x/found.txt'],
        startedAt: 5000,
      })
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumeIds: { left: 'search-results', right: 'root' },
        paths: { right: '/Users/y/dest' },
        volumes: [ROOT_VOLUME], // no 'search-results' volume registered
      })

      controller.handleDrop(['/Users/x/found.txt'], { x: 1, y: 1 })
      await flushDrop()

      const props = showTransfer.mock.calls[0][0]
      // Resolver wins: the real absolute path resolves to root, not the virtual id.
      expect(props.sourceVolumeId).toBe('root')
      expect(props.sourcePaths).toEqual(['/Users/x/found.txt'])
      // The resolver path runs the kind probe; the identity path skips it.
      expect(statPathsKindsSpy).toHaveBeenCalled()
    })

    it('a local self-drag with a recorded identity is unaffected (identity matches what the resolver would say)', async () => {
      getIsDraggingFromSelfSpy.mockReturnValue(true)
      getSelfDragIdentitySpy.mockReturnValue({
        sourceVolumeId: 'root',
        sourcePaths: [SAME_VOL_PATH_A],
        startedAt: 3000,
      })
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumeIds: { left: 'root', right: 'ext' },
        paths: { right: EXT_VOL_PATH },
        volumes: [ROOT_VOLUME, EXT_VOLUME],
      })

      controller.handleDrop([SAME_VOL_PATH_A], { x: 1, y: 1 })
      await flushDrop()

      const props = showTransfer.mock.calls[0][0]
      expect(props.sourceVolumeId).toBe('root')
      expect(props.sourcePaths).toEqual([SAME_VOL_PATH_A])
      expect(props.operationType).toBe('copy') // root → ext is cross-volume
    })

    it('an external drop (no self-drag, no recorded identity) keeps the resolver path', async () => {
      // Genuine Finder drop: not a self-drag, no recorded identity. The paths are
      // real local absolute paths, so the resolver runs as before.
      getIsDraggingFromSelfSpy.mockReturnValue(false)
      getSelfDragIdentitySpy.mockReturnValue(null)
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumeIds: { left: 'root', right: 'root' },
        paths: { right: '/Users/x/dest' },
        volumes: [ROOT_VOLUME, MTP_VOLUME],
      })

      controller.handleDrop(['mtp://dev/65537/DCIM/IMG_0001.JPG'], { x: 1, y: 1 })
      await flushDrop()

      const props = showTransfer.mock.calls[0][0]
      // Resolver matched the MTP root via longest-prefix (the pasteboard carried a
      // full mtp:// path, as a genuine external drop of such a path would).
      expect(props.sourceVolumeId).toBe('mtp-dev:65537')
      expect(props.sourcePaths).toEqual(['mtp://dev/65537/DCIM/IMG_0001.JPG'])
      expect(statPathsKindsSpy).toHaveBeenCalled()
    })

    it('a stale-cleared record (self-drag flag reset) falls back to the resolver, never claiming a later external drop', async () => {
      // The drag ended (resetDraggingFromSelf ran), so the flag is false even
      // though a record might linger. The consume is tied to the self-drag flag,
      // so the resolver runs for this genuine external drop.
      getIsDraggingFromSelfSpy.mockReturnValue(false)
      getSelfDragIdentitySpy.mockReturnValue({
        sourceVolumeId: 'mtp-dev:65537',
        sourcePaths: ['/photos/old.jpg'],
        startedAt: 4000,
      })
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create({
        focusedPane: 'left',
        volumeIds: { left: 'root', right: 'root' },
        paths: { right: '/Users/x/dest' },
        volumes: [ROOT_VOLUME, EXT_VOLUME],
      })

      controller.handleDrop(['/Volumes/Ext/genuine.txt'], { x: 1, y: 1 })
      await flushDrop()

      const props = showTransfer.mock.calls[0][0]
      // Resolver wins: the real dropped path on Ext, not the stale MTP record.
      expect(props.sourceVolumeId).toBe('ext')
      expect(props.sourcePaths).toEqual(['/Volumes/Ext/genuine.txt'])
    })
  })

  describe('clearDropTargets', () => {
    it('clears the pane highlight', () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller } = create()
      controller.handleDragOver({ x: 1, y: 1 })
      expect(controller.getDropTargetPane()).toBe('right')

      controller.clearDropTargets()
      expect(controller.getDropTargetPane()).toBeNull()
    })
  })
})
