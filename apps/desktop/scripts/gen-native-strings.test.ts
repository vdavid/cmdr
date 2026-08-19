/**
 * TDD for the native-strings codegen (`gen-native-strings-lib.ts`): message
 * catalogs → the Rust table the native menu bar, the window title, and the
 * already-running alert read.
 *
 * The properties that matter are the ones the Rust side depends on: only native
 * keys ride along, entries are sorted (Rust binary-searches them), the
 * pseudolocale never appears (it's gitignored, so including it would make the
 * generated file differ between checkouts), and values survive escaping intact.
 */
import { describe, it, expect } from 'vitest'
import {
  buildNativeStrings,
  emitRustModule,
  isNativeKey,
  nativeEntriesOf,
  PSEUDO_LOCALE,
  rustStringLiteral,
} from './gen-native-strings-lib.ts'

describe('isNativeKey', () => {
  it('takes the menu family and the two pre-webview strings', () => {
    expect(isNativeKey('menu.file.open')).toBe(true)
    expect(isNativeKey('licensing.windowTitle.personalUse')).toBe(true)
    expect(isNativeKey('main.instanceLock.alertTitle')).toBe(true)
  })

  it('leaves every webview-owned key behind', () => {
    // The frontend loads the whole catalog; only what Rust draws belongs in the
    // Rust table, or the table becomes a second copy of the catalog.
    expect(isNativeKey('settings.appearance.title')).toBe(false)
    expect(isNativeKey('licensing.about.version')).toBe(false)
    expect(isNativeKey('main.quit.title')).toBe(false)
  })
})

describe('nativeEntriesOf', () => {
  it('sorts by key, because Rust binary-searches the result', () => {
    const entries = nativeEntriesOf({
      'menu.view.zoom': 'Zoom',
      'menu.file.open': 'Open',
      'settings.x.y': 'ignored',
      'menu.bar.file': 'File',
    })
    expect(entries).toEqual([
      ['menu.bar.file', 'File'],
      ['menu.file.open', 'Open'],
      ['menu.view.zoom', 'Zoom'],
    ])
  })
})

describe('buildNativeStrings', () => {
  it('drops the pseudolocale so the generated file matches a fresh clone', () => {
    const table = buildNativeStrings({
      en: { 'menu.file.open': 'Open' },
      [PSEUDO_LOCALE]: { 'menu.file.open': '[Öpéñ]' },
    })
    expect(table.map((l) => l.tag)).toEqual(['en'])
  })

  it('keeps a locale with no native keys yet, as an empty row', () => {
    // The nine locales exist before their menu translations do. An empty row
    // says "this locale ships, its menu is still English", which is the truth.
    const table = buildNativeStrings({ en: { 'menu.file.open': 'Open' }, hu: { 'settings.x.y': 'Valami' } })
    expect(table).toEqual([
      { tag: 'en', entries: [['menu.file.open', 'Open']] },
      { tag: 'hu', entries: [] },
    ])
  })

  it('sorts locales by tag so the file is stable across filesystem orderings', () => {
    const table = buildNativeStrings({ sv: {}, de: {}, en: {} })
    expect(table.map((l) => l.tag)).toEqual(['de', 'en', 'sv'])
  })
})

describe('rustStringLiteral', () => {
  it('escapes only what can break out of a Rust string', () => {
    expect(rustStringLiteral('Copy "{name}"')).toBe('"Copy \\"{name}\\""')
    expect(rustStringLiteral('back\\slash')).toBe('"back\\\\slash"')
  })

  it('keeps non-ASCII verbatim, so a translated label stays readable in the diff', () => {
    expect(rustStringLiteral('Beenden…')).toBe('"Beenden…"')
    expect(rustStringLiteral('打开')).toBe('"打开"')
  })
})

describe('emitRustModule', () => {
  it('emits a compilable table with the skip attribute the freshness check needs', () => {
    const rust = emitRustModule(buildNativeStrings({ en: { 'menu.file.open': 'Open' } }))
    expect(rust).toContain('#[rustfmt::skip]')
    expect(rust).toContain('pub(crate) const NATIVE_STRINGS: &[LocaleStrings] = &[')
    expect(rust).toContain('tag: "en",')
    expect(rust).toContain('("menu.file.open", "Open"),')
    expect(rust).toContain('@generated')
  })

  it('writes an empty slice rather than an empty block for a locale with no entries', () => {
    const rust = emitRustModule(buildNativeStrings({ hu: {} }))
    expect(rust).toContain('entries: &[],')
  })
})
