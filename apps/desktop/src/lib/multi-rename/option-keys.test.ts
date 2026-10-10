import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { optionKeyOf } from './option-keys'

// `formatKeyCombo` emits ⌘-form modifiers only when `isMacOS()` is true, and happy-dom reports a Linux UA.
const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
beforeEach(() => {
  navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
})
afterEach(() => navigatorSpy.mockReset())

/** ⌘⌥ plus a letter, as macOS sends it on a US layout: `key` is what ⌥ composed. */
function optionCombo(code: string, composed: string, init: KeyboardEventInit = {}): KeyboardEvent {
  return new KeyboardEvent('keydown', { key: composed, code, metaKey: true, altKey: true, ...init })
}

describe('optionKeyOf', () => {
  it.each([
    ['KeyN', 'Dead', 'removeDiacritics'],
    ['KeyG', '©', 'greekToLatin'],
    ['KeyP', 'π', 'normalizeUnicode'],
    ['KeyI', 'Dead', 'caseSensitive'],
    ['KeyF', 'ƒ', 'firstOnly'],
    ['KeyE', 'Dead', 'includeExtension'],
    ['KeyR', '®', 'regex'],
    ['KeyW', '∑', 'substitute'],
  ])('reads ⌘⌥ on %s (typed %s) as flipping %s', (code, composed, field) => {
    expect(optionKeyOf(optionCombo(code, composed))).toEqual({ kind: 'toggle', field })
  })

  it('reads ⌘⌥U as opening Letter case', () => {
    expect(optionKeyOf(optionCombo('KeyU', 'Dead'))).toEqual({ kind: 'letterCase' })
  })

  it('leaves other combos with those letters alone', () => {
    expect(optionKeyOf(optionCombo('KeyR', '®', { shiftKey: true }))).toBeNull()
    expect(optionKeyOf(new KeyboardEvent('keydown', { key: 'r', code: 'KeyR', metaKey: true }))).toBeNull()
    expect(optionKeyOf(new KeyboardEvent('keydown', { key: '®', code: 'KeyR', altKey: true }))).toBeNull()
  })

  it('never acts mid-composition', () => {
    expect(optionKeyOf(optionCombo('KeyR', '®', { isComposing: true }))).toBeNull()
  })
})
