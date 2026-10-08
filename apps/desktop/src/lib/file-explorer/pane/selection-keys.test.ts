/**
 * The file list's selection keys must match their WHOLE combo.
 *
 * Regression anchor: `⌥⌘A` (Ask Cmdr) used to select every file, because the pane
 * matched `e.key === 'a' && e.metaKey` — a modifier SUPERSET — and only called
 * `preventDefault()`, so the event still reached the document dispatcher and both
 * commands ran. Resolving through the registry makes that impossible by construction.
 */

import { describe, it, expect, vi, beforeAll, afterAll } from 'vitest'
import { classifySelectionKey } from './selection-keys'

const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
beforeAll(() => {
  navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
})
afterAll(() => navigatorSpy.mockReset())

function keydown(overrides: Partial<KeyboardEvent>): KeyboardEvent {
  return {
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    key: '',
    code: '',
    ...overrides,
  } as KeyboardEvent
}

describe('classifySelectionKey', () => {
  it('maps each selection combo to its command', () => {
    expect(classifySelectionKey(keydown({ key: ' ' }))).toBe('selection.toggle')
    expect(classifySelectionKey(keydown({ key: 'Insert' }))).toBe('selection.toggleAndDown')
    expect(classifySelectionKey(keydown({ key: 'a', metaKey: true }))).toBe('selection.selectAll')
    expect(classifySelectionKey(keydown({ key: 'a', metaKey: true, shiftKey: true }))).toBe('selection.deselectAll')
  })

  it('maps whichever key types `*` to invert, on any layout', () => {
    // US ⇧8, Swedish ⇧', and the numpad all type `*`. Hungarian ⇧8 types `(`, so
    // it's that user's `(` key, not their `*`.
    expect(classifySelectionKey(keydown({ key: '*', code: 'Digit8', shiftKey: true }))).toBe('selection.invert')
    expect(classifySelectionKey(keydown({ key: '*', code: 'Backslash', shiftKey: true }))).toBe('selection.invert')
    expect(classifySelectionKey(keydown({ key: '*', code: 'NumpadMultiply' }))).toBe('selection.invert')
    expect(classifySelectionKey(keydown({ key: '(', code: 'Digit8', shiftKey: true }))).toBeNull()
  })

  it('maps the PC Insert key to toggle-and-down, though macOS reports it as Help', () => {
    expect(classifySelectionKey(keydown({ key: 'Help', code: 'Help' }))).toBe('selection.toggleAndDown')
  })

  it('maps ⌥⇧= to select-same-kind by its physical key, whatever the layout types', () => {
    // macOS types `±` on a US layout with ⌥⇧ held, and something else again on a
    // Hungarian one; both are the Equal key. ⌥ makes it a command, named by position.
    expect(classifySelectionKey(keydown({ key: '±', code: 'Equal', altKey: true, shiftKey: true }))).toBe(
      'selection.selectSameKind',
    )
    expect(classifySelectionKey(keydown({ key: '=', code: 'Equal', altKey: true, shiftKey: true }))).toBe(
      'selection.selectSameKind',
    )
  })

  it('maps the numpad ⌥+ to select-same-kind, the way Total Commander users type it', () => {
    // `Alt+Num +` in Total Commander. The numpad key reports `+` with no Shift on
    // every layout, so it needs its own default.
    expect(classifySelectionKey(keydown({ key: '+', code: 'NumpadAdd', altKey: true }))).toBe(
      'selection.selectSameKind',
    )
  })

  it('ignores a bare `=` and a ⌘-carrying ⌥⇧=', () => {
    // A bare `=` has to stay typable; `⌘⌥⇧=` is a different combo entirely.
    expect(classifySelectionKey(keydown({ key: '=', code: 'Equal' }))).toBeNull()
    expect(
      classifySelectionKey(keydown({ key: '±', code: 'Equal', altKey: true, shiftKey: true, metaKey: true })),
    ).toBeNull()
  })

  it('ignores an unshifted 8 and a ⌘-carrying ⇧8', () => {
    expect(classifySelectionKey(keydown({ key: '8', code: 'Digit8' }))).toBeNull()
    expect(classifySelectionKey(keydown({ key: '*', code: 'Digit8', shiftKey: true, metaKey: true }))).toBeNull()
  })

  it('ignores ⌥⌘A, so Ask Cmdr does not also select every file', () => {
    expect(classifySelectionKey(keydown({ key: 'a', metaKey: true, altKey: true }))).toBeNull()
  })

  it('ignores every other modifier superset of a selection key', () => {
    expect(classifySelectionKey(keydown({ key: 'a', metaKey: true, ctrlKey: true }))).toBeNull()
    expect(classifySelectionKey(keydown({ key: 'a', metaKey: true, altKey: true, shiftKey: true }))).toBeNull()
    // ⇧Space is Quick Look; ⌘Space is Spotlight. Neither toggles the selection.
    expect(classifySelectionKey(keydown({ key: ' ', shiftKey: true }))).toBeNull()
    expect(classifySelectionKey(keydown({ key: ' ', metaKey: true }))).toBeNull()
    expect(classifySelectionKey(keydown({ key: 'Insert', metaKey: true }))).toBeNull()
  })

  it('ignores unrelated keys', () => {
    expect(classifySelectionKey(keydown({ key: 'a' }))).toBeNull()
    expect(classifySelectionKey(keydown({ key: 'ArrowDown' }))).toBeNull()
  })
})
