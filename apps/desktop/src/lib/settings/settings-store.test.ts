import { describe, it, expect, vi, beforeEach } from 'vitest'
import { emit } from '@tauri-apps/api/event'
import { setSetting, onSettingChange } from './settings-store'

// `test-setup.ts` already mocks `@tauri-apps/api/event` globally. We don't
// reset the module here: settings-store's `settingsCache` is module-scoped,
// so each test picks a setting ID nobody else in this file uses to avoid
// leaking cache state between tests.
const mockedEmit = vi.mocked(emit)

const settingsChangedEmits = () => mockedEmit.mock.calls.filter((c) => c[0] === 'settings:changed')

describe('setSetting idempotency', () => {
  beforeEach(() => {
    mockedEmit.mockClear()
  })

  it('cascades on a real value change: emits + notifies listeners', () => {
    // First test in the file: `appearance.uiDensity` is still at default in
    // the cache, so this is a real change.
    const listener = vi.fn()
    const unsub = onSettingChange(listener)

    setSetting('appearance.uiDensity', 'spacious')

    expect(listener).toHaveBeenCalledTimes(1)
    expect(listener).toHaveBeenCalledWith('appearance.uiDensity', 'spacious')
    expect(settingsChangedEmits()).toHaveLength(1)
    expect(settingsChangedEmits()[0]?.[1]).toEqual({ id: 'appearance.uiDensity', value: 'spacious', explicit: true })

    unsub()
  })

  it('short-circuits when value is unchanged: no emit, no listener', () => {
    // Use a fresh setting ID so the cache for it starts undefined.
    const listener = vi.fn()
    const unsub = onSettingChange(listener)

    setSetting('network.enabled', false)
    expect(listener).toHaveBeenCalledTimes(1)
    listener.mockClear()
    mockedEmit.mockClear()

    // Same value: must be a complete no-op past validation.
    setSetting('network.enabled', false)

    expect(listener).not.toHaveBeenCalled()
    expect(settingsChangedEmits()).toHaveLength(0)

    unsub()
  })

  it('cascades again when the value flips after a no-op', () => {
    const listener = vi.fn()
    const unsub = onSettingChange(listener)

    // Note: `appearance.uiDensity` is already 'spacious' in the cache from
    // the first test in this file. Setting it again must short-circuit.
    setSetting('appearance.uiDensity', 'spacious')
    expect(listener).not.toHaveBeenCalled()

    // Real change: cascade fires.
    setSetting('appearance.uiDensity', 'compact')
    expect(listener).toHaveBeenCalledTimes(1)
    expect(listener).toHaveBeenCalledWith('appearance.uiDensity', 'compact')
    expect(settingsChangedEmits()).toHaveLength(1)

    unsub()
  })

  it('handles numbers the same way (=== covers all primitive setting types)', () => {
    const listener = vi.fn()
    const unsub = onSettingChange(listener)

    setSetting('advanced.maxLogStorageMb', 123)
    expect(listener).toHaveBeenCalledTimes(1)

    listener.mockClear()
    setSetting('advanced.maxLogStorageMb', 123)
    expect(listener).not.toHaveBeenCalled()

    setSetting('advanced.maxLogStorageMb', 456)
    expect(listener).toHaveBeenCalledTimes(1)
    expect(listener).toHaveBeenLastCalledWith('advanced.maxLogStorageMb', 456)

    unsub()
  })
})
