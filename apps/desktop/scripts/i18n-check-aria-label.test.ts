/**
 * Tests for the ARIA-LABEL CONTAINMENT check (`i18n-check-aria-label.ts`).
 *
 * WCAG 2.5.3: a control's accessible name must CONTAIN its visible label, so a
 * voice-control user can say what they see. The pairing is by naming convention
 * (`foo` + `fooAria`), and English containment is the gate that says a pair is
 * real, so a key that merely ends in `Aria` without being its sibling's
 * accessible name is never checked.
 */
import { describe, it, expect } from 'vitest'
import { ariaPairs, containsLabel, checkLocale } from './i18n-check-aria-label.ts'
import type { Catalog } from './i18n-catalog-lib.ts'

const cat = (messages: Record<string, string>): Catalog => ({ messages, metadata: {} })

describe('containsLabel', () => {
  it('ignores case, so English "Background" ⊂ "…in the background" counts', () => {
    expect(containsLabel('Keep this running in the background', 'Background')).toBe(true)
  })
  it('ignores placeholders, tags, and punctuation', () => {
    expect(containsLabel('Remove {folder} from indexing!', 'Remove')).toBe(true)
    expect(containsLabel('<b>Remove</b> it', 'Remove')).toBe(true)
  })
  it('unescapes the ICU doubled apostrophe on both sides', () => {
    expect(containsLabel("Don''t show it again", "Don''t show")).toBe(true)
  })
  it('is false when the aria paraphrases instead of containing', () => {
    expect(containsLabel('把 {folder} 移出索引', '移除')).toBe(false)
  })
  it('is true once the aria uses the label verbatim', () => {
    expect(containsLabel('從索引中移除 {folder}', '移除')).toBe(true)
  })
})

describe('ariaPairs: only REAL pairs, gated on English containment', () => {
  it('pairs foo with fooAria and fooAriaLabel', () => {
    const source = cat({ a: 'Remove', aAria: 'Remove it', b: 'Send', bAriaLabel: 'Send the report' })
    expect(
      ariaPairs(source)
        .map((p) => p.ariaKey)
        .sort(),
    ).toEqual(['aAria', 'bAriaLabel'])
  })
  it('skips a pair English itself does not satisfy (not an accessible-name pair)', () => {
    // A countdown sentence and a timer description are two different strings, not a label and its name.
    expect(ariaPairs(cat({ a: 'Quitting in 5 seconds', aAria: 'Time until Cmdr quits' }))).toEqual([])
  })
  it('skips an Aria key with no sibling label', () => {
    expect(ariaPairs(cat({ loneAria: 'Some description' }))).toEqual([])
  })
})

describe('checkLocale', () => {
  const source = cat({ rm: 'Remove', rmAria: 'Remove {folder} from indexing' })

  it('finds nothing when the locale keeps the label inside the aria', () => {
    expect(checkLocale(source, cat({ rm: '移除', rmAria: '從索引中移除 {folder}' }))).toEqual([])
  })
  it('flags a locale whose aria paraphrases its own label', () => {
    const out = checkLocale(source, cat({ rm: '移除', rmAria: '把 {folder} 移出索引' }))
    expect(out).toHaveLength(1)
    expect(out[0].ariaKey).toBe('rmAria')
  })
  it('skips a key the locale does not define (an overlay forking neither half)', () => {
    expect(checkLocale(source, cat({}))).toEqual([])
  })
  it('still checks when an overlay forks only the aria, using the source label it renders', () => {
    const out = checkLocale(source, cat({ rmAria: 'Take {folder} out of indexing' }), true)
    expect(out).toHaveLength(1)
  })
})
