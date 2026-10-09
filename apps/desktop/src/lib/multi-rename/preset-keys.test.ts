import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { presetKeyOf } from './preset-keys'

// `formatKeyCombo` emits ⌘-form modifiers only when `isMacOS()` is true, and happy-dom reports a Linux UA.
const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
beforeEach(() => {
  navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
})
afterEach(() => navigatorSpy.mockReset())

function key(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent('keydown', init)
}

describe('presetKeyOf', () => {
  it('reads F2 as opening the Presets menu', () => {
    expect(presetKeyOf(key({ key: 'F2', code: 'F2' }))).toBe('openMenu')
  })

  it('reads ⌘S as saving a preset', () => {
    expect(presetKeyOf(key({ key: 's', code: 'KeyS', metaKey: true }))).toBe('save')
  })

  it('leaves combos that only contain those keys alone', () => {
    expect(presetKeyOf(key({ key: 'F2', code: 'F2', shiftKey: true }))).toBeNull()
    expect(presetKeyOf(key({ key: 's', code: 'KeyS', metaKey: true, altKey: true }))).toBeNull()
    expect(presetKeyOf(key({ key: 's', code: 'KeyS' }))).toBeNull()
  })

  it('never acts mid-composition', () => {
    expect(presetKeyOf(key({ key: 'F2', code: 'F2', isComposing: true }))).toBeNull()
    expect(presetKeyOf(key({ key: 's', code: 'KeyS', metaKey: true, isComposing: true }))).toBeNull()
  })
})
