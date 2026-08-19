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
 * alongside `errors.*`.
 */

/** One shipped locale's native strings, mirroring Rust's `LocaleStrings`. */
export interface LocaleNativeStrings {
  /** The catalog directory name, verbatim (`en`, `zh`). */
  tag: string
  /** `[key, value]` pairs, sorted by key so Rust can binary-search them. */
  entries: [string, string][]
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
): [string, string][] {
  return Object.entries(messages)
    .filter(([key]) => isNativeKey(key, prefixes))
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
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

/** Renders one locale as a Rust struct literal. */
function emitLocale(locale: LocaleNativeStrings): string {
  const rows = locale.entries.map(
    ([key, value]) => `            (${rustStringLiteral(key)}, ${rustStringLiteral(value)}),`,
  )
  const entries = rows.length === 0 ? '&[]' : ['&[', ...rows, '        ]'].join('\n')
  return ['    LocaleStrings {', `        tag: "${locale.tag}",`, `        entries: ${entries},`, '    },'].join('\n')
}

/**
 * Emits the whole generated Rust module. The `LocaleStrings` struct is
 * hand-written in `intl/native_strings.rs`; only the data lives here, so the
 * doc comments explaining the lookup don't churn when a label changes.
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

use super::LocaleStrings;

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
