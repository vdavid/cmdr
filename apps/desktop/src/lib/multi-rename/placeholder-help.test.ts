import { describe, it, expect } from 'vitest'
import { FULL_NAME_MASK, PLACEHOLDER_HELP, exampleMasks, examplePieces, renderedByMask } from './placeholder-help'

describe('placeholder help', () => {
  it('asks for each mask once: the full name, every placeholder, its syntax, and what surrounds each part', () => {
    const masks = exampleMasks(PLACEHOLDER_HELP)
    expect(masks[0]).toBe(FULL_NAME_MASK)
    expect(masks).toContain('[E]')
    expect(masks).toContain('[E2-3]')
    expect(masks).toContain('[E4-]')
    expect(new Set(masks).size).toBe(masks.length)
  })

  it('leaves counters out of the request: their numbers come from the token itself', () => {
    expect(exampleMasks(PLACEHOLDER_HELP).some((mask) => mask.startsWith('[C'))).toBe(false)
  })

  it('pairs each mask with what the backend rendered for it, dropping the ones it couldn’t', () => {
    const rendered = renderedByMask(['[E]', '[Q]'], { rendered: ['pdf', null], sampleDate: false })
    expect([...rendered]).toEqual([['[E]', 'pdf']])
  })

  it('marks the part a range takes, with what the field keeps around it quiet', () => {
    const rendered = new Map([
      ['[E1]', 'p'],
      ['[E2-3]', 'df'],
      ['[E4-]', ''],
    ])
    expect(examplePieces({ mask: '[E2-3]', around: { before: '[E1]', after: '[E4-]' } }, rendered)).toEqual([
      { text: 'p', taken: false },
      { text: 'df', taken: true },
    ])
  })

  it('shows a whole placeholder as taken entirely', () => {
    expect(examplePieces({ mask: '[E]' }, new Map([['[E]', 'pdf']]))).toEqual([{ text: 'pdf', taken: true }])
  })

  it('counts a counter’s first three numbers, padded as the names get them', () => {
    expect(examplePieces({ mask: '[C10+5:3]' }, new Map())).toEqual([{ text: '010, 015, 020…', taken: true }])
  })

  it('has no example for a mask the backend didn’t render', () => {
    expect(examplePieces({ mask: '[N3]', around: { before: '[N1-2]' } }, new Map())).toBeNull()
  })
})
