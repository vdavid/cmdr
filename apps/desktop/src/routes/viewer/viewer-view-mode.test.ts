import { describe, expect, it } from 'vitest'
import { modeForKey } from './viewer-view-mode'

describe('viewer mode keys', () => {
  it('maps unmodified 1, 2, and 3 to text, binary, and hex', () => {
    expect(modeForKey(new KeyboardEvent('keydown', { key: '1' }))).toBe('text')
    expect(modeForKey(new KeyboardEvent('keydown', { key: '2' }))).toBe('binary')
    expect(modeForKey(new KeyboardEvent('keydown', { key: '3' }))).toBe('hex')
  })

  it('leaves modified digits and other keys alone', () => {
    expect(modeForKey(new KeyboardEvent('keydown', { key: '2', metaKey: true }))).toBeNull()
    expect(modeForKey(new KeyboardEvent('keydown', { key: '3', shiftKey: true }))).toBeNull()
    expect(modeForKey(new KeyboardEvent('keydown', { key: '4' }))).toBeNull()
  })
})
