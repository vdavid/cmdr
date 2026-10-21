import { describe, it, expect } from 'vitest'
import {
  DATE_LEAD_MASK,
  FOLDER_LEAD_MASK,
  PLACEHOLDER_HELP,
  hintMask,
  hintPieces,
  placeholderExamples,
} from './placeholder-help'
import { MARK_END, MARK_START, SAMPLE_FILE } from './rename-examples'

describe('placeholder help', () => {
  it('asks for each example once, on the sample file, the name mask alone', () => {
    const asked = placeholderExamples(PLACEHOLDER_HELP)
    expect([...asked.keys()]).toContain(FOLDER_LEAD_MASK)
    expect([...asked.keys()]).toContain(DATE_LEAD_MASK)
    expect(asked.get(hintMask({ mask: '[E]' }))).toMatchObject({
      fileName: SAMPLE_FILE,
      spec: { nameMask: `${MARK_START}[E]${MARK_END}`, extensionMask: '' },
    })
  })

  it('renders a range with what the field keeps around it, the part it takes between the marks', () => {
    expect(hintMask({ mask: '[E2-3]', around: { before: '[E1]', after: '[E4-]' } })).toBe(
      `[E1]${MARK_START}[E2-3]${MARK_END}[E4-]`,
    )
  })

  it('leaves counters out of the request: their numbers come from the token itself', () => {
    expect([...placeholderExamples(PLACEHOLDER_HELP).keys()].some((mask) => mask.includes('[C'))).toBe(false)
  })

  it('marks the part a range takes, with what the field keeps around it quiet', () => {
    const hint = { mask: '[N3]', around: { before: '[N1-2]', after: '[N4-]' } }
    const rendered = new Map([[hintMask(hint), `Be${MARK_START}a${MARK_END}ch day`]])
    expect(hintPieces(hint, rendered)).toEqual([
      { text: 'Be', marked: false },
      { text: 'a', marked: true },
      { text: 'ch day', marked: false },
    ])
  })

  it('counts a counter’s first three numbers, padded as the names get them', () => {
    expect(hintPieces({ mask: '[C10+5:3]' }, new Map())).toEqual([{ text: '010, 015, 020…', marked: true }])
  })

  it('shows the name’s ranges from one character to the last few, each with what the field keeps around it', () => {
    const name = PLACEHOLDER_HELP.find((help) => help.placeholder === '[N]')
    // `mask_test.rs` holds the engine to these masks' examples on “Beach day”.
    expect(name?.syntax.map(hintMask)).toEqual([
      `[N1-2]${MARK_START}[N3]${MARK_END}[N4-]`,
      `[N1]${MARK_START}[N2-5]${MARK_END}[N6-]`,
      `${MARK_START}[N1--2]${MARK_END}[N-1-]`,
      `[N1-6]${MARK_START}[N7,3]${MARK_END}[N10-]`,
      `[N1--4]${MARK_START}[N-3,3]${MARK_END}`,
    ])
  })

  it('has no example for a mask the engine didn’t render', () => {
    expect(hintPieces({ mask: '[N3]', around: { before: '[N1-2]' } }, new Map())).toBeNull()
  })
})
