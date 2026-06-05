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
import type { PaneAccess } from './pane-access'
import type { VolumeInfo } from '../types'

/** Tauri drag-drop event payloads (a structural subset of the real shape). */
type DragDropPayload =
  | { type: 'enter'; paths: string[]; position: { x: number; y: number } }
  | { type: 'over'; position: { x: number; y: number } }
  | { type: 'drop'; paths: string[]; position: { x: number; y: number } }
  | { type: 'leave' }

const {
  resolveDropTargetSpy,
  getIsDraggingFromSelfSpy,
  getSelfDragFileInfosSpy,
  setSelfDragResolvedOperationSpy,
  getCachedIconSpy,
  showOverlaySpy,
  updateOverlaySpy,
  hideOverlaySpy,
  startModifierTrackingSpy,
  stopModifierTrackingSpy,
  getModifierStateSpy,
  statPathsKindsSpy,
  listenHandlers,
  dragDropHandlerRef,
} = vi.hoisted(() => ({
  resolveDropTargetSpy: vi.fn<() => DropTarget | null>(),
  getIsDraggingFromSelfSpy: vi.fn<() => boolean>(),
  getSelfDragFileInfosSpy: vi.fn<() => DragFileInfo[] | null>(),
  setSelfDragResolvedOperationSpy: vi.fn<() => Promise<void>>(),
  statPathsKindsSpy: vi.fn<(paths: string[]) => Promise<(boolean | null)[]>>(),
  getCachedIconSpy: vi.fn<(iconId: string) => string | undefined>(),
  showOverlaySpy: vi.fn(),
  updateOverlaySpy: vi.fn(),
  hideOverlaySpy: vi.fn(),
  startModifierTrackingSpy: vi.fn(),
  stopModifierTrackingSpy: vi.fn(),
  getModifierStateSpy: vi.fn<() => { altHeld: boolean; cmdHeld: boolean; shiftHeld: boolean }>(),
  // Captured event-name → handler map, for driving the native listeners in `init()`.
  listenHandlers: new Map<string, (event: { payload: unknown }) => void>(),
  dragDropHandlerRef: { current: null as ((event: { payload: DragDropPayload }) => void) | null },
}))

vi.mock('$lib/tauri-commands', () => ({
  setSelfDragResolvedOperation: setSelfDragResolvedOperationSpy,
  statPathsKinds: statPathsKindsSpy,
  listen: vi.fn((eventName: string, handler: (event: { payload: unknown }) => void) => {
    listenHandlers.set(eventName, handler)
    return Promise.resolve(vi.fn())
  }),
}))

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
import type { createDialogState } from './dialog-state.svelte'
import type { TransferDialogPropsData } from './transfer-operations'

type DialogState = ReturnType<typeof createDialogState>
type ShowTransferSpy = ReturnType<typeof vi.fn<(props: TransferDialogPropsData) => void>>

const SAME_VOL_PATH_A = '/Users/x/a'
const SAME_VOL_PATH_B = '/Users/x/b'
const EXT_VOL_PATH = '/Volumes/Ext/dest'

const ROOT_VOLUME: VolumeInfo = {
  id: 'root',
  name: 'Macintosh HD',
  path: '/',
  volumeType: 'local',
  isReadOnly: false,
  supportsTrash: true,
} as unknown as VolumeInfo

const EXT_VOLUME: VolumeInfo = {
  id: 'ext',
  name: 'Ext',
  path: '/Volumes/Ext',
  volumeType: 'local',
  isReadOnly: false,
  supportsTrash: true,
} as unknown as VolumeInfo

interface AccessConfig {
  focusedPane?: 'left' | 'right'
  paths?: Partial<Record<'left' | 'right', string>>
  volumeIds?: Partial<Record<'left' | 'right', string>>
  volumes?: VolumeInfo[]
}

function buildAccess(config: AccessConfig = {}): PaneAccess {
  const otherPane = (pane: 'left' | 'right'): 'left' | 'right' => (pane === 'left' ? 'right' : 'left')
  return {
    getPaneRef: () => undefined,
    getPanePath: (pane) => config.paths?.[pane] ?? (pane === 'left' ? '/left/dir' : '/right/dir'),
    getPaneVolumeId: (pane) => config.volumeIds?.[pane] ?? 'root',
    getPaneSort: () => ({ sortBy: 'name', sortOrder: 'ascending' }),
    getPaneHistory: () => ({ stack: [], currentIndex: 0 }),
    getFocusedPane: () => config.focusedPane ?? 'left',
    otherPane,
    getShowHiddenFiles: () => true,
    getVolumes: () => config.volumes ?? [ROOT_VOLUME],
    focusContainer: vi.fn(),
  }
}

function buildDialogs(): { dialogs: DialogState; showTransfer: ShowTransferSpy } {
  const showTransfer = vi.fn<(props: TransferDialogPropsData) => void>()
  const dialogs = { showTransfer } as unknown as DialogState
  return { dialogs, showTransfer }
}

/**
 * `handleDrop` fires `handleFileDrop` without awaiting; `handleFileDrop` awaits
 * `statPathsKinds` before opening the dialog. Flush a couple of microtask turns
 * so the dialog open lands before the assertion.
 */
async function flushDrop(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

/** Returns the args of the most recent `updateOverlay` call: [x, y, targetName, canDrop, operation]. */
function lastOverlayArgs(): [number, number, string | null, boolean, 'copy' | 'move'] {
  const calls = updateOverlaySpy.mock.calls
  expect(calls.length).toBeGreaterThan(0)
  return calls[calls.length - 1] as [number, number, string | null, boolean, 'copy' | 'move']
}

/** Returns the captured `onDragDropEvent` handler or throws if `init()` didn't register one. */
function dragDropHandler(): (event: { payload: DragDropPayload }) => void {
  const handler = dragDropHandlerRef.current
  if (!handler) throw new Error("init() didn't register an onDragDropEvent handler")
  return handler
}

/** Returns the captured `listen` handler for an event name or throws if absent. */
function listenHandler(eventName: string): (event: { payload: unknown }) => void {
  const handler = listenHandlers.get(eventName)
  if (!handler) throw new Error(`no listener registered for "${eventName}"`)
  return handler
}

function paneTarget(paneId: 'left' | 'right'): DropTarget {
  return { type: 'pane', paneId }
}

function folderTarget(path: string, paneId: 'left' | 'right' = 'left'): DropTarget {
  return {
    type: 'folder',
    path,
    paneId,
    element: { classList: { add: vi.fn(), remove: vi.fn() } } as unknown as HTMLElement,
  }
}

describe('drag-drop-controller', () => {
  let dispose: (() => void) | undefined

  function create(config: AccessConfig = {}, paneWrapperEls?: Record<'left' | 'right', HTMLDivElement | undefined>) {
    const access = buildAccess(config)
    const { dialogs, showTransfer } = buildDialogs()
    let controller!: ReturnType<typeof createDragDropController>
    dispose = $effect.root(() => {
      controller = createDragDropController({
        access,
        dialogs,
        getPaneWrapperEls: () => paneWrapperEls ?? { left: undefined, right: undefined },
      })
    })
    return { controller, showTransfer }
  }

  beforeEach(() => {
    vi.clearAllMocks()
    listenHandlers.clear()
    dragDropHandlerRef.current = null
    getModifierStateSpy.mockReturnValue({ altHeld: false, cmdHeld: false, shiftHeld: false })
    getIsDraggingFromSelfSpy.mockReturnValue(false)
    getSelfDragFileInfosSpy.mockReturnValue(null)
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

  describe('handleDragEnter', () => {
    it('shows the overlay and starts modifier tracking for a normal drag', () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('left'))
      getIsDraggingFromSelfSpy.mockReturnValue(false)
      const { controller } = create()

      controller.handleDragEnter(['/a/f.txt'], { x: 1, y: 1 })

      expect(showOverlaySpy).toHaveBeenCalledTimes(1)
      expect(startModifierTrackingSpy).toHaveBeenCalledTimes(1)
      // handleDragEnter chains into handleDragOver
      expect(updateOverlaySpy).toHaveBeenCalled()
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

  describe('init + native listeners', () => {
    it('registers the three native-drag listeners', async () => {
      const { controller } = create()
      await controller.init()
      expect(listenHandlers.has('drag-image-size')).toBe(true)
      expect(listenHandlers.has('drag-modifiers')).toBe(true)
      expect(dragDropHandlerRef.current).not.toBeNull()
    })

    it('drives a full enter → over → drop drag cycle through the webview listener', async () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create({ focusedPane: 'left', paths: { right: '/right/dir' } })
      await controller.init()
      const fire = dragDropHandler()

      fire({ payload: { type: 'enter', paths: [SAME_VOL_PATH_A], position: { x: 1, y: 1 } } })
      expect(showOverlaySpy).toHaveBeenCalledTimes(1)
      expect(controller.getDropTargetPane()).toBe('right')

      fire({ payload: { type: 'over', position: { x: 2, y: 2 } } })

      fire({ payload: { type: 'drop', paths: [SAME_VOL_PATH_A], position: { x: 2, y: 2 } } })
      await flushDrop()
      expect(showTransfer).toHaveBeenCalledTimes(1)
      expect(hideOverlaySpy).toHaveBeenCalled()
      // Drop clears the highlight as part of its teardown.
      expect(controller.getDropTargetPane()).toBeNull()
    })

    it('on leave, hides the overlay and clears targets without ending the self-drag session', async () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller } = create()
      await controller.init()
      const fire = dragDropHandler()

      fire({ payload: { type: 'enter', paths: ['/a/f'], position: { x: 1, y: 1 } } })
      expect(controller.getDropTargetPane()).toBe('right')

      fire({ payload: { type: 'leave' } })
      expect(hideOverlaySpy).toHaveBeenCalled()
      expect(stopModifierTrackingSpy).toHaveBeenCalled()
      expect(controller.getDropTargetPane()).toBeNull()
    })

    it('the drag-image-size listener suppresses the overlay for a large external image', async () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      getIsDraggingFromSelfSpy.mockReturnValue(false)
      const { controller } = create()
      await controller.init()
      const fire = dragDropHandler()
      const sizeHandler = listenHandler('drag-image-size')

      sizeHandler({ payload: { width: 200, height: 200 } })
      fire({ payload: { type: 'enter', paths: ['/a/big'], position: { x: 1, y: 1 } } })

      // Large external image → overlay suppressed.
      expect(showOverlaySpy).not.toHaveBeenCalled()
    })

    it('the drag-modifiers listener re-runs handleDragOver at the last position', async () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller } = create()
      await controller.init()
      const fire = dragDropHandler()
      const modHandler = listenHandler('drag-modifiers')

      fire({ payload: { type: 'enter', paths: ['/a/f'], position: { x: 7, y: 9 } } })
      updateOverlaySpy.mockClear()

      modHandler({ payload: { altHeld: true, cmdHeld: false, shiftHeld: false } })
      // Re-evaluated at the last drag position (7, 9).
      const lastCall = lastOverlayArgs()
      expect(lastCall[0]).toBe(7)
      expect(lastCall[1]).toBe(9)
    })

    it('cleanup unsubscribes the listeners and stops modifier tracking', async () => {
      const { controller } = create()
      await controller.init()
      controller.cleanup()
      expect(stopModifierTrackingSpy).toHaveBeenCalled()
    })
  })
})
