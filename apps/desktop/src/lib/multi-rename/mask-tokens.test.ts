import { describe, it, expect } from 'vitest'
import { counterKind } from './counter-token'
import { findTokens, replaceToken, scanPlaceholders } from './mask-tokens'

const KINDS = [counterKind]

describe('mask placeholders', () => {
  it('finds each bracketed placeholder with its place in the text', () => {
    expect(scanPlaceholders('IMG_[C:3] [N]')).toEqual([
      { from: 4, to: 9, inner: 'C:3' },
      { from: 10, to: 13, inner: 'N' },
    ])
  })

  it('reads [[ as a literal bracket, as the backend does', () => {
    expect(scanPlaceholders('[[C] [C]')).toEqual([{ from: 5, to: 8, inner: 'C' }])
  })

  it('stops at an unclosed bracket', () => {
    expect(scanPlaceholders('[N] [C')).toEqual([{ from: 0, to: 3, inner: 'N' }])
  })

  it('keeps only the tokens a kind can edit', () => {
    const tokens = findTokens('[N] [C10] [Cx] [C]', KINDS)
    expect(tokens.map((t) => [t.span.from, t.kind.id, t.value])).toEqual([
      [4, 'counter', { start: 10, step: 1, digits: 1 }],
      [15, 'counter', { start: 1, step: 1, digits: 1 }],
    ])
  })

  it('rewrites one token and puts the caret right after it', () => {
    const [token] = findTokens('IMG [C] [N]', KINDS)
    expect(replaceToken('IMG [C] [N]', token.span, 'C10:3')).toEqual({ mask: 'IMG [C10:3] [N]', caret: 11 })
  })
})
