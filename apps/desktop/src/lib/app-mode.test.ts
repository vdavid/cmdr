/**
 * Tests for the app-mode helper. `_resetForTests` clears the module's cached
 * mode between cases so each test sees a fresh resolution. The backend
 * `isE2eMode` call is mocked at the `tauri-commands` barrel; `import.meta.env.DEV`
 * is whatever vitest reports (DEV=true in the dev test runner), which the
 * assertions account for.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { isE2eModeSpy } = vi.hoisted(() => ({
  isE2eModeSpy: vi.fn<() => Promise<boolean>>(),
}))

vi.mock('$lib/tauri-commands', () => ({
  isE2eMode: isE2eModeSpy,
}))

import { initAppMode, getAppMode, decorateChildWindowTitle, _resetForTests } from './app-mode'

describe('app-mode', () => {
  beforeEach(() => {
    _resetForTests()
    isE2eModeSpy.mockReset()
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
})
