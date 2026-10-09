import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

type EventHandler = (e: { payload: unknown }) => void
type HandlerMap = Record<string, EventHandler>

// `vi.mock` is hoisted to the top of the file. Anything the mock factory
// references must come from `vi.hoisted(...)`, not a top-level `const`.
// The factory's return type is annotated explicitly because ESLint's
// `no-unnecessary-type-assertion` would otherwise strip an inline
// `as HandlerMap` cast (it doesn't see the wider scope where the cast
// matters for the destructured `handlers`).
// The close gesture resolves `file.quickLook` through the command registry, and that
// command's `⇧Space` default is gated on `isMacOS()` AT MODULE LOAD (Quick Look is a
// macOS-only feature, so the registry declares no shortcut elsewhere). happy-dom
// reports a Linux UA, so pin macOS from a `vi.hoisted` block: a `beforeAll` spy would
// run after `command-registry` has already read the user agent.
vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })
})

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
  initQuickLookListeners,
  closeFromPaneError,
  closeFromMainWindowKey,
  shouldCloseFromMainWindowKey,
} from './quick-look-state.svelte'

describe('quickLookState', () => {
  const noDispatch = () => Promise.resolve()
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

  it('initQuickLookListeners attaches both event listeners and is idempotent', async () => {
    teardown = await initQuickLookListeners(() => undefined, noDispatch)
    expect(typeof teardown).toBe('function')
    expect(typeof handlers['quick-look-closed']).toBe('function')
    expect(typeof handlers['quick-look-key']).toBe('function')

    // Second call short-circuits and returns the no-op (the listener-set is
    // module-singleton; double-attach during HMR would otherwise double-fire
    // every event).
    const cleanupB = await initQuickLookListeners(() => undefined, noDispatch)
    expect(typeof cleanupB).toBe('function')
    // Still only the original two unlisten functions registered.
    expect(unlistenFns).toHaveLength(2)
  })

  it('quick-look-closed event flips isOpen to false', async () => {
    teardown = await initQuickLookListeners(() => undefined, noDispatch)
    quickLookState.isOpen = true
    handlers['quick-look-closed']({ payload: null })
    expect(quickLookState.isOpen).toBe(false)
  })

  it('⇧Space from the panel dispatches file.quickLook down the keyboard road', async () => {
    // The toggle goes through the dispatch core like any keypress, so the central
    // keyboard+menu dedup drops the File menu's late duplicate of the same press
    // instead of letting it reopen the panel (cmdr-reports#32).
    const dispatchKeyboard = vi.fn(() => Promise.resolve())
    teardown = await initQuickLookListeners(() => undefined, dispatchKeyboard)
    quickLookState.isOpen = true
    handlers['quick-look-key']({
      payload: { key: ' ', code: 'Space', shiftKey: true, metaKey: false, altKey: false, ctrlKey: false },
    })
    expect(dispatchKeyboard).toHaveBeenCalledExactlyOnceWith('file.quickLook')
    expect(quickLookCloseMock).not.toHaveBeenCalled()
  })

  it('⌥⇧Space routes through the explorer instead of closing (modifier superset)', async () => {
    const routePanelKey = vi.fn()
    const fakeExplorer = { routePanelKey } as unknown as NonNullable<
      ReturnType<Parameters<typeof initQuickLookListeners>[0]>
    >
    teardown = await initQuickLookListeners(() => fakeExplorer, noDispatch)
    quickLookState.isOpen = true
    // `⇧Space` is `file.quickLook`; `⌥⇧Space` is a different combo entirely. The old
    // `payload.shiftKey && payload.key === ' '` test matched both, so any ⇧Space
    // superset dismissed the panel on its way elsewhere.
    handlers['quick-look-key']({
      payload: { key: ' ', code: 'Space', shiftKey: true, metaKey: false, altKey: true, ctrlKey: false },
    })
    expect(quickLookCloseMock).not.toHaveBeenCalled()
    expect(quickLookState.isOpen).toBe(true)
  })

  it('plain Space from the panel closes it and never reaches the selection toggle', async () => {
    // Space closes Quick Look like it does in Finder (cmdr-reports#32): the press that
    // dismisses the preview must not also select the file. The native monitor consumes
    // it first; this is the path for a Space the panel forwards anyway.
    const routePanelKey = vi.fn()
    const fakeExplorer = { routePanelKey } as unknown as NonNullable<
      ReturnType<Parameters<typeof initQuickLookListeners>[0]>
    >
    teardown = await initQuickLookListeners(() => fakeExplorer, noDispatch)
    quickLookState.isOpen = true
    handlers['quick-look-key']({
      payload: { key: ' ', code: 'Space', shiftKey: false, metaKey: false, altKey: false, ctrlKey: false },
    })
    expect(routePanelKey).not.toHaveBeenCalled()
    expect(quickLookCloseMock).toHaveBeenCalledTimes(1)
    expect(quickLookState.isOpen).toBe(false)
  })

  it('non-shift-space key events route through the explorer', async () => {
    const routePanelKey = vi.fn()
    const fakeExplorer = { routePanelKey } as unknown as NonNullable<
      ReturnType<Parameters<typeof initQuickLookListeners>[0]>
    >
    teardown = await initQuickLookListeners(() => fakeExplorer, noDispatch)
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

  it('Escape during the opening handoff closes once and clears optimistic state', () => {
    quickLookState.isOpen = true
    expect(closeFromMainWindowKey()).toBe(true)
    expect(quickLookState.isOpen).toBe(false)
    expect(closeFromMainWindowKey()).toBe(false)
    expect(quickLookCloseMock).toHaveBeenCalledTimes(1)
  })

  it('only takes plain Escape when the main window has no foreground dialog', () => {
    quickLookState.isOpen = true
    const noDialogs = { dialogOpen: false, paletteOpen: false }
    expect(shouldCloseFromMainWindowKey(new KeyboardEvent('keydown', { key: 'Escape' }), noDialogs)).toBe(true)
    expect(
      shouldCloseFromMainWindowKey(new KeyboardEvent('keydown', { key: 'Escape', metaKey: true }), noDialogs),
    ).toBe(false)
    expect(
      shouldCloseFromMainWindowKey(new KeyboardEvent('keydown', { key: 'Escape' }), {
        dialogOpen: true,
        paletteOpen: false,
      }),
    ).toBe(false)
    quickLookState.isOpen = false
    expect(shouldCloseFromMainWindowKey(new KeyboardEvent('keydown', { key: 'Escape' }), noDialogs)).toBe(false)
  })

  it('takes plain Space in the main window while open, so it closes instead of selecting', () => {
    quickLookState.isOpen = true
    const noDialogs = { dialogOpen: false, paletteOpen: false }
    const space = (init: KeyboardEventInit = {}) => new KeyboardEvent('keydown', { key: ' ', code: 'Space', ...init })
    expect(shouldCloseFromMainWindowKey(space(), noDialogs)).toBe(true)
    // ⇧Space is the toggle command itself, which closes through the dispatcher.
    expect(shouldCloseFromMainWindowKey(space({ shiftKey: true }), noDialogs)).toBe(false)
    expect(shouldCloseFromMainWindowKey(space({ metaKey: true }), noDialogs)).toBe(false)
    expect(shouldCloseFromMainWindowKey(space(), { dialogOpen: false, paletteOpen: true })).toBe(false)
    quickLookState.isOpen = false
    expect(shouldCloseFromMainWindowKey(space(), noDialogs)).toBe(false)
  })

  it('leaves a Space typed into a text field alone', () => {
    quickLookState.isOpen = true
    const input = document.createElement('input')
    document.body.append(input)
    input.focus()
    const verdict = shouldCloseFromMainWindowKey(new KeyboardEvent('keydown', { key: ' ', code: 'Space' }), {
      dialogOpen: false,
      paletteOpen: false,
    })
    input.remove()
    expect(verdict).toBe(false)
  })

  it('teardown detaches both listeners and allows fresh attachment afterwards', async () => {
    teardown = await initQuickLookListeners(() => undefined, noDispatch)
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
    teardown = await initQuickLookListeners(() => undefined, noDispatch)
    expect(unlistenFns.length).toBe(before + 2)
  })
})
