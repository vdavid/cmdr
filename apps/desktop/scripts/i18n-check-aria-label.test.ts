/**
 * Tests for the ARIA-LABEL CONTAINMENT check (`i18n-check-aria-label.ts`).
 *
 * WCAG 2.5.3: a control's accessible name must CONTAIN its visible label, so a
 * voice-control user can say what they see. The pairing is by naming convention
 * (`foo` + `fooAria`), and English containment is the gate that says a pair is
 * real, so a key that merely ends in `Aria` without being its sibling's
 * accessible name is never checked.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { ariaPairs, containsLabel, checkLocale, runAriaCheck } from './i18n-check-aria-label.ts'
import { EXIT_CLEAN } from './i18n-locale-check-lib.ts'
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

describe('checkLocale: platform blocks, compared per rendered OS', () => {
  // The label is a block, its accessible name plain: branch names don't line up across the two keys,
  // so the pair is judged on what each OS renders.
  const source = cat({
    show: '{platform, select, macos {Show in Finder} windows {Show in File Explorer} other {Show in file manager}}',
    showAria: '{platform, select, macos {Show {name} in Finder} other {Show {name} in file manager}}',
  })

  it('holds a pair only on the OSes where English holds it', () => {
    expect(ariaPairs(source)).toEqual([{ labelKey: 'show', ariaKey: 'showAria', platforms: ['macos', 'linux'] }])
  })

  it('flags a translated rendering that lost its label, naming the OS', () => {
    const de = cat({
      show: '{platform, select, macos {Im Finder zeigen} windows {Show in File Explorer} other {Im Dateimanager zeigen}}',
      showAria: '{platform, select, macos {{name} im Finder anzeigen} other {{name} im Dateimanager zeigen}}',
    })
    const out = checkLocale(source, de, false, source.messages)
    expect(out.map(({ ariaKey, platform }) => [ariaKey, platform])).toEqual([['showAria', 'macos']])
  })

  it('skips an OS where either half still carries its English, pending translation', () => {
    const de = cat({
      show: '{platform, select, macos {Im Finder zeigen} windows {Show in File Explorer} other {Show in file manager}}',
      showAria: '{platform, select, macos {{name} im Finder zeigen} other {{name} im Dateimanager zeigen}}',
    })
    expect(checkLocale(source, de, false, source.messages)).toEqual([])
  })
})

describe('runAriaCheck: the pseudolocale', () => {
  // `gen-pseudolocale.ts` pads every string on its own (`Ṗáüšé⟦áé⟧`), so a short
  // label's padding never appears inside the longer accessible name, and en-XA
  // would fail every pair by construction.
  let root: string
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-aria-'))
    const writeLocale = (locale: string, messages: Record<string, string>) => {
      mkdirSync(join(root, locale), { recursive: true })
      writeFileSync(join(root, locale, 'app.json'), JSON.stringify(messages))
    }
    writeLocale('en', { pause: 'Pause', pauseAria: 'Pause copying' })
    writeLocale('de', { pause: 'Pausieren', pauseAria: 'Pausieren und Kopieren anhalten' })
    writeLocale('en-XA', { pause: 'Ṗáüšé⟦áé⟧', pauseAria: 'Ṗáüšé çöṗýíñĝ⟦áéíóú⟧' })
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })
  const run = (only?: readonly string[]) => {
    const lines: string[] = []
    const code = runAriaCheck({ messagesRoot: root, write: (line) => void lines.push(line), only })
    return { code, text: lines.join('\n') }
  }

  it('leaves it out of a default run, which then agrees with CI (where en-XA never exists)', () => {
    const { code, text } = run()
    expect(code).toBe(EXIT_CLEAN)
    expect(text).toMatch(/de: clean\./)
    expect(text).not.toMatch(/en-XA/)
  })
  it('skips it even when named, saying why', () => {
    const { code, text } = run(['en-XA'])
    expect(code).toBe(EXIT_CLEAN)
    expect(text).toMatch(/en-XA: skipped/)
  })
})
