import { describe, it, expect } from 'vitest'
import { classifySelectionDialogKey } from './selection-dialog-keys'

function ev(opts: Partial<KeyboardEventInit> & { key: string }): KeyboardEvent {
  return new KeyboardEvent('keydown', opts)
}

describe('classifySelectionDialogKey', () => {
  it('opens add on bare `+`', () => {
    expect(classifySelectionDialogKey(ev({ key: '+' }))).toBe('open-add')
  })

  it('opens add on Shift+= (US QWERTY produces key === `+`)', () => {
    // event.key === '+' is the contract; the shift modifier is implicit and we
    // intentionally don't filter on it.
    expect(classifySelectionDialogKey(ev({ key: '+', shiftKey: true }))).toBe('open-add')
  })

  it('opens remove on bare `-`', () => {
    expect(classifySelectionDialogKey(ev({ key: '-' }))).toBe('open-remove')
  })

  it('opens remove on Shift+- (US QWERTY produces key === `_`, code === `Minus`)', () => {
    expect(classifySelectionDialogKey(ev({ key: '_', code: 'Minus', shiftKey: true }))).toBe('open-remove')
  })

  it('opens remove on the physical Minus key regardless of layout (code === `Minus`)', () => {
    expect(classifySelectionDialogKey(ev({ key: '-', code: 'Minus' }))).toBe('open-remove')
  })

  it('returns null when meta is held', () => {
    expect(classifySelectionDialogKey(ev({ key: '+', metaKey: true }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: '-', metaKey: true }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: '_', code: 'Minus', metaKey: true }))).toBeNull()
  })

  it('returns null when alt is held', () => {
    expect(classifySelectionDialogKey(ev({ key: '+', altKey: true }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: '-', altKey: true }))).toBeNull()
  })

  it('returns null when ctrl is held', () => {
    expect(classifySelectionDialogKey(ev({ key: '+', ctrlKey: true }))).toBeNull()
  })

  it('returns null on other keys', () => {
    expect(classifySelectionDialogKey(ev({ key: '=' }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: 'a' }))).toBeNull()
    expect(classifySelectionDialogKey(ev({ key: 'Enter' }))).toBeNull()
  })
})
