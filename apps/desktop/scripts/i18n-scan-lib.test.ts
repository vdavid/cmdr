/**
 * Tests for the typography scanner (`i18n-scan-lib.ts`): what a reader sees, as
 * text a mechanics rule can match, with every insert carrying its name and kind.
 */
import { describe, it, expect } from 'vitest'
import {
  CODE_MARK,
  INSERT_MARK,
  compileScanPattern,
  placeholderKind,
  scanValue,
  scanPatternError,
} from './i18n-scan-lib.ts'
import type { ScannedText } from './i18n-scan-lib.ts'

/** A variant as `text` with each insert spelled `{name:kind}` and each code mark as `` `…` ``. */
const shown = ({ text, inserts }: ScannedText): string => {
  let index = 0
  return Array.from(text)
    .map((char) => {
      if (char === CODE_MARK) return '`…`'
      if (char !== INSERT_MARK) return char
      const insert = inserts[index++]
      return `{${insert.name}:${insert.kind}}`
    })
    .join('')
}
const scan = (value: string, key = 'a.b', locale = 'en') => scanValue(key, value, locale).map(shown)
const hits = (pattern: string, value: string, key = 'a.b') => {
  const rule = compileScanPattern(pattern)
  return scanValue(key, value, 'en').some((variant) => rule.find(variant) !== undefined)
}

describe('scanValue', () => {
  it('names each placeholder and gives it a kind', () => {
    expect(scan('Copy {countText} files to {path}')).toEqual(['Copy {countText:number} files to {path:name}'])
    expect(scan('Open {systemSettings}')).toEqual(['Open {systemSettings:token}'])
    expect(scan('{size, number} left')).toEqual(['{size:number} left'])
  })

  it('reads a raw family literally, its tokens named', () => {
    expect(scan("Can't open {path} in {system_settings}", 'errors.a.message')).toEqual([
      "Can't open {path:name} in {system_settings:token}",
    ])
  })

  it('renders an empty tag as an insert Cmdr fills', () => {
    expect(scan('Press <key></key> to open')).toEqual(['Press {key:token} to open'])
    expect(scan('<size></size>/s')).toEqual(['{size:number}/s'])
  })

  it('renders a line-break tag as a line break', () => {
    expect(scan('I use Cmdr<break></break>for fun')).toEqual(['I use Cmdr\nfor fun'])
  })

  it('gives a code span its own mark, unless it holds nothing but one insert', () => {
    expect(scan('Run `ls -la` in `{path}`')).toEqual(['Run `…` in {path:name}'])
    expect(scan('Run `cd {path}` now')).toEqual(['Run `…` now'])
  })

  it('treats a tag around one symbol like code', () => {
    expect(scan('Prefix with <bang>!</bang> or <move>↑↓</move>, <b>bold</b>')).toEqual(['Prefix with `…` or `…`, bold'])
  })

  it('makes plural and select nodes transparent: each branch reads in place', () => {
    expect(scan('{count, plural, one {# fájl} other {# fájl}}ban, {kind, select, dir {mappa} other {fájl}}')).toEqual([
      '{count:number} fájlban, mappa',
      '{count:number} fájlban, fájl',
    ])
  })

  it('keeps every branch within the variant cap', () => {
    const many = Array.from({ length: 8 }, (_, i) => `{a${String(i)}, select, x {x} y {y} other {z}}`).join(' ')
    const variants = scan(many)
    expect(variants.length).toBeLessThanOrEqual(64)
    for (const branch of ['x', 'y', 'z']) expect(variants.some((v) => v.startsWith(branch))).toBe(true)
  })
})

describe('placeholderKind', () => {
  it('classifies counts, controlled tokens, and the uncontrolled rest', () => {
    expect(['count', 'countText', 'fileCount', 'total', 'percent', 'duration'].map(placeholderKind)).toEqual(
      Array(6).fill('number'),
    )
    expect(['system_settings', 'localNetwork', 'copyKey', 'verb', 'nextLabel'].map(placeholderKind)).toEqual(
      Array(5).fill('token'),
    )
    expect(['name', 'path', 'host', 'app', 'reason'].map(placeholderKind)).toEqual(Array(5).fill('name'))
  })
})

describe('compileScanPattern', () => {
  it('keeps a plain pattern plain: \\uFFFC is any insert', () => {
    expect(hits('\\uFFFC-\\p{Ll}', 'Open {path}-ban')).toBe(true)
    expect(hits('\\uFFFC-\\p{Ll}', 'Open `.git`-ben')).toBe(false)
  })

  it('targets an insert kind, a union of kinds, a placeholder by name, or a code span', () => {
    expect(hits('\\ba {@name}', 'Open a {path}')).toBe(true)
    expect(hits('\\ba {@name}', 'Open a {count}')).toBe(false)
    expect(hits('\\ba {@name}', 'Open a {localNetwork}')).toBe(false)
    expect(hits('\\ba {@number|token}', 'Open a {localNetwork}')).toBe(true)
    expect(hits('{@arg:path}-\\p{Ll}', 'Open {name}-ben and {path}-ban')).toBe(true)
    expect(hits('{@arg:path}-\\p{Ll}', 'Open {name}-ben')).toBe(false)
    expect(hits('{@number} %', '{count, plural, other {# %}}')).toBe(true)
    expect(hits('{@code}-\\p{Ll}', 'Open `.git`-ben')).toBe(true)
  })

  it('finds a later insert of the right kind after an earlier one of the wrong kind', () => {
    expect(hits('{@name}s\\b', '{count}s and {name}s')).toBe(true)
  })

  it('renders the hit with its placeholder names', () => {
    const rule = compileScanPattern('\\ba {@name}')
    const [variant] = scanValue('a.b', 'Open a {path} now', 'en')
    expect(rule.find(variant)).toBe('a {path}')
  })

  it('rejects an unknown macro', () => {
    expect(scanPatternError('{@colour}')).toMatch(/unknown insert kind "colour"/)
    expect(scanPatternError('{@name}x')).toBeUndefined()
    expect(scanPatternError('(')).toMatch(/doesn't compile/)
  })
})
