/** The queue window's keys: Esc and F2 close it; anything with a modifier is someone else's. */

import { describe, it, expect } from 'vitest'
import { queueWindowKeyAction } from './queue-window-keys'

const key = (k: string, mods: KeyboardEventInit = {}) => new KeyboardEvent('keydown', { key: k, ...mods })

describe('queueWindowKeyAction', () => {
  it('closes on Esc and on F2, the key that opened it from the progress dialog', () => {
    expect(queueWindowKeyAction(key('Escape'))).toBe('close')
    expect(queueWindowKeyAction(key('F2'))).toBe('close')
  })

  it('leaves other keys and modified F2 / Esc alone', () => {
    expect(queueWindowKeyAction(key('Enter'))).toBeNull()
    expect(queueWindowKeyAction(key('F2', { shiftKey: true }))).toBeNull()
    expect(queueWindowKeyAction(key('F2', { metaKey: true }))).toBeNull()
    expect(queueWindowKeyAction(key('Escape', { altKey: true }))).toBeNull()
  })
})
