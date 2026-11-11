/**
 * Pure logic for the native-strings codegen, factored out of the CLI
 * (`gen-native-strings.ts`) so it's unit-testable against in-memory catalogs.
 *
 * Rust draws a handful of user-facing strings that the webview can never own:
 * the native menu bar (built during `setup`), the main window's title, and the
 * "Cmdr is already running" alert (which fires before any window exists). They
 * live in the same message catalogs as every other string, so translators work
 * one pile and one set of checks covers everything; this generator lifts the
 * subset Rust needs into a table the crate compiles in.
 *
 * The subset is chosen by KEY PREFIX ([`NATIVE_KEY_PREFIXES`]), so adding a
 * native string is adding a catalog key under one of those prefixes and
 * regenerating. Nothing hand-maintains a second list.
 *
 * Values are emitted verbatim: `menu_t` is a raw lookup with no ICU, matching
 * `isRawKey()` in `i18n-catalog-lib.ts`, which classifies `menu.*` as raw
 * alongside `errors.*`. A platform block (`{platform, select, …}`) is split
 * here by the one splitter (`platform-select.ts`) into one text per OS, and Rust
 * picks the running OS's text at lookup time, so the generated file never needs
 * an OS `cfg`.
 */

import { parsePlatformSelect, PLATFORM_SELECT_PREFIX } from '../src/lib/intl/platform-select.ts'

/**
 * One native value, mirroring Rust's `NativeText`: the same text on every OS
 * (`All`), or one text per OS (`PerOs`), with a branch the block omits already
 * resolved to `other`, so Rust never has to know the folding rule.
 */
export type NativeText = { all: string } | { macos: string; windows: string; other: string }

/** One shipped locale's native strings, mirroring Rust's `LocaleStrings`. */
export interface LocaleNativeStrings {
  /** The catalog directory name, verbatim (`en`, `zh`). */
  tag: string
  /** `[key, value]` pairs, sorted by key so Rust can binary-search them. */
  entries: [string, NativeText][]
}

/**
 * A catalog value as Rust stores it. Throws, naming the key, for a value that
 * starts like a block but doesn't split: emitted as plain text, it would put raw
 * ICU on the menu bar.
 */
export function nativeTextOf(key: string, value: string): NativeText {
  const branches = parsePlatformSelect(value)
  if (branches === null) {
    if (value.startsWith(PLATFORM_SELECT_PREFIX)) {
      throw new Error(`${key} starts like a platform block but doesn't split as one: ${value}`)
    }
    return { all: value }
  }
  const { other } = branches
  return { macos: branches.macos ?? other, windows: branches.windows ?? other, other }
}

/**
 * Key prefixes whose catalog entries Rust reads directly. Closed and small on
 * purpose: every prefix names a surface the webview structurally cannot draw.
 *
 *  - `menu.` — the native menu bar, its submenus, and every native context menu.
 *  - `licensing.windowTitle.` — the main window's title bar, set from Rust
 *    before the frontend has loaded.
 *  - `main.instanceLock.` — the native alert refusing a second instance. It runs
 *    before the webview exists, so a frontend-supplied string could never reach
 *    it; this is the reason the lookup lives in Rust at all.
 *
 * ❌ Don't add a prefix for a string the webview could render itself: every key
 * here costs bundle weight in the frontend (which loads the whole catalog) AND
 * a row in the generated Rust table.
 */
export const NATIVE_KEY_PREFIXES: readonly string[] = Object.freeze([
  'menu.',
  'licensing.windowTitle.',
  'main.instanceLock.',
])

/**
 * The dev-only pseudolocale, excluded for two reasons: it's never a language
 * anyone reads, and it's gitignored + regenerated, so including it would make
 * this generated file differ between a fresh clone and a machine that ran
 * `pnpm i18n:pseudo` — permanent phantom drift for the freshness check.
 */
export const PSEUDO_LOCALE = 'en-XA'

/**
 * The catalog key holding the Finder menu item's name. The same text heads its settings card, so a
 * translator handles it once. ❗ The English value must equal `serviceName` in
 * `finder-extension/OpenInCmdr.swift`, which `NSPerformService` finds the service by.
 */
export const FINDER_MENU_KEY = 'settings.finderExtension.card'

/** Where the generator's block sits in `src-tauri/Info.plist`. */
const MENU_ITEM_START = '<!-- generated: pnpm intl:native-strings -->'
const MENU_ITEM_END = '<!-- end generated -->'

/** The name macOS gives a catalog tag's language in `.lproj` folders and service titles. */
export function appleLanguageName(tag: string): string {
  return tag === 'zh' ? 'zh-Hans' : tag
}

/**
 * The menu item's title per language, English first and the rest sorted. A locale whose title
 * matches English is left out: macOS falls back to English on its own, and an overlay catalog that
 * inherits the key would only add a duplicate. A locale with no such key yet is left out too.
 *
 * Throws for a platform block in any locale: the item exists only on macOS, and `Info.plist` and
 * `Localizable.strings` take one literal title, so a block would ship as raw ICU.
 */
export function finderMenuTitles(
  catalogs: Partial<Record<string, Partial<Record<string, string>>>>,
): { lproj: string; title: string }[] {
  for (const [tag, messages] of Object.entries(catalogs)) {
    if (messages?.[FINDER_MENU_KEY]?.startsWith(PLATFORM_SELECT_PREFIX) === true) {
      throw new Error(`${tag}'s ${FINDER_MENU_KEY} holds a platform block, but the Finder menu item is macOS-only`)
    }
  }
  const english = catalogs.en?.[FINDER_MENU_KEY]
  if (english === undefined) throw new Error(`The English catalog has no ${FINDER_MENU_KEY}`)
  const others = Object.entries(catalogs)
    .flatMap(([tag, messages]) => {
      const title = messages?.[FINDER_MENU_KEY]
      return tag === PSEUDO_LOCALE || title === undefined || title === english
        ? []
        : [{ lproj: appleLanguageName(tag), title }]
    })
    .sort((a, b) => (a.lproj < b.lproj ? -1 : a.lproj > b.lproj ? 1 : 0))
  return [{ lproj: 'en', title: english }, ...others]
}

/** One `Localizable.strings` file, keyed by the English title the Swift code asks for. */
export function emitLocalizableStrings(title: string, englishTitle = 'Open in Cmdr'): string {
  const quoted = (text: string) => `"${text.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`
  return `/* Generated by pnpm intl:native-strings. Never hand-edit. */\n${quoted(englishTitle)} = ${quoted(title)};\n`
}

function xmlEscaped(text: string): string {
  return text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')
}

/** `plist` with the service's `NSMenuItem` dictionary rewritten between its markers. */
export function replaceServiceMenuItem(plist: string, titles: { lproj: string; title: string }[]): string {
  const start = plist.indexOf(MENU_ITEM_START)
  const end = plist.indexOf(MENU_ITEM_END)
  if (start === -1 || end === -1) throw new Error('Info.plist has lost its generated NSMenuItem markers')
  const indent = ' '.repeat(16)
  const entry = (key: string, title: string) =>
    `${indent}<key>${key}</key>\n${indent}<string>${xmlEscaped(title)}</string>\n`
  const rows = titles.map(({ lproj, title }) => entry(lproj === 'en' ? 'default' : lproj, title)).join('')
  return `${plist.slice(0, start)}${MENU_ITEM_START}\n${rows}${indent}${plist.slice(end)}`
}

/** Whether a catalog key is one Rust reads. */
export function isNativeKey(key: string, prefixes: readonly string[] = NATIVE_KEY_PREFIXES): boolean {
  return prefixes.some((prefix) => key.startsWith(prefix))
}

/**
 * Picks the native subset out of one locale's merged messages, sorted by key.
 * @param messages the locale's renderable messages (metadata already stripped)
 * @param prefixes override for tests
 */
export function nativeEntriesOf(
  messages: Record<string, string>,
  prefixes: readonly string[] = NATIVE_KEY_PREFIXES,
): [string, NativeText][] {
  return Object.entries(messages)
    .filter(([key]) => isNativeKey(key, prefixes))
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([key, value]): [string, NativeText] => [key, nativeTextOf(key, value)])
}

/**
 * Builds the whole table: one entry per shipped catalog, sorted by tag, with the
 * pseudolocale dropped. A locale with no native keys yet still gets a row with an
 * empty list, so the table shape says which locales exist and a translation
 * landing shows up as rows filling in rather than a row appearing.
 * @param catalogs locale tag → that locale's renderable messages
 * @param prefixes override for tests
 */
export function buildNativeStrings(
  catalogs: Record<string, Record<string, string>>,
  prefixes: readonly string[] = NATIVE_KEY_PREFIXES,
): LocaleNativeStrings[] {
  return Object.entries(catalogs)
    .filter(([tag]) => tag !== PSEUDO_LOCALE)
    .map(([tag, messages]) => ({ tag, entries: nativeEntriesOf(messages, prefixes) }))
    .sort((a, b) => a.tag.localeCompare(b.tag))
}

/**
 * Escapes a catalog value into a Rust string literal body. Only the four
 * sequences that can break out of `"…"` are escaped; everything else (including
 * every non-ASCII character) is emitted verbatim, because the generated file is
 * UTF-8 Rust source and a menu label full of `\u{…}` would be unreadable in a
 * diff.
 */
export function rustStringLiteral(value: string): string {
  const escaped = value
    .replaceAll('\\', '\\\\')
    .replaceAll('"', '\\"')
    .replaceAll('\n', '\\n')
    .replaceAll('\r', '\\r')
    .replaceAll('\t', '\\t')
  return `"${escaped}"`
}

/** Renders one value as a `NativeText` literal (the variants are imported bare). */
function emitNativeText(text: NativeText): string {
  if ('all' in text) return `All(${rustStringLiteral(text.all)})`
  const { macos, windows, other } = text
  return `PerOs { macos: ${rustStringLiteral(macos)}, windows: ${rustStringLiteral(windows)}, other: ${rustStringLiteral(other)} }`
}

/**
 * The `use` line for the `NativeText` variants the table spells bare, naming only
 * those it uses: an unused import is a warning, and clippy denies warnings.
 */
function variantImport(locales: readonly LocaleNativeStrings[]): string {
  const texts = locales.flatMap((locale) => locale.entries.map(([, text]) => text))
  const used = [
    ...(texts.some((text) => 'all' in text) ? ['All'] : []),
    ...(texts.some((text) => !('all' in text)) ? ['PerOs'] : []),
  ]
  if (used.length === 0) return ''
  return `\nuse super::NativeText::${used.length === 1 ? used[0] : `{${used.join(', ')}}`};`
}

/** Renders one locale as a Rust struct literal. */
function emitLocale(locale: LocaleNativeStrings): string {
  const rows = locale.entries.map(
    ([key, value]) => `            (${rustStringLiteral(key)}, ${emitNativeText(value)}),`,
  )
  const entries = rows.length === 0 ? '&[]' : ['&[', ...rows, '        ]'].join('\n')
  return ['    LocaleStrings {', `        tag: "${locale.tag}",`, `        entries: ${entries},`, '    },'].join('\n')
}

/**
 * Emits the whole generated Rust module. The `LocaleStrings` struct and the
 * `NativeText` enum are hand-written in `intl/native_strings.rs`; only the data
 * lives here, so the doc comments explaining the lookup don't churn when a label
 * changes.
 * @param locales the table, already built and sorted
 */
export function emitRustModule(locales: readonly LocaleNativeStrings[]): string {
  return `//! The user-facing strings Rust draws itself, per shipped locale.
//!
//! @generated by \`apps/desktop/scripts/gen-native-strings.ts\` from the catalogs
//! under \`apps/desktop/src/lib/intl/messages/\`. DO NOT EDIT BY HAND: run
//! \`pnpm intl:native-strings\` from \`apps/desktop/\`, or just let the
//! \`native-strings-fresh\` check rewrite it on a local run.
//!
//! Covers the keys under ${NATIVE_KEY_PREFIXES.map((p) => `\`${p}\``).join(', ')} —
//! the native menu bar, the window title, and the already-running alert. The
//! \`${PSEUDO_LOCALE}\` pseudolocale is deliberately absent: it's regenerated and
//! gitignored, so including it would make this file differ between checkouts.
//! A platform block becomes \`PerOs\`; \`menu_t\` picks the running OS's text.

use super::LocaleStrings;${variantImport(locales)}

/// Every shipped locale's native strings, sorted by tag; each locale's entries
/// are sorted by key so [\`super::lookup\`] can binary-search them.
//
// \`rustfmt::skip\`: layout here is the GENERATOR's output, and the freshness
// check diffs that output byte for byte. Letting rustfmt rewrap the table would
// make the two disagree forever, one reformatting what the other regenerates.
#[rustfmt::skip]
pub(crate) const NATIVE_STRINGS: &[LocaleStrings] = &[
${locales.map(emitLocale).join('\n')}
];
`
}
