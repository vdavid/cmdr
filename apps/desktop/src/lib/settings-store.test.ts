/**
 * Tests for the legacy settings store's persist contract.
 *
 * `saveSettings` must return whether the write reached disk (so FDA / onboarding
 * callers can react) and log a failed write instead of swallowing it silently.
 * The FDA and onboarding flows still go through this legacy file.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { load } from '@tauri-apps/plugin-store'
import { saveSettings } from './settings-store'

// `vi.hoisted` so the spy exists when the hoisted `vi.mock` factory runs.
const errorSpy = vi.hoisted(() => vi.fn())

// One store instance backs the whole module (getStore caches it). We reconfigure
// `set` / `save` per test rather than swapping the instance.
const set = vi.fn().mockResolvedValue(undefined)
const save = vi.fn().mockResolvedValue(undefined)

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(),
}))

// `getStore` resolves the store path via this; return the bare name (prod path).
vi.mock('./settings/store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

vi.mock('./logging/logger', () => ({
  getAppLogger: () => ({
    error: errorSpy,
    warn: vi.fn(),
    info: vi.fn(),
    debug: vi.fn(),
  }),
}))

describe('saveSettings', () => {
  beforeEach(() => {
    set.mockClear().mockResolvedValue(undefined)
    save.mockClear().mockResolvedValue(undefined)
    errorSpy.mockClear()
    vi.mocked(load).mockResolvedValue({ set, save } as unknown as Awaited<ReturnType<typeof load>>)
  })

  it('returns true and writes the given keys when the store save succeeds', async () => {
    const ok = await saveSettings({ fullDiskAccessChoice: 'allow' })

    expect(ok).toBe(true)
    expect(set).toHaveBeenCalledWith('fullDiskAccessChoice', 'allow')
    expect(save).toHaveBeenCalledOnce()
    expect(errorSpy).not.toHaveBeenCalled()
  })

  it('returns false and logs (does not throw) when the store save fails', async () => {
    save.mockRejectedValueOnce(new Error('disk full'))

    const ok = await saveSettings({ isOnboarded: true })

    expect(ok).toBe(false)
    // The failure is logged with the attempted keys, not swallowed.
    expect(errorSpy).toHaveBeenCalledOnce()
    const [, context] = errorSpy.mock.calls[0]
    expect(context.keys).toEqual(['isOnboarded'])
  })
})
