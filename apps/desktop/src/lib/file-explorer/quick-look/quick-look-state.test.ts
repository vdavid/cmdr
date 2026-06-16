import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

type EventHandler = (e: { payload: unknown }) => void
type HandlerMap = Record<string, EventHandler>

// `vi.mock` is hoisted to the top of the file. Anything the mock factory
// references must come from `vi.hoisted(...)`, not a top-level `const`.
// The factory's return type is annotated explicitly because ESLint's
// `no-unnecessary-type-assertion` would otherwise strip an inline
// `as HandlerMap` cast (it doesn't see the wider scope where the cast
// matters for the destructured `handlers`).
const { handlers, unlistenFns, quickLookCloseMock } = vi.hoisted(
  (): {
    handlers: HandlerMap
    unlistenFns: Array<ReturnType<typeof vi.fn>>
    quickLookCloseMock: ReturnType<typeof vi.fn>
  } => ({
    handlers: {},
    unlistenFns: [],
    quickLookCloseMock: vi.fn(() => Promise.resolve()),
  }),
)

vi.mock('@tauri-apps/api/event', () => ({
  // `listen` returns `Promise<UnlistenFn>` so callers can `await` it; the
  // explicit `Promise.resolve` keeps the return type while satisfying
  // `@typescript-eslint/require-await` (we don't `await` anything inside).
  listen: vi.fn((event: string, handler: (e: { payload: unknown }) => void) => {
    handlers[event] = handler
    const fn = vi.fn(() => {})
    unlistenFns.push(fn)
    return Promise.resolve(fn)
  }),
}))

vi.mock('$lib/tauri-commands', () => ({
  quickLookClose: quickLookCloseMock,
  // The typed `onQuickLook*` wrappers hand a bare payload; route them into the
  // `handlers` map under their wire names, re-wrapping into the `{ payload }`
  // shape the tests' emitter uses.
  onQuickLookClosed: (handler: () => void) => {
    handlers['quick-look-closed'] = () => {
      handler()
    }
    const fn = vi.fn(() => {})
    unlistenFns.push(fn)
    return Promise.resolve(fn)
  },
  onQuickLookKey: (handler: (payload: unknown) => void) => {
    handlers['quick-look-key'] = (event: { payload: unknown }) => {
      handler(event.payload)
    }
    const fn = vi.fn(() => {})
    unlistenFns.push(fn)
    return Promise.resolve(fn)
  },
}))

import {
  quickLookState,
  quickLookDispatchGuardJustFired,
  armQuickLookDispatchGuard,
  initQuickLookListeners,
  closeFromPaneError,
} from './quick-look-state.svelte'

describe('quickLookState', () => {
  let teardown: (() => void) | undefined

  beforeEach(() => {
    quickLookState.isOpen = false
    quickLookCloseMock.mockClear()
    for (const k of Object.keys(handlers)) {
      // Reset the open-shape mock map of module-singleton handlers between tests.
      delete handlers[k]
    }
    unlistenFns.length = 0
  })
  afterEach(() => {
    // Reset the module's `attached` singleton flag so the next test can attach
    // fresh listeners. The cleanup the production module returns flips it.
    teardown?.()
    teardown = undefined
    vi.useRealTimers()
  })

  it('starts closed', () => {
    expect(quickLookState.isOpen).toBe(false)
  })

  it('quickLookDispatchGuardJustFired defaults to false before any dispatch fires', () => {
    expect(quickLookDispatchGuardJustFired()).toBe(false)
  })

  it('armQuickLookDispatchGuard arms the guard so subsequent fires are swallowed', () => {
    // Defends against the AppKit menu accelerator + JS shortcut double-dispatch:
    // the dispatcher arms on entry, so the second fire of the same keystroke
    // reads `quickLookDispatchGuardJustFired() === true` and returns early.
    const nowSpy = vi.spyOn(performance, 'now')
    nowSpy.mockReturnValue(5_000)
    expect(quickLookDispatchGuardJustFired()).toBe(false)
    armQuickLookDispatchGuard()
    expect(quickLookDispatchGuardJustFired()).toBe(true)
    // Still armed 100 ms later.
    nowSpy.mockReturnValue(5_100)
    expect(quickLookDispatchGuardJustFired()).toBe(true)
    // Expired 250 ms later (> 200 ms grace).
    nowSpy.mockReturnValue(5_250)
    expect(quickLookDispatchGuardJustFired()).toBe(false)
    nowSpy.mockRestore()
  })

  it('initQuickLookListeners attaches both event listeners and is idempotent', async () => {
    teardown = await initQuickLookListeners(() => undefined)
    expect(typeof teardown).toBe('function')
    expect(typeof handlers['quick-look-closed']).toBe('function')
    expect(typeof handlers['quick-look-key']).toBe('function')

    // Second call short-circuits and returns the no-op (the listener-set is
    // module-singleton; double-attach during HMR would otherwise double-fire
    // every event).
    const cleanupB = await initQuickLookListeners(() => undefined)
    expect(typeof cleanupB).toBe('function')
    // Still only the original two unlisten functions registered.
    expect(unlistenFns).toHaveLength(2)
  })

  it('quick-look-closed event flips isOpen to false', async () => {
    teardown = await initQuickLookListeners(() => undefined)
    quickLookState.isOpen = true
    handlers['quick-look-closed']({ payload: null })
    expect(quickLookState.isOpen).toBe(false)
  })

  it('Shift+Space from panel closes via IPC and arms the dispatch guard', async () => {
    teardown = await initQuickLookListeners(() => undefined)
    quickLookState.isOpen = true
    handlers['quick-look-key']({
      payload: { key: ' ', code: 'Space', shiftKey: true, metaKey: false, altKey: false, ctrlKey: false },
    })
    expect(quickLookState.isOpen).toBe(false)
    expect(quickLookCloseMock).toHaveBeenCalledTimes(1)
    expect(quickLookDispatchGuardJustFired()).toBe(true)
  })

  it('non-shift-space key events route through the explorer', async () => {
    const routePanelKey = vi.fn()
    const fakeExplorer = { routePanelKey } as unknown as NonNullable<
      ReturnType<Parameters<typeof initQuickLookListeners>[0]>
    >
    teardown = await initQuickLookListeners(() => fakeExplorer)
    const payload = {
      key: 'ArrowDown',
      code: 'ArrowDown',
      shiftKey: false,
      metaKey: false,
      altKey: false,
      ctrlKey: false,
    }
    handlers['quick-look-key']({ payload })
    expect(routePanelKey).toHaveBeenCalledWith(payload)
    expect(quickLookCloseMock).not.toHaveBeenCalled()
  })

  it('closeFromPaneError flips isOpen and calls the close IPC', () => {
    quickLookState.isOpen = true
    closeFromPaneError()
    expect(quickLookState.isOpen).toBe(false)
    expect(quickLookCloseMock).toHaveBeenCalledTimes(1)
  })

  it('closeFromPaneError is idempotent when already closed', () => {
    // No prior open: must not call the IPC.
    expect(quickLookState.isOpen).toBe(false)
    closeFromPaneError()
    expect(quickLookState.isOpen).toBe(false)
    expect(quickLookCloseMock).not.toHaveBeenCalled()

    // Calling twice in a row also stays a no-op after the first call closes.
    quickLookState.isOpen = true
    closeFromPaneError()
    closeFromPaneError()
    expect(quickLookState.isOpen).toBe(false)
    expect(quickLookCloseMock).toHaveBeenCalledTimes(1)
  })

  it('teardown detaches both listeners and allows fresh attachment afterwards', async () => {
    teardown = await initQuickLookListeners(() => undefined)
    expect(unlistenFns).toHaveLength(2)
    expect(unlistenFns[0]).not.toHaveBeenCalled()
    expect(unlistenFns[1]).not.toHaveBeenCalled()

    // Calling teardown invokes both unlisten functions.
    teardown()
    teardown = undefined
    expect(unlistenFns[0]).toHaveBeenCalledTimes(1)
    expect(unlistenFns[1]).toHaveBeenCalledTimes(1)

    // After teardown the module is detachable again — a fresh init attaches
    // new listeners rather than short-circuiting on the singleton guard.
    const before = unlistenFns.length
    teardown = await initQuickLookListeners(() => undefined)
    expect(unlistenFns.length).toBe(before + 2)
  })

  it('dispatch guard window expires after QUICK_LOOK_DISPATCH_GRACE_MS', async () => {
    // The guard reads `performance.now()` directly, not `Date.now()`. Stub it
    // so we can travel deterministically across the 200 ms window without
    // relying on real wall-clock timing in tests.
    const nowSpy = vi.spyOn(performance, 'now')
    nowSpy.mockReturnValue(1_000)
    teardown = await initQuickLookListeners(() => undefined)
    quickLookState.isOpen = true
    // Drive the same Shift+Space close path that arms the grace window.
    handlers['quick-look-key']({
      payload: { key: ' ', code: 'Space', shiftKey: true, metaKey: false, altKey: false, ctrlKey: false },
    })
    expect(quickLookDispatchGuardJustFired()).toBe(true)
    // 250 ms later (> 200 ms grace window) the guard reports false.
    nowSpy.mockReturnValue(1_000 + 250)
    expect(quickLookDispatchGuardJustFired()).toBe(false)
    nowSpy.mockRestore()
  })
})
