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

  it('maps ⇧8 to invert by its physical key, whatever the layout types', () => {
    // US QWERTY types `*`, Hungarian types `(`; both are the Digit8 key with Shift.
    expect(classifySelectionKey(keydown({ key: '*', code: 'Digit8', shiftKey: true }))).toBe('selection.invert')
    expect(classifySelectionKey(keydown({ key: '(', code: 'Digit8', shiftKey: true }))).toBe('selection.invert')
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
