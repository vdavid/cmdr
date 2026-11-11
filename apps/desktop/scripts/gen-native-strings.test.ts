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
  appleLanguageName,
  emitLocalizableStrings,
  finderMenuTitles,
  replaceServiceMenuItem,
  buildNativeStrings,
  emitRustModule,
  isNativeKey,
  nativeEntriesOf,
  nativeTextOf,
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
      ['menu.bar.file', { all: 'File' }],
      ['menu.file.open', { all: 'Open' }],
      ['menu.view.zoom', { all: 'Zoom' }],
    ])
  })
})

describe('nativeTextOf', () => {
  it('reads a plain value as one text for every OS', () => {
    expect(nativeTextOf('menu.file.open', 'Open')).toEqual({ all: 'Open' })
  })

  it('reads a full block as one text per OS', () => {
    const value =
      '{platform, select, macos {Show in Finder} windows {Show in File Explorer} other {Show in file manager}}'
    expect(nativeTextOf('menu.file.show', value)).toEqual({
      macos: 'Show in Finder',
      windows: 'Show in File Explorer',
      other: 'Show in file manager',
    })
  })

  it('gives macOS the `other` text when the block has no `macos` branch', () => {
    expect(nativeTextOf('menu.x', '{platform, select, windows {Exit} other {Quit}}')).toEqual({
      macos: 'Quit',
      windows: 'Exit',
      other: 'Quit',
    })
  })

  it('gives Windows the `other` text when the block has no `windows` branch', () => {
    expect(nativeTextOf('menu.x', '{platform, select, macos {Get Info} other {Properties}}')).toEqual({
      macos: 'Get Info',
      windows: 'Properties',
      other: 'Properties',
    })
  })

  it('keeps a `windows` branch that repeats the `macos` one, so Windows never reads Linux words by omission', () => {
    expect(nativeTextOf('menu.x', '{platform, select, macos {Trash} windows {Trash} other {Bin}}')).toEqual({
      macos: 'Trash',
      windows: 'Trash',
      other: 'Bin',
    })
  })

  it('keeps raw tokens and apostrophes inside a branch verbatim', () => {
    expect(nativeTextOf('menu.x', "{platform, select, macos {Eject ({name})} other {Unmount ({name}) it's}}")).toEqual({
      macos: 'Eject ({name})',
      windows: "Unmount ({name}) it's",
      other: "Unmount ({name}) it's",
    })
  })

  it('refuses a value that starts like a block but is not one, naming the key', () => {
    // A malformed block emitted as plain text would put raw ICU on the menu bar.
    expect(() => nativeTextOf('menu.broken', '{platform, select, macos {A} windows {B}}')).toThrow(/menu\.broken/)
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
      { tag: 'en', entries: [['menu.file.open', { all: 'Open' }]] },
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
    expect(rust).toContain('("menu.file.open", All("Open")),')
    expect(rust).toContain('@generated')
  })

  it('emits a block as one escaped text per OS, with no OS cfg in the file', () => {
    const rust = emitRustModule(
      buildNativeStrings({
        en: { 'menu.file.show': '{platform, select, macos {Show in "Finder"} other {Show in file manager}}' },
      }),
    )
    expect(rust).toContain(
      '("menu.file.show", PerOs { macos: "Show in \\"Finder\\"", windows: "Show in file manager", other: "Show in file manager" }),',
    )
    // The OS answer comes from the `intl/platform/` seam; a cfg here would sit outside it.
    expect(rust).not.toMatch(/cfg/)
  })

  it('imports only the variants the table uses, since clippy denies an unused import', () => {
    const block = '{platform, select, macos {A} other {B}}'
    expect(emitRustModule(buildNativeStrings({ en: { 'menu.a': 'A' } }))).toContain('use super::NativeText::All;\n')
    expect(emitRustModule(buildNativeStrings({ en: { 'menu.a': block } }))).toContain('use super::NativeText::PerOs;\n')
    expect(emitRustModule(buildNativeStrings({ en: { 'menu.a': 'A', 'menu.b': block } }))).toContain(
      'use super::NativeText::{All, PerOs};\n',
    )
    expect(emitRustModule(buildNativeStrings({ hu: {} }))).not.toContain('NativeText')
  })

  it('writes an empty slice rather than an empty block for a locale with no entries', () => {
    const rust = emitRustModule(buildNativeStrings({ hu: {} }))
    expect(rust).toContain('entries: &[],')
  })
})

describe('the Finder menu item', () => {
  const key = 'settings.finderExtension.card'

  it('names Simplified Chinese the way macOS does', () => {
    expect(appleLanguageName('zh')).toBe('zh-Hans')
    expect(appleLanguageName('zh-Hant')).toBe('zh-Hant')
    expect(appleLanguageName('de')).toBe('de')
  })

  it('puts English first and leaves out what falls back to it anyway', () => {
    const titles = finderMenuTitles({
      sv: { [key]: 'Öppna i Cmdr' },
      en: { [key]: 'Open in Cmdr' },
      nl: { [key]: 'Open in Cmdr' },
      de: { [key]: 'In Cmdr öffnen' },
      'en-XA': { [key]: 'Ōpēn' },
      fr: {},
    })
    expect(titles).toEqual([
      { lproj: 'en', title: 'Open in Cmdr' },
      { lproj: 'de', title: 'In Cmdr öffnen' },
      { lproj: 'sv', title: 'Öppna i Cmdr' },
    ])
  })

  it('refuses to generate from a catalog with no English title', () => {
    expect(() => finderMenuTitles({ de: { [key]: 'x' } })).toThrow()
  })

  it('refuses a platform block in any locale, since the Finder menu item exists only on macOS', () => {
    // `Info.plist` and `Localizable.strings` take one literal title; a block there would ship as raw ICU.
    const block = '{platform, select, macos {Open in Cmdr} other {Open with Cmdr}}'
    expect(() => finderMenuTitles({ en: { [key]: block } })).toThrow(/macOS-only/)
    expect(() => finderMenuTitles({ en: { [key]: 'Open in Cmdr' }, de: { [key]: block } })).toThrow(/de/)
  })

  it('keys each strings file by the English title and escapes its value', () => {
    expect(emitLocalizableStrings('In Cmdr "öffnen"')).toContain('"Open in Cmdr" = "In Cmdr \\"öffnen\\"";')
  })

  it('rewrites only the block between its markers, the same way every time', () => {
    const plist = 'a <!-- generated: pnpm intl:native-strings -->\nold\n<!-- end generated --> b'
    const once = replaceServiceMenuItem(plist, [
      { lproj: 'en', title: 'Open in Cmdr' },
      { lproj: 'de', title: 'A & B' },
    ])
    expect(once).toContain('<key>default</key>')
    expect(once).toContain('<key>de</key>')
    expect(once).toContain('<string>A &amp; B</string>')
    expect(once).not.toContain('old')
    expect(once.endsWith('<!-- end generated --> b')).toBe(true)
    expect(
      replaceServiceMenuItem(once, [
        { lproj: 'en', title: 'Open in Cmdr' },
        { lproj: 'de', title: 'A & B' },
      ]),
    ).toBe(once)
  })

  it('throws when the markers are gone, so a bad merge can not ship an English-only menu', () => {
    expect(() => replaceServiceMenuItem('no markers', [])).toThrow()
  })
})
