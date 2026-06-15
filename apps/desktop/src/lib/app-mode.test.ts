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
  decorateChildWindowTitle,
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

  describe('orderChildWindowToBackInE2e', () => {
    it('orders the window back once created when in E2E', async () => {
      isE2eModeSpy.mockResolvedValue(true)
      await initAppMode()
      const win = fakeWindow('viewer-123')
      await orderChildWindowToBackInE2e(win)
      expect(win.once).toHaveBeenCalledWith('tauri://created', expect.any(Function))
      expect(orderWindowToBackSpy).toHaveBeenCalledWith('viewer-123')
    })

    it('is a no-op outside E2E', async () => {
      isE2eModeSpy.mockResolvedValue(false)
      await initAppMode()
      const win = fakeWindow('settings')
      await orderChildWindowToBackInE2e(win)
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
})
