import { describe, it, expect, vi, beforeAll, afterAll } from 'vitest'
import { classifySelectionDialogKey } from './selection-dialog-keys'

const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
beforeAll(() => {
  navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
})
afterAll(() => navigatorSpy.mockReset())

function ev(opts: Partial<KeyboardEventInit> & { key: string }): KeyboardEvent {
  return new KeyboardEvent('keydown', opts)
}

describe('classifySelectionDialogKey', () => {
  it.each([
    ['a bare +', { key: '+', code: 'Minus' }],
    ['US ⇧=', { key: '+', code: 'Equal', shiftKey: true }],
    ['Hungarian ⇧3', { key: '+', code: 'Digit3', shiftKey: true }],
    ['the numpad', { key: '+', code: 'NumpadAdd' }],
  ])('opens add on %s', (_how, init) => {
    expect(classifySelectionDialogKey(ev(init))).toBe('open-add')
  })

  it.each([
    ['a bare -', { key: '-', code: 'Minus' }],
    ['the numpad', { key: '-', code: 'NumpadSubtract' }],
    ['Swedish -', { key: '-', code: 'Slash' }],
    ['an AltGr-style ⌥ layout', { key: '-', code: 'Digit6', altKey: true }],
  ])('opens remove on %s', (_how, init) => {
    expect(classifySelectionDialogKey(ev(init))).toBe('open-remove')
  })

  it('leaves ⌥ with + to select-same-kind, which binds `⌥+` exactly', () => {
    // The one place the AltGr fallback yields: a layout that types `+` with ⌥
    // reports exactly the numpad's `⌥+`, and the exact binding wins.
    expect(classifySelectionDialogKey(ev({ key: '+', code: 'NumpadAdd', altKey: true }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: '+', code: 'Digit1', altKey: true }))).toBeNull()
  })

  it('returns null with ⌘ or ⌃ held', () => {
    expect(classifySelectionDialogKey(ev({ key: '+', metaKey: true }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: '-', metaKey: true }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: '+', ctrlKey: true }))).toBeNull()
  })

  it('returns null on other keys, including the _ that US ⇧- types', () => {
    expect(classifySelectionDialogKey(ev({ key: '_', code: 'Minus', shiftKey: true }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: '=' }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: 'a' }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: 'Enter' }))).toBeNull()
  })
})
