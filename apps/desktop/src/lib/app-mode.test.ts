/**
 * Tests for the app-mode helper. `_resetForTests` clears the module's cached
 * mode between cases so each test sees a fresh resolution. The backend
 * `isE2eMode` call is mocked at the `tauri-commands` barrel; `import.meta.env.DEV`
 * is whatever vitest reports (DEV=true in the dev test runner), which the
 * assertions account for.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { isE2eModeSpy, orderWindowToBackSpy, warnSpy } = vi.hoisted(() => ({
  isE2eModeSpy: vi.fn<() => Promise<boolean>>(),
  orderWindowToBackSpy: vi.fn<(label: string) => Promise<void>>(),
  warnSpy: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  isE2eMode: isE2eModeSpy,
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
  _setCaptureBuildForTests,
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
    isE2eModeSpy.mockReset()
    orderWindowToBackSpy.mockReset()
    orderWindowToBackSpy.mockResolvedValue(undefined)
    warnSpy.mockReset()
  })

  it('resolves to e2e when backend reports E2E', async () => {
    isE2eModeSpy.mockResolvedValue(true)
    expect(await initAppMode()).toBe('e2e')
    expect(getAppMode()).toBe('e2e')
    expect(decorateChildWindowTitle('Settings')).toBe('E2E - Settings - E2E')
  })

  it('falls back to dev (vitest DEV=true) when backend says no', async () => {
    isE2eModeSpy.mockResolvedValue(false)
    expect(await initAppMode()).toBe('dev')
    expect(getAppMode()).toBe('dev')
    // Dev mode leaves child titles untouched — only E2E decorates.
    expect(decorateChildWindowTitle('Viewer')).toBe('Viewer')
  })

  it('initAppMode is idempotent', async () => {
    isE2eModeSpy.mockResolvedValue(true)
    await initAppMode()
    await initAppMode()
    expect(isE2eModeSpy).toHaveBeenCalledTimes(1)
  })

  it('getAppMode pre-init falls back to dev/prod from import.meta.env.DEV', () => {
    // Before initAppMode runs, vitest's DEV=true → dev. Either way, never e2e.
    expect(getAppMode()).not.toBe('e2e')
  })

  describe('capture mode', () => {
    it('resolves to capture from the build define, without asking the backend', async () => {
      _setCaptureBuildForTests(true)
      expect(await initAppMode()).toBe('capture')
      expect(getAppMode()).toBe('capture')
      // The define is synchronous and decisive, so there's no `isE2eMode()` round trip.
      expect(isE2eModeSpy).not.toHaveBeenCalled()
    })

    it('wins over e2e, which in turn wins over dev', async () => {
      // A capture run IS an E2E run (backend reports E2E too); the most specific marker wins.
      _setCaptureBuildForTests(true)
      isE2eModeSpy.mockResolvedValue(true)
      expect(await initAppMode()).toBe('capture')

      _resetForTests()
      isE2eModeSpy.mockResolvedValue(true)
      expect(await initAppMode()).toBe('e2e')

      _resetForTests()
      isE2eModeSpy.mockResolvedValue(false)
      expect(await initAppMode()).toBe('dev')
    })

    it('reports capture pre-init too, so the first frame is already yellow', () => {
      _setCaptureBuildForTests(true)
      expect(getAppMode()).toBe('capture')
    })

    it('counts as an E2E run, so harness-only behavior stays on', async () => {
      _setCaptureBuildForTests(true)
      await initAppMode()
      expect(isE2eRun()).toBe(true)

      _resetForTests()
      isE2eModeSpy.mockResolvedValue(true)
      await initAppMode()
      expect(isE2eRun()).toBe(true)

      _resetForTests()
      isE2eModeSpy.mockResolvedValue(false)
      await initAppMode()
      expect(isE2eRun()).toBe(false)
    })

    it('still keeps child windows out of the way, like any run', async () => {
      _setCaptureBuildForTests(true)
      await initAppMode()
      const win = fakeWindow('settings')
      await orderChildWindowToBackInE2e(win)
      expect(orderWindowToBackSpy).toHaveBeenCalledWith('settings')
    })

    it('marks child window titles with SCREENSHOT', async () => {
      _setCaptureBuildForTests(true)
      await initAppMode()
      expect(decorateChildWindowTitle('Settings')).toBe('SCREENSHOT - Settings - SCREENSHOT')
    })
  })

  describe('orderChildWindowToBackInE2e', () => {
    it('orders the window back once created when in E2E', async () => {
      isE2eModeSpy.mockResolvedValue(true)
      await initAppMode()
      const win = fakeWindow('viewer-123')
      await orderChildWindowToBackInE2e(win)
      // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest mock, no `this` binding
      expect(win.once).toHaveBeenCalledWith('tauri://created', expect.any(Function))
      expect(orderWindowToBackSpy).toHaveBeenCalledWith('viewer-123')
    })

    it('is a no-op outside E2E', async () => {
      isE2eModeSpy.mockResolvedValue(false)
      await initAppMode()
      const win = fakeWindow('settings')
      await orderChildWindowToBackInE2e(win)
      // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest mock, no `this` binding
      expect(win.once).not.toHaveBeenCalled()
      expect(orderWindowToBackSpy).not.toHaveBeenCalled()
    })

    it('swallows and logs errors so callers can fire-and-forget', async () => {
      isE2eModeSpy.mockResolvedValue(true)
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
