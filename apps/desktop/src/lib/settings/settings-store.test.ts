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
    expect(listener).toHaveBeenCalledWith({ id: 'appearance.uiDensity', value: 'spacious' })
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
    expect(listener).toHaveBeenCalledWith({ id: 'appearance.uiDensity', value: 'compact' })
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
    expect(listener).toHaveBeenLastCalledWith({ id: 'advanced.maxLogStorageMb', value: 456 })

    unsub()
  })

  it('short-circuits an array setting written with equal contents in a new array', () => {
    // The four `string[]` settings never arrive by the same reference twice: every
    // writer builds a fresh array, and an MCP `set_setting` deserializes one out of
    // JSON. Under `===` alone the guard was dead for all of them, so a teardown
    // resetting `mediaIndex.networkVolumes` to `[]` ran the whole cascade against a
    // value that was already `[]`.
    const listener = vi.fn()
    const unsub = onSettingChange(listener)

    setSetting('mediaIndex.networkVolumes', ['smb-a'])
    expect(listener).toHaveBeenCalledTimes(1)

    listener.mockClear()
    mockedEmit.mockClear()
    setSetting('mediaIndex.networkVolumes', ['smb-a'])

    expect(listener).not.toHaveBeenCalled()
    expect(settingsChangedEmits()).toHaveLength(0)

    unsub()
  })

  it('still cascades when an array setting really changes, order included', () => {
    const listener = vi.fn()
    const unsub = onSettingChange(listener)

    setSetting('mediaIndex.excludedFolders', ['/a', '/b'])
    expect(listener).toHaveBeenCalledTimes(1)

    // A removal, an addition, and a reorder are all real changes. Order matters
    // because these lists render in the order they're stored.
    listener.mockClear()
    setSetting('mediaIndex.excludedFolders', ['/a'])
    expect(listener).toHaveBeenCalledTimes(1)

    listener.mockClear()
    setSetting('mediaIndex.excludedFolders', ['/a', '/b'])
    expect(listener).toHaveBeenCalledTimes(1)

    listener.mockClear()
    setSetting('mediaIndex.excludedFolders', ['/b', '/a'])
    expect(listener).toHaveBeenCalledTimes(1)
    expect(listener).toHaveBeenLastCalledWith({ id: 'mediaIndex.excludedFolders', value: ['/b', '/a'] })

    unsub()
  })

  it('treats an emptied array as a change, then a no-op on the second empty', () => {
    // The exact teardown shape: opt a volume in, clear it, clear it again.
    const listener = vi.fn()
    const unsub = onSettingChange(listener)

    setSetting('mediaIndex.alwaysIndexVolumes', ['vol-1'])
    expect(listener).toHaveBeenCalledTimes(1)

    listener.mockClear()
    setSetting('mediaIndex.alwaysIndexVolumes', [])
    expect(listener).toHaveBeenCalledTimes(1)

    listener.mockClear()
    setSetting('mediaIndex.alwaysIndexVolumes', [])
    expect(listener).not.toHaveBeenCalled()

    unsub()
  })
})
