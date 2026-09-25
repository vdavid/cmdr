import { describe, expect, it } from 'vitest'
import { modeForKey } from './viewer-view-mode'

describe('viewer mode keys', () => {
  it('maps unmodified 1, 2, and 3 to text, binary, and hex', () => {
    expect(modeForKey(new KeyboardEvent('keydown', { key: '1' }), false)).toBe('text')
    expect(modeForKey(new KeyboardEvent('keydown', { key: '2' }), false)).toBe('binary')
    expect(modeForKey(new KeyboardEvent('keydown', { key: '3' }), false)).toBe('hex')
  })

  it('maps 0 to media only when the file has a dedicated viewer', () => {
    expect(modeForKey(new KeyboardEvent('keydown', { key: '0' }), true)).toBe('media')
    expect(modeForKey(new KeyboardEvent('keydown', { key: '0' }), false)).toBeNull()
  })

  it('leaves modified digits and other keys alone', () => {
    expect(modeForKey(new KeyboardEvent('keydown', { key: '0', metaKey: true }), true)).toBeNull()
    expect(modeForKey(new KeyboardEvent('keydown', { key: '2', metaKey: true }), false)).toBeNull()
    expect(modeForKey(new KeyboardEvent('keydown', { key: '3', shiftKey: true }), false)).toBeNull()
    expect(modeForKey(new KeyboardEvent('keydown', { key: '4' }), false)).toBeNull()
  })
})
