/**
 * Characterization tests for `drag-drop-controller.svelte.ts`'s native-listener
 * band: `init()`'s three Tauri listener registrations, the webview
 * `onDragDropEvent` enter→over→drop cycle, the leave teardown, the
 * drag-image-size + drag-modifiers listeners, and `cleanup()`'s unsubscribe.
 *
 * Split out of `drag-drop-controller.svelte.test.ts` (which keeps the headless
 * handler-contract suites). The shared volume fixtures + `buildAccess` /
 * `buildDialogs` / `paneTarget` / `folderTarget` / `flushDrop` builders are
 * imported from `drag-drop-controller.test-fixtures.ts`.
 *
 * ⚠️ The `vi.hoisted` + `vi.mock(...)` block below is INTENTIONALLY DUPLICATED
 * with the sibling `drag-drop-controller.svelte.test.ts`. Vitest hoists
 * `vi.mock` PER TEST FILE — the factories can't move into the imported fixtures
 * (they'd not be hoisted into this file's module graph), so each test file
 * carries its own full mock block. The handler helpers that read these hoisted
 * spies (`lastOverlayArgs`, `dragDropHandler`, `listenHandler`) are duplicated
 * here too, for the same reason.
 *
 * Uses Svelte runes (`$effect.root`), so the filename carries the `.svelte.`
 * infix vite-plugin-svelte's compile-module looks for.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { DropTarget } from '../drag/drop-target-hit-testing'
import type { DragFileInfo } from '../drag/drag-drop'
import {
  type DragDropPayload,
  type AccessConfig,
  SAME_VOL_PATH_A,
  buildAccess,
  buildDialogs,
  paneTarget,
  flushDrop,
} from './drag-drop-controller.test-fixtures'

const {
  resolveDropTargetSpy,
  getIsDraggingFromSelfSpy,
  getSelfDragFileInfosSpy,
  getSelfDragIdentitySpy,
  setSelfDragResolvedOperationSpy,
  getCachedIconSpy,
  showOverlaySpy,
  updateOverlaySpy,
  hideOverlaySpy,
  startModifierTrackingSpy,
  stopModifierTrackingSpy,
  getModifierStateSpy,
  statPathsKindsSpy,
  storedSpellingsSpy,
  addToastSpy,
  trackEventSpy,
  releasePromiseDropSpy,
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
  storedSpellingsSpy: vi.fn<(volumeId: string, paths: string[]) => Promise<string[]>>(),
  getCachedIconSpy: vi.fn<(iconId: string) => string | undefined>(),
  showOverlaySpy: vi.fn(),
  updateOverlaySpy: vi.fn(),
  hideOverlaySpy: vi.fn(),
  startModifierTrackingSpy: vi.fn(),
  stopModifierTrackingSpy: vi.fn(),
  getModifierStateSpy: vi.fn<() => { altHeld: boolean; cmdHeld: boolean; shiftHeld: boolean }>(),
  addToastSpy: vi.fn(),
  trackEventSpy: vi.fn(() => Promise.resolve()),
  releasePromiseDropSpy: vi.fn(() => Promise.resolve()),
  // Captured event-name → handler map, for driving the native listeners in `init()`.
  listenHandlers: new Map<string, (event: { payload: unknown }) => void>(),
  dragDropHandlerRef: { current: null as ((event: { payload: DragDropPayload }) => void) | null },
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  DEFAULT_VOLUME_ID: 'root',
  // The drop path reports one PII-free event per drop; the emitter is fire-and-forget
  // so a stub is all it needs (`drag/drag-analytics.ts`).
  trackEvent: trackEventSpy,
  itemCountBucket: (n: number) => String(n),
  setSelfDragResolvedOperation: setSelfDragResolvedOperationSpy,
  statPathsKinds: statPathsKindsSpy,
  storedSpellings: storedSpellingsSpy,
  // `resolvePathVolume` is the controller's default fallback. Tests that want it
  // to fire inject a per-test spy via `create(..., resolvePathVolume)`; this stub
  // keeps the import resolvable for the default path (no registered-root miss in
  // the common test volumes, so it's never actually called).
  resolvePathVolume: vi.fn(() => Promise.resolve({ volume: null, timedOut: false })),
  listen: vi.fn((eventName: string, handler: (event: { payload: unknown }) => void) => {
    listenHandlers.set(eventName, handler)
    return Promise.resolve(vi.fn())
  }),
  // The controller now uses the typed `onDragImageSize` / `onDragModifiers`
  // wrappers (bare payload). Route them into `listenHandlers` under their wire
  // names, re-wrapping into the `{ payload }` shape the test's emitter uses.
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
  // Drops from other apps (`promise-drop-controller.ts`), routed the same way.
  // `onDragPayloadKind` hands over the bare kind, like the real wrapper.
  onDragPayloadKind: vi.fn((handler: (kind: unknown) => void) => {
    listenHandlers.set('drag-payload-kind', (event) => {
      handler((event.payload as { kind: unknown }).kind)
    })
    return Promise.resolve(vi.fn())
  }),
  onPromiseDropStarted: vi.fn((handler: (payload: unknown) => void) => {
    listenHandlers.set('promise-drop-started', (event) => {
      handler(event.payload)
    })
    return Promise.resolve(vi.fn())
  }),
  onPromiseDropReady: vi.fn((handler: (payload: unknown) => void) => {
    listenHandlers.set('promise-drop-ready', (event) => {
      handler(event.payload)
    })
    return Promise.resolve(vi.fn())
  }),
  onPromiseDropFailed: vi.fn((handler: (payload: unknown) => void) => {
    listenHandlers.set('promise-drop-failed', (event) => {
      handler(event.payload)
    })
    return Promise.resolve(vi.fn())
  }),
  releasePromiseDrop: releasePromiseDropSpy,
}))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: addToastSpy, dismissToast: vi.fn() }))

// Nothing is on screen in this suite, so the gate always lets a drop through.
// Its verdict is driven in `drag-drop-controller.svelte.test.ts`.
vi.mock('./operation-start-gate', () => ({ operationStartIsBlocked: () => false }))

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
  clearSelfDragIdentity: vi.fn(),
  endSelfDragSession: vi.fn(() => Promise.resolve()),
}))

vi.mock('../drag/drag-overlay.svelte.js', () => ({
  showOverlay: showOverlaySpy,
  updateOverlay: updateOverlaySpy,
  hideOverlay: hideOverlaySpy,
}))

vi.mock('#lib/icon-cache.js', () => ({ getCachedIcon: getCachedIconSpy, getCachedCustomFolderIcon: () => undefined }))

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

describe('drag-drop-controller — native listeners', () => {
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
    storedSpellingsSpy.mockImplementation((_volumeId, paths) => Promise.resolve(paths))
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
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

    it('a drag carrying nothing droppable lights nothing up, and its drop is no transfer', async () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create()
      await controller.init()
      const fire = dragDropHandler()

      listenHandler('drag-payload-kind')({ payload: { kind: 'nothing' } })
      fire({ payload: { type: 'enter', paths: [], position: { x: 1, y: 1 } } })
      expect(controller.getDropTargetPane()).toBeNull()
      expect(lastOverlayArgs()[3]).toBe(false)

      fire({ payload: { type: 'drop', paths: [], position: { x: 1, y: 1 } } })
      await flushDrop()
      expect(showTransfer).not.toHaveBeenCalled()
      expect(trackEventSpy).toHaveBeenCalledWith('drop_received', expect.objectContaining({ outcome: 'nothingUsable' }))
    })

    it('a browser image drop opens the Copy dialog once its files land, labeled by where it came from', async () => {
      resolveDropTargetSpy.mockReturnValue(paneTarget('right'))
      const { controller, showTransfer } = create({ paths: { right: '/Users/x/Pictures' } })
      await controller.init()
      const fire = dragDropHandler()
      const received = '/Users/x/Library/Application Support/cmdr/drops/drop-1-0/cat.jpg'

      listenHandler('drag-payload-kind')({ payload: { kind: 'promises' } })
      fire({ payload: { type: 'enter', paths: [], position: { x: 1, y: 1 } } })
      expect(controller.getDropTargetPane()).toBe('right')

      // The backend's started event goes out before wry's drop.
      listenHandler('promise-drop-started')({
        payload: {
          dropId: 'drop-1-0',
          itemCount: 1,
          source: { appName: 'Google Chrome', pageUrl: 'https://www.example.com/cat.jpg' },
        },
      })
      fire({ payload: { type: 'drop', paths: [], position: { x: 1, y: 1 } } })
      await flushDrop()
      expect(showTransfer).not.toHaveBeenCalled()
      expect(trackEventSpy).toHaveBeenCalledWith('drop_received', expect.objectContaining({ outcome: 'promise' }))

      listenHandler('promise-drop-ready')({ payload: { dropId: 'drop-1-0', paths: [received], failedCount: 0 } })
      await flushDrop()
      expect(showTransfer).toHaveBeenCalledTimes(1)
      const props = showTransfer.mock.calls[0]?.[0]
      expect(props).toMatchObject({
        operationType: 'copy',
        sourcePaths: [received],
        destinationPath: '/Users/x/Pictures',
        sourceVolumeId: 'root',
        sourceDisplay: 'cat.jpg from Google Chrome (example.com)',
        promiseDropId: 'drop-1-0',
      })
    })

    it('a browser image dropped outside both panes lets its drop folder go', async () => {
      resolveDropTargetSpy.mockReturnValue(null)
      const { controller, showTransfer } = create()
      await controller.init()
      const fire = dragDropHandler()

      listenHandler('drag-payload-kind')({ payload: { kind: 'promises' } })
      listenHandler('promise-drop-started')({
        payload: { dropId: 'drop-2-0', itemCount: 1, source: { appName: null, pageUrl: null } },
      })
      fire({ payload: { type: 'drop', paths: [], position: { x: 1, y: 1 } } })
      listenHandler('promise-drop-ready')({ payload: { dropId: 'drop-2-0', paths: ['/d/cat.jpg'], failedCount: 0 } })
      await flushDrop()

      expect(showTransfer).not.toHaveBeenCalled()
      expect(releasePromiseDropSpy).toHaveBeenCalledWith('drop-2-0')
    })

    it('cleanup unsubscribes the listeners and stops modifier tracking', async () => {
      const { controller } = create()
      await controller.init()
      controller.cleanup()
      expect(stopModifierTrackingSpy).toHaveBeenCalled()
    })
  })
})
