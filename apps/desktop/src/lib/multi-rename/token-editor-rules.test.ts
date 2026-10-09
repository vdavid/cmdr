import { describe, it, expect } from 'vitest'
import { nextEditor, tokenInside, type EditorOpen } from './token-editor-rules'

const span = (from: number, to: number) => ({ span: { from, to, inner: '' } })

describe('tokenInside', () => {
  const tokens = [span(4, 7), span(10, 17)]

  it('finds the token the caret is strictly inside', () => {
    expect(tokenInside(tokens, 5)).toBe(tokens[0])
    expect(tokenInside(tokens, 16)).toBe(tokens[1])
  })

  it('leaves a caret on either bracket’s outer side alone, so completing a token never pops its editor', () => {
    expect(tokenInside(tokens, 4)).toBeUndefined()
    expect(tokenInside(tokens, 7)).toBeUndefined()
  })
})

describe('nextEditor', () => {
  const caretOn4: EditorOpen = { from: 4, reason: 'caret' }
  const hoverOn4: EditorOpen = { from: 4, reason: 'hover' }
  const focusOn4: EditorOpen = { from: 4, reason: 'focus' }

  it('opens on the caret’s token and follows the caret to another one', () => {
    expect(nextEditor(null, { type: 'caret', from: 4 })).toEqual(caretOn4)
    expect(nextEditor(caretOn4, { type: 'caret', from: 10 })).toEqual({ from: 10, reason: 'caret' })
  })

  it('closes once the caret leaves a token it opened for', () => {
    expect(nextEditor(caretOn4, { type: 'caret', from: null })).toBeNull()
  })

  it('takes over a hover-opened editor when the caret lands in a token', () => {
    expect(nextEditor(hoverOn4, { type: 'caret', from: 10 })).toEqual({ from: 10, reason: 'caret' })
  })

  it('leaves a hover-opened editor to the pointer when the caret is elsewhere', () => {
    expect(nextEditor(hoverOn4, { type: 'caret', from: null })).toEqual(hoverOn4)
  })

  it('opens on hover only when nothing else holds the editor', () => {
    expect(nextEditor(null, { type: 'hover', from: 4 })).toEqual(hoverOn4)
    expect(nextEditor(hoverOn4, { type: 'hover', from: 10 })).toEqual({ from: 10, reason: 'hover' })
    expect(nextEditor(caretOn4, { type: 'hover', from: 10 })).toEqual(caretOn4)
    expect(nextEditor(focusOn4, { type: 'hover', from: 10 })).toEqual(focusOn4)
  })

  it('closes a hover-opened editor when the pointer leaves, and nothing else', () => {
    expect(nextEditor(hoverOn4, { type: 'hover', from: null })).toBeNull()
    expect(nextEditor(caretOn4, { type: 'hover', from: null })).toEqual(caretOn4)
    expect(nextEditor(focusOn4, { type: 'hover', from: null })).toEqual(focusOn4)
  })

  it('keeps an editor the user is working in, whatever opened it', () => {
    expect(nextEditor(hoverOn4, { type: 'engage' })).toEqual(focusOn4)
    expect(nextEditor(caretOn4, { type: 'engage' })).toEqual(focusOn4)
  })

  it('opens straight into the editor on an explicit request (ArrowDown, the marker)', () => {
    expect(nextEditor(null, { type: 'request', from: 10 })).toEqual({ from: 10, reason: 'focus' })
    expect(nextEditor(caretOn4, { type: 'request', from: 4 })).toEqual(focusOn4)
  })

  it('hands an editor back to the caret when the user returns to the field', () => {
    expect(nextEditor(focusOn4, { type: 'caret', from: 4 })).toEqual(caretOn4)
    expect(nextEditor(focusOn4, { type: 'caret', from: null })).toBeNull()
  })

  it('closes when its token disappears', () => {
    expect(nextEditor(caretOn4, { type: 'tokens', froms: [10] })).toBeNull()
    expect(nextEditor(focusOn4, { type: 'tokens', froms: [4, 10] })).toEqual(focusOn4)
  })

  it('closes on dismiss', () => {
    expect(nextEditor(focusOn4, { type: 'dismiss' })).toBeNull()
  })
})
