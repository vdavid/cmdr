/**
 * Tests for the app-mode helper. `_resetForTests` clears the module's cached
 * mode between cases so each test sees a fresh resolution. The backend
 * `getAutomatedRun` call is mocked at the `tauri-commands` barrel;
 * `import.meta.env.DEV` is whatever vitest reports (DEV=true in the dev test
 * runner), which the assertions account for.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { getAutomatedRunSpy, orderWindowToBackSpy, warnSpy } = vi.hoisted(() => ({
  getAutomatedRunSpy: vi.fn<() => Promise<'none' | 'e2e' | 'capture'>>(),
  orderWindowToBackSpy: vi.fn<(label: string) => Promise<void>>(),
  warnSpy: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  getAutomatedRun: getAutomatedRunSpy,
  orderWindowToBack: orderWindowToBackSpy,
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: warnSpy, error: vi.fn() }),
}))

import {
  initAppMode,
  getAppMode,
  isE2eRun,
  decorateChildWindowTitle,
  decorateMainWindowTitle,
  orderChildWindowToBackInE2e,
  _resetForTests,
} from './app-mode'

/** Minimal `WebviewWindow` stand-in: `once` fires the callback so the helper's
 *  `tauri://created` wait resolves immediately. */
function fakeWindow(label: string) {
  return {
    label,
    once: vi.fn((_event: string, cb: () => void) => {
      cb()
      return Promise.resolve(() => {})
    }),
  } as unknown as Parameters<typeof orderChildWindowToBackInE2e>[0]
}

describe('app-mode', () => {
  beforeEach(() => {
    _resetForTests()
    getAutomatedRunSpy.mockReset()
    orderWindowToBackSpy.mockReset()
    orderWindowToBackSpy.mockResolvedValue(undefined)
    warnSpy.mockReset()
  })

  it('resolves to e2e when backend reports an E2E run', async () => {
    getAutomatedRunSpy.mockResolvedValue('e2e')
    expect(await initAppMode()).toBe('e2e')
    expect(getAppMode()).toBe('e2e')
    expect(decorateChildWindowTitle('Settings')).toBe('E2E - Settings - E2E')
  })

  it('falls back to dev (vitest DEV=true) when backend reports no automated run', async () => {
    getAutomatedRunSpy.mockResolvedValue('none')
    expect(await initAppMode()).toBe('dev')
    expect(getAppMode()).toBe('dev')
    // Dev mode leaves child titles untouched — only E2E decorates.
    expect(decorateChildWindowTitle('Viewer')).toBe('Viewer')
  })

  it('initAppMode is idempotent', async () => {
    getAutomatedRunSpy.mockResolvedValue('e2e')
    await initAppMode()
    await initAppMode()
    expect(getAutomatedRunSpy).toHaveBeenCalledTimes(1)
  })

  it('shares one backend round trip between callers that start before it resolves', async () => {
    // The root layout and a page's own `onMount` both call it while the first answer is in flight.
    getAutomatedRunSpy.mockResolvedValue('capture')
    const [first, second] = await Promise.all([initAppMode(), initAppMode()])
    expect(first).toBe('capture')
    expect(second).toBe('capture')
    expect(getAutomatedRunSpy).toHaveBeenCalledTimes(1)
  })

  it('getAppMode pre-init falls back to dev/prod, never a run marker', () => {
    // Before initAppMode runs, vitest's DEV=true → dev. The unit-test build bakes the E2E
    // build define in, and that must not make a window look like a run before the backend says so.
    expect(getAppMode()).toBe('dev')
    expect(isE2eRun()).toBe(false)
  })

  describe('capture mode', () => {
    it('resolves to capture when the backend reports a capture run', async () => {
      getAutomatedRunSpy.mockResolvedValue('capture')
      expect(await initAppMode()).toBe('capture')
      expect(getAppMode()).toBe('capture')
    })

    it('counts as an E2E run, so harness-only behavior stays on', async () => {
      getAutomatedRunSpy.mockResolvedValue('capture')
      await initAppMode()
      expect(isE2eRun()).toBe(true)

      _resetForTests()
      getAutomatedRunSpy.mockResolvedValue('e2e')
      await initAppMode()
      expect(isE2eRun()).toBe(true)

      _resetForTests()
      getAutomatedRunSpy.mockResolvedValue('none')
      await initAppMode()
      expect(isE2eRun()).toBe(false)
    })

    it('still keeps child windows out of the way, like any run', async () => {
      getAutomatedRunSpy.mockResolvedValue('capture')
      await initAppMode()
      const win = fakeWindow('settings')
      await orderChildWindowToBackInE2e(win)
      expect(orderWindowToBackSpy).toHaveBeenCalledWith('settings')
    })

    it('marks child window titles with SCREENSHOT', async () => {
      getAutomatedRunSpy.mockResolvedValue('capture')
      await initAppMode()
      expect(decorateChildWindowTitle('Settings')).toBe('SCREENSHOT - Settings - SCREENSHOT')
    })
  })

  describe('orderChildWindowToBackInE2e', () => {
    it('orders the window back once created when in E2E', async () => {
      getAutomatedRunSpy.mockResolvedValue('e2e')
      await initAppMode()
      const win = fakeWindow('viewer-123')
      await orderChildWindowToBackInE2e(win)
      // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest mock, no `this` binding
      expect(win.once).toHaveBeenCalledWith('tauri://created', expect.any(Function))
      expect(orderWindowToBackSpy).toHaveBeenCalledWith('viewer-123')
    })

    it('is a no-op outside E2E', async () => {
      getAutomatedRunSpy.mockResolvedValue('none')
      await initAppMode()
      const win = fakeWindow('settings')
      await orderChildWindowToBackInE2e(win)
      // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest mock, no `this` binding
      expect(win.once).not.toHaveBeenCalled()
      expect(orderWindowToBackSpy).not.toHaveBeenCalled()
    })

    it('swallows and logs errors so callers can fire-and-forget', async () => {
      getAutomatedRunSpy.mockResolvedValue('e2e')
      await initAppMode()
      orderWindowToBackSpy.mockRejectedValue(new Error('no window'))
      const win = fakeWindow('shortcuts')
      await expect(orderChildWindowToBackInE2e(win)).resolves.toBeUndefined()
      expect(warnSpy).toHaveBeenCalled()
    })
  })

  describe('decorateMainWindowTitle', () => {
    it('leaves the title untouched in prod', () => {
      expect(decorateMainWindowTitle('Cmdr', 'prod', '')).toBe('Cmdr')
      // A stray label never leaks into a prod title.
      expect(decorateMainWindowTitle('Cmdr', 'prod', 'colorful-tags')).toBe('Cmdr')
    })

    it('wraps the worktree label around the dev marker', () => {
      expect(decorateMainWindowTitle('Cmdr', 'dev', 'colorful-tags')).toBe(
        '(colorful-tags) DEV MODE - Cmdr - DEV MODE (colorful-tags)',
      )
      expect(decorateMainWindowTitle('Cmdr – Personal use only', 'dev', 'main')).toBe(
        '(main) DEV MODE - Cmdr – Personal use only - DEV MODE (main)',
      )
    })

    it('omits the label parens in dev when no label is set', () => {
      expect(decorateMainWindowTitle('Cmdr', 'dev', '')).toBe('DEV MODE - Cmdr - DEV MODE')
    })

    it('marks E2E without a label (E2E sessions carry none)', () => {
      expect(decorateMainWindowTitle('Cmdr', 'e2e', '')).toBe('E2E MODE - Cmdr - E2E MODE')
    })

    it('marks a capture run SCREENSHOT, the text baked into every translator image', () => {
      expect(decorateMainWindowTitle('Cmdr – Personal use only', 'capture', '')).toBe(
        'SCREENSHOT - Cmdr – Personal use only - SCREENSHOT',
      )
    })

    it('keeps the worktree label wrapping in a capture run started from a worktree', () => {
      expect(decorateMainWindowTitle('Cmdr', 'capture', 'i18n-blank-shots')).toBe(
        '(i18n-blank-shots) SCREENSHOT - Cmdr - SCREENSHOT (i18n-blank-shots)',
      )
    })
  })
})
