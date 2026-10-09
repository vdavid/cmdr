/**
 * The guards every decision prompt's letter keys share: what counts as the bare
 * key, and what must never answer a prompt. The per-dialog key maps and the
 * arming delay are pinned beside each dialog.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { answerDecisionKey, decisionKeyOf, type DecisionChoice } from './decision-keys'

/** Dispatches a keydown on `target` and hands back the event, with its real target set. */
function keydown(init: KeyboardEventInit, target: HTMLElement = document.body): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init })
  target.dispatchEvent(event)
  return event
}

function element<K extends keyof HTMLElementTagNameMap>(tag: K): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag)
  document.body.appendChild(el)
  return el
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('decisionKeyOf', () => {
  it('names a bare letter in uppercase, whichever case was typed', () => {
    expect(decisionKeyOf(keydown({ key: 'o', code: 'KeyO' }))).toBe('O')
    expect(decisionKeyOf(keydown({ key: 'O', code: 'KeyO' }))).toBe('O')
  })

  it('takes the typed letter over the key position, so AZERTY and QWERTZ answer by the label', () => {
    // AZERTY: the key labelled A sits where QWERTY has Q.
    expect(decisionKeyOf(keydown({ key: 'a', code: 'KeyQ' }))).toBe('A')
  })

  it('reaches the letter through the physical key on a non-Latin layout', () => {
    // Russian layout: the O position types "щ".
    expect(decisionKeyOf(keydown({ key: 'щ', code: 'KeyO' }))).toBe('O')
  })

  it('names Enter, but not on a focused button, which Enter activates instead', () => {
    expect(decisionKeyOf(keydown({ key: 'Enter', code: 'Enter' }))).toBe('Enter')
    expect(decisionKeyOf(keydown({ key: 'Enter', code: 'Enter' }, element('button')))).toBeNull()
  })

  it('lets a letter answer while a button has focus', () => {
    expect(decisionKeyOf(keydown({ key: 's', code: 'KeyS' }, element('button')))).toBe('S')
  })

  it.each([
    ['⌘', { metaKey: true }],
    ['⌃', { ctrlKey: true }],
    ['⌥', { altKey: true }],
    ['⇧', { shiftKey: true }],
  ])('ignores a letter with %s held: that is another combo', (_name, modifier) => {
    expect(decisionKeyOf(keydown({ key: 'o', code: 'KeyO', ...modifier }))).toBeNull()
    expect(decisionKeyOf(keydown({ key: 'Enter', code: 'Enter', ...modifier }))).toBeNull()
  })

  it('ignores a key repeat, so a held key cannot answer the next prompt too', () => {
    expect(decisionKeyOf(keydown({ key: 's', code: 'KeyS', repeat: true }))).toBeNull()
  })

  it('ignores keys while an IME composes, including its first `Process` keydown', () => {
    expect(decisionKeyOf(keydown({ key: 'o', code: 'KeyO', isComposing: true }))).toBeNull()
    expect(decisionKeyOf(keydown({ key: 'Process', code: 'KeyO' }))).toBeNull()
  })

  it('ignores keys typed into a text field', () => {
    expect(decisionKeyOf(keydown({ key: 'o', code: 'KeyO' }, element('input')))).toBeNull()
    expect(decisionKeyOf(keydown({ key: 'Enter', code: 'Enter' }, element('textarea')))).toBeNull()
  })

  it('ignores everything that is not a letter or Enter', () => {
    expect(decisionKeyOf(keydown({ key: 'Escape', code: 'Escape' }))).toBeNull()
    expect(decisionKeyOf(keydown({ key: '1', code: 'Digit1' }))).toBeNull()
    expect(decisionKeyOf(keydown({ key: ' ', code: 'Space' }))).toBeNull()
    expect(decisionKeyOf(keydown({ key: 'F2', code: 'F2' }))).toBeNull()
  })
})

describe('answerDecisionKey', () => {
  function choices(): { list: DecisionChoice[]; skip: () => void; overwrite: () => void; locked: () => void } {
    const skip = vi.fn()
    const overwrite = vi.fn()
    const locked = vi.fn()
    return {
      list: [
        { key: 'S', enabled: true, run: skip },
        { key: 'O', enabled: true, run: overwrite },
        { key: 'M', enabled: false, run: locked },
      ],
      skip,
      overwrite,
      locked,
    }
  }

  it('runs the choice its letter names, once, and claims the key', () => {
    const { list, skip, overwrite } = choices()
    const event = keydown({ key: 'o', code: 'KeyO' })
    expect(answerDecisionKey(event, list, 'S')).toBe(true)
    expect(overwrite).toHaveBeenCalledTimes(1)
    expect(skip).not.toHaveBeenCalled()
    expect(event.defaultPrevented).toBe(true)
  })

  it('runs the Enter choice on Enter', () => {
    const { list, skip, overwrite } = choices()
    expect(answerDecisionKey(keydown({ key: 'Enter', code: 'Enter' }), list, 'S')).toBe(true)
    expect(skip).toHaveBeenCalledTimes(1)
    expect(overwrite).not.toHaveBeenCalled()
  })

  it('does nothing for a disabled choice, and leaves the key unclaimed', () => {
    const { list, locked } = choices()
    const event = keydown({ key: 'm', code: 'KeyM' })
    expect(answerDecisionKey(event, list, 'S')).toBe(false)
    expect(locked).not.toHaveBeenCalled()
    expect(event.defaultPrevented).toBe(false)
  })

  it('does nothing for a letter no choice has', () => {
    const { list, skip, overwrite } = choices()
    expect(answerDecisionKey(keydown({ key: 'x', code: 'KeyX' }), list, 'S')).toBe(false)
    expect(skip).not.toHaveBeenCalled()
    expect(overwrite).not.toHaveBeenCalled()
  })
})
