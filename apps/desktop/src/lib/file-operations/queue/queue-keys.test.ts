import { beforeEach, describe, expect, it } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { queueKeyAction } from './queue-keys'

// `formatKeyCombo` emits ⌘-form modifiers only when `isMacOS()` is true, and Vitest runs off macOS.
beforeEach(() => withPlatform('macos'))

function key(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent('keydown', init)
}

describe('queueKeyAction', () => {
  it.each([
    [
      { key: 'ArrowUp', code: 'ArrowUp' },
      { kind: 'cursor', to: 'previous' },
    ],
    [
      { key: 'ArrowDown', code: 'ArrowDown' },
      { kind: 'cursor', to: 'next' },
    ],
    [
      { key: 'Home', code: 'Home' },
      { kind: 'cursor', to: 'first' },
    ],
    [
      { key: 'End', code: 'End' },
      { kind: 'cursor', to: 'last' },
    ],
    [
      { key: 'ArrowUp', code: 'ArrowUp', shiftKey: true },
      { kind: 'extend', to: 'previous' },
    ],
    [
      { key: 'ArrowDown', code: 'ArrowDown', shiftKey: true },
      { kind: 'extend', to: 'next' },
    ],
    [{ key: ' ', code: 'Space' }, { kind: 'toggleSelected' }],
    [{ key: 'a', code: 'KeyA', metaKey: true }, { kind: 'selectAll' }],
    [{ key: 'Enter', code: 'Enter' }, { kind: 'show' }],
    [{ key: 'Backspace', code: 'Backspace' }, { kind: 'cancelOrDismiss' }],
    [{ key: 'Delete', code: 'Delete' }, { kind: 'cancelOrDismiss' }],
    [
      { key: 'ArrowUp', code: 'ArrowUp', altKey: true },
      { kind: 'move', direction: 'earlier' },
    ],
    [
      { key: 'ArrowDown', code: 'ArrowDown', altKey: true },
      { kind: 'move', direction: 'later' },
    ],
    [
      { key: 'ArrowRight', code: 'ArrowRight' },
      { kind: 'control', step: 1 },
    ],
    [
      { key: 'ArrowLeft', code: 'ArrowLeft' },
      { kind: 'control', step: -1 },
    ],
    [{ key: 'Escape', code: 'Escape' }, { kind: 'close' }],
    [{ key: 's', code: 'KeyS' }, { kind: 'show' }],
    [{ key: 'p', code: 'KeyP' }, { kind: 'pauseOrResume' }],
    [{ key: 'c', code: 'KeyC' }, { kind: 'cancel' }],
    [{ key: 'b', code: 'KeyB' }, { kind: 'rollback' }],
    // A Russian layout's B key types "и": the physical key answers, as on the decision prompts.
    [{ key: 'и', code: 'KeyB' }, { kind: 'rollback' }],
  ])('reads %o as %o', (init, action) => {
    expect(queueKeyAction(key(init))).toEqual(action)
  })

  // The whole combo, never a superset: ⌥⌘A is somebody else's key, and ⌥⇧↑ is neither
  // a reorder nor an extend.
  it('leaves combos that only contain a queue key alone', () => {
    expect(queueKeyAction(key({ key: 'a', code: 'KeyA', metaKey: true, altKey: true }))).toBeNull()
    expect(queueKeyAction(key({ key: 'ArrowUp', code: 'ArrowUp', altKey: true, shiftKey: true }))).toBeNull()
    expect(queueKeyAction(key({ key: 'ArrowDown', code: 'ArrowDown', metaKey: true }))).toBeNull()
    expect(queueKeyAction(key({ key: 'Enter', code: 'Enter', metaKey: true }))).toBeNull()
  })

  // A held P would pause and resume in turn, and ⌘C is copy.
  it('reads a letter only bare and unrepeated', () => {
    expect(queueKeyAction(key({ key: 'p', code: 'KeyP', repeat: true }))).toBeNull()
    expect(queueKeyAction(key({ key: 'c', code: 'KeyC', metaKey: true }))).toBeNull()
    expect(queueKeyAction(key({ key: 'C', code: 'KeyC', shiftKey: true }))).toBeNull()
  })

  it('answers nothing for a key the window has no use for', () => {
    expect(queueKeyAction(key({ key: 'x', code: 'KeyX' }))).toBeNull()
  })
})
