/**
 * Tests for the reactive part of `volume-tint.svelte.ts`:
 * `initVolumeTints`, `cleanupVolumeTints`, and `getPaneTintBg`.
 *
 * The pure `volumeKindFor` classifier has its own test file at
 * `volume-tint.test.ts`.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

const subscribers = new Map<string, (id: string, value: unknown) => void>()
const settings = new Map<string, string>()

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((id: string) => settings.get(id) ?? 'none'),
  setSetting: vi.fn((id: string, value: string) => {
    settings.set(id, value)
    const cb = subscribers.get(id)
    cb?.(id, value)
    return Promise.resolve()
  }),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn((id: string, cb: (id: string, value: unknown) => void) => {
    subscribers.set(id, cb)
    return () => subscribers.delete(id)
  }),
  onSettingChange: vi.fn(() => () => {}),
}))

import { initVolumeTints, cleanupVolumeTints, getPaneTintBg } from './volume-tint.svelte'

beforeEach(() => {
  settings.clear()
  subscribers.clear()
})

afterEach(() => {
  cleanupVolumeTints()
})

describe('getPaneTintBg with no settings configured', () => {
  it('returns null for every kind when all tints are "none"', () => {
    initVolumeTints()
    expect(getPaneTintBg('root', 'apfs', 'main_volume')).toBeNull()
    expect(getPaneTintBg('volumesnaspi', 'smbfs', 'network')).toBeNull()
    expect(getPaneTintBg('mtp-1:1', undefined, 'mobile_device')).toBeNull()
  })

  it('returns null for the "other" kind even with all tints set', () => {
    settings.set('appearance.tintLocal', 'red')
    settings.set('appearance.tintSmb', 'blue')
    settings.set('appearance.tintMtp', 'green')
    initVolumeTints()
    expect(getPaneTintBg('network', undefined, 'favorite')).toBeNull()
  })
})

describe('getPaneTintBg with configured tints', () => {
  it('returns the color-mix expression for a local volume', () => {
    settings.set('appearance.tintLocal', 'blue')
    initVolumeTints()
    const bg = getPaneTintBg('root', 'apfs', 'main_volume')
    expect(bg).toContain('color-mix')
    expect(bg).toContain('var(--color-tint-blue)')
    expect(bg).toContain('var(--pane-tint-bg-pct')
    expect(bg).toContain('var(--pane-tint-fg-pct')
  })

  it('returns the right tint per kind', () => {
    settings.set('appearance.tintLocal', 'red')
    settings.set('appearance.tintSmb', 'green')
    settings.set('appearance.tintMtp', 'purple')
    initVolumeTints()
    expect(getPaneTintBg('root', 'apfs', 'main_volume')).toContain('var(--color-tint-red)')
    expect(getPaneTintBg('volumesnaspi', 'smbfs', 'network')).toContain('var(--color-tint-green)')
    expect(getPaneTintBg('mtp-1:1', undefined, 'mobile_device')).toContain('var(--color-tint-purple)')
  })
})

describe('reactivity', () => {
  it('updates the result when a setting changes after init', async () => {
    initVolumeTints()
    expect(getPaneTintBg('root', 'apfs', 'main_volume')).toBeNull()

    // Simulate cross-window setting update
    const { setSetting } = await import('$lib/settings/settings-store')
    await setSetting('appearance.tintLocal', 'amber')

    const bg = getPaneTintBg('root', 'apfs', 'main_volume')
    expect(bg).toContain('var(--color-tint-amber)')
  })

  it('initVolumeTints is idempotent', () => {
    initVolumeTints()
    initVolumeTints()
    initVolumeTints()
    // Should not double-subscribe; behavior is correct
    expect(getPaneTintBg('root', 'apfs', 'main_volume')).toBeNull()
  })

  it('cleanupVolumeTints lets a re-init read fresh values', async () => {
    settings.set('appearance.tintLocal', 'pink')
    initVolumeTints()
    expect(getPaneTintBg('root', 'apfs', 'main_volume')).toContain('var(--color-tint-pink)')
    cleanupVolumeTints()
    settings.set('appearance.tintLocal', 'teal')
    initVolumeTints()
    expect(getPaneTintBg('root', 'apfs', 'main_volume')).toContain('var(--color-tint-teal)')
  })
})
