/**
 * Tests for the platform-select shape check (`i18n-check-platform-select.ts`).
 *
 * The pure classifier gets one case per rule; the runner gets a temp catalog
 * root holding `en`, a full translation (`de`), and an overlay (`en-GB`), so the
 * branch-set comparison is exercised against the source each one resolves to.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { platformSelectIssues, runPlatformSelectCheck } from './i18n-check-platform-select.ts'
import { EXIT_CLEAN, EXIT_ISSUES } from './i18n-locale-check-lib.ts'

const issues = (key: string, value: string, sourceValue?: string) => platformSelectIssues({ key, value, sourceValue })

describe('platformSelectIssues: shape', () => {
  it('is clean for plain values, ICU and raw', () => {
    expect(issues('a.b', 'Hello {name}')).toEqual([])
    expect(issues('errors.x.y', "Open {system_settings}, here's <folder-path>")).toEqual([])
    expect(issues('a.b', '{side, select, left {Left} other {Right}}')).toEqual([])
  })

  it('is clean for every legal branch set, including equal macos and windows', () => {
    expect(issues('a.b', '{platform, select, macos {Finder} other {Files}}')).toEqual([])
    expect(issues('a.b', '{platform, select, windows {Explorer} other {Files}}')).toEqual([])
    expect(issues('a.b', '{platform, select, macos {Finder} windows {Explorer} other {Files}}')).toEqual([])
    expect(issues('a.b', '{platform, select, macos {Trash} windows {Trash} other {Bin}}')).toEqual([])
  })

  it('is clean for an ICU block with a nested plural and a quoted brace', () => {
    expect(
      issues(
        'a.b',
        "{platform, select, macos {{count, plural, one {# item} other {# items}} '{'x'}'} other {{count, plural, one {# file} other {# files}}}}",
      ),
    ).toEqual([])
  })

  it('flags non-canonical spelling', () => {
    expect(issues('a.b', '{platform,select,macos {A} other {B}}')).toEqual([
      expect.stringMatching(/canonical spelling/),
    ])
    expect(issues('a.b', '{platform, select, macos {A}  other {B}}')).toEqual([
      expect.stringMatching(/canonical spelling/),
    ])
  })

  it('flags branches out of canonical order', () => {
    expect(issues('a.b', '{platform, select, other {B} macos {A}}')).toEqual([
      expect.stringMatching(/canonical spelling/),
    ])
  })

  it('flags an unknown branch, a missing other, and a lone other', () => {
    expect(issues('a.b', '{platform, select, macos {A} linux {L} other {B}}')).toEqual([
      expect.stringMatching(/not a well-formed block/),
    ])
    expect(issues('a.b', '{platform, select, macos {A} windows {W}}')).toEqual([
      expect.stringMatching(/not a well-formed block/),
    ])
    expect(issues('a.b', '{platform, select, other {B}}')).toEqual([expect.stringMatching(/at least two branches/)])
  })

  it('flags a macos or windows branch equal to other', () => {
    expect(issues('a.b', '{platform, select, windows {Same} other {Same}}')).toEqual([
      expect.stringMatching(/windows branch equals other/),
    ])
    expect(issues('a.b', '{platform, select, macos {Same} windows {W} other {Same}}')).toEqual([
      expect.stringMatching(/macos branch equals other/),
    ])
  })

  it('flags a block nested inside a sentence and a {platform} interpolation', () => {
    expect(issues('a.b', 'Open {platform, select, macos {Finder} other {Files}} now')).toEqual([
      expect.stringMatching(/outside a whole-value block/),
    ])
    expect(issues('a.b', 'Running on {platform}')).toEqual([expect.stringMatching(/outside a whole-value block/)])
    expect(issues('errors.x.y', 'Running on {platform}')).toEqual([
      expect.stringMatching(/outside a whole-value block/),
    ])
  })

  it('flags a block nested inside a branch', () => {
    expect(issues('a.b', '{platform, select, macos {A} other {{platform, select, windows {W} other {O}}}}')).toEqual([
      expect.stringMatching(/outside a whole-value block/),
    ])
  })
})

describe('platformSelectIssues: raw bodies', () => {
  it('is clean for a raw block with literal tokens, markdown, and <…>', () => {
    expect(
      issues(
        'errors.x.y',
        "{platform, select, macos {Open {system_settings}, here's `<folder-path>`} other {Run **it**}}",
      ),
    ).toEqual([])
  })

  it("flags a ' directly before a brace or angle bracket in a raw body", () => {
    for (const body of ["l'{name}", "x'}", "l'<folder>", "a'>"]) {
      const value = `{platform, select, macos {${body} m} other {plain}}`
      expect(issues('errors.x.y', value)).toContainEqual(expect.stringMatching(/apostrophe directly before/))
    }
  })

  it('does not apply the apostrophe rule to an ICU block, where quoting is ICU grammar', () => {
    expect(issues('a.b', "{platform, select, macos {Use '{'braces'}'} other {Plain}}")).toEqual([])
  })

  it('flags unbalanced braces in a raw body', () => {
    // The opening brace of `{oops` swallows the rest, so the block no longer closes.
    expect(issues('errors.x.y', '{platform, select, macos {Open {oops} other {Files}}')).toEqual([
      expect.stringMatching(/not a well-formed block/),
    ])
  })
})

describe('platformSelectIssues: keys that must stay plain', () => {
  it('flags a block in a manifest key and in the Finder menu key', () => {
    expect(issues('settings.appearance.language.label', '{platform, select, macos {A} other {B}}')).toEqual([
      expect.stringMatching(/must stay a plain value/),
    ])
    expect(issues('settings.finderExtension.card', '{platform, select, macos {A} other {B}}')).toEqual([
      expect.stringMatching(/must stay a plain value/),
    ])
  })
})

describe('platformSelectIssues: branch set against the source', () => {
  const en = '{platform, select, macos {Finder} windows {File Explorer} other {Files}}'

  it('is clean for the same branch set, English carried in the untranslated branches', () => {
    expect(issues('a.b', '{platform, select, macos {Im Finder} windows {File Explorer} other {Files}}', en)).toEqual([])
  })

  it('flags a missing branch, an extra branch, and a plain value beside a block', () => {
    expect(issues('a.b', '{platform, select, macos {Im Finder} other {Files}}', en)).toEqual([
      expect.stringMatching(/branches \{macos, other\}, but its source has \{macos, windows, other\}/),
    ])
    expect(issues('a.b', en, '{platform, select, macos {Finder} other {Files}}')).toEqual([
      expect.stringMatching(/branches \{macos, windows, other\}, but its source has \{macos, other\}/),
    ])
    expect(issues('a.b', 'Im Finder', en)).toEqual([expect.stringMatching(/plain value, but its source is a block/)])
    expect(issues('a.b', en, 'Finder')).toEqual([expect.stringMatching(/block, but its source is a plain value/)])
  })
})

describe('runPlatformSelectCheck (temp catalogs)', () => {
  let root: string
  const block = '{platform, select, macos {Show in Finder} windows {Show in File Explorer} other {Show in Files}}'

  const writeLocale = (locale: string, messages: Record<string, unknown>) => {
    mkdirSync(join(root, locale), { recursive: true })
    writeFileSync(join(root, locale, 'fixture.json'), JSON.stringify(messages, null, 2) + '\n', 'utf8')
  }
  const run = () => {
    const lines: string[] = []
    const code = runPlatformSelectCheck({ messagesRoot: root, write: (l) => void lines.push(l) })
    return { code, text: lines.join('\n') }
  }

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-platform-select-'))
    writeLocale('en', { 'fixture.show': block, 'fixture.plain': 'Cancel' })
    writeLocale('de', {
      'fixture.show':
        '{platform, select, macos {Im Finder zeigen} windows {Show in File Explorer} other {Show in Files}}',
      '@fixture.show': { sourceHash: 'abcdef0' },
      'fixture.plain': 'Abbrechen',
    })
    writeLocale('en-GB', {
      'fixture.show':
        '{platform, select, macos {Show in Finder} windows {Show in File Explorer} other {Reveal in Files}}',
    })
  })
  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  it('is clean for a migrated key in every locale kind', () => {
    const { code, text } = run()
    expect(code).toBe(EXIT_CLEAN)
    expect(text).toMatch(/all locales clean/)
  })

  it('flags a locale whose branch set drifted, and only that key', () => {
    writeLocale('de', { 'fixture.show': '{platform, select, macos {Im Finder zeigen} other {Show in Files}}' })
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/fixture\.show → .*source has \{macos, windows, other\}/)
    expect(text.match(/^ {2}- /gm)?.length).toBe(1)
  })

  it('checks en itself for shape', () => {
    writeLocale('en', { 'fixture.show': '{platform,select,macos {A} other {B}}', 'fixture.plain': 'Cancel' })
    const { code, text } = run()
    expect(code).toBe(EXIT_ISSUES)
    expect(text).toMatch(/en[,:(].*\n(.*\n)*.*fixture\.show → .*canonical spelling/)
  })
})
