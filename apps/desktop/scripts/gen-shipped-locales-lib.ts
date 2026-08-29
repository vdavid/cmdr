/**
 * Pure logic for the shipped-locale codegen, factored out of the CLI
 * (`gen-shipped-locales.ts`) so it's unit-testable against in-memory inputs.
 *
 * The Rust resolver (`src-tauri/src/intl/`) picks the app's UI language from
 * the user's ordered macOS preference list, and it refuses to cross a script
 * boundary: a Traditional-Chinese reader must not land on the Simplified `zh`
 * catalog. Deciding that needs CLDR likely-subtags data, which the webview gets
 * free from `Intl.Locale.maximize()` and Rust does not. So we ask Node's `Intl`
 * here, at build time, and emit the answer as a Rust table.
 *
 * The `likelyScript` we ask with is the SAME function the message runtime and the
 * i18n checks use (`src/lib/intl/locale-inheritance.ts`), so all three layers
 * answer "can this reader read that catalog?" identically, and Rust inherits that
 * answer through this table.
 *
 * Two facts per shipped catalog:
 *  - the script its readers read (`zh` → `Hans`, everything Latin → `Latn`), and
 *  - the regions whose likely script DIFFERS from the language's default
 *    (`zh` → `TW`/`HK`/`MO`/… → `Hant`; for every Latin-script language, empty).
 *
 * Everything is emitted lowercase, because the resolver lowercases the tags it
 * compares (macOS reports `zh-Hant-TW`, a POSIX-ish path reports `zh_HANT_tw`).
 */

import { likelyScript } from '../src/lib/intl/locale-inheritance.ts'
import { baseLanguageOf } from './i18n-catalog-lib.ts'

/** One shipped catalog's script facts, mirroring Rust's `ShippedLocale`. */
export interface ShippedLocaleEntry {
  /**
   * The catalog directory name, verbatim. The resolver hands this straight back
   * to the frontend, which keys its catalog map on the directory name, so the
   * spelling has to survive the round trip (comparisons are case-insensitive).
   */
  tag: string
  /** The likely script of the catalog's own tag: what its readers read. */
  script: string
  /** The likely script of the bare language subtag, with no region attached. */
  defaultScript: string
  /** Regions of that language whose likely script differs from `defaultScript`. */
  regionScripts: { region: string; script: string }[]
}

/**
 * The dev-only pseudolocale. It's a build artifact of `gen-pseudolocale.ts`
 * (accented, inflated English for overflow testing), never a language anyone
 * reads, so auto-selection must never be able to reach it. Excluding it here
 * rather than in Rust makes that structural: the resolver only ever sees the
 * table, so a locale absent from the table cannot be auto-selected.
 */
export const PSEUDO_LOCALE = 'en-XA'

/**
 * Every region subtag worth probing: the 676 two-letter combinations (a
 * superset of ISO 3166-1 alpha-2) plus the 1000 three-digit UN M49 codes.
 * Enumerated rather than listed because CLDR's region set drifts with every
 * ICU update, and a hand-kept list would quietly stop covering new codes.
 * Unknown codes maximize to the language default and so contribute nothing.
 */
function allRegionSubtags(): string[] {
  const regions: string[] = []
  for (let first = 65; first <= 90; first++) {
    for (let second = 65; second <= 90; second++) {
      regions.push(String.fromCharCode(first) + String.fromCharCode(second))
    }
  }
  for (let code = 0; code < 1000; code++) regions.push(String(code).padStart(3, '0'))
  return regions
}

/**
 * Builds the script facts for one catalog tag by asking `Intl` for CLDR's
 * likely subtags, probing every region subtag against the tag's base language.
 * @param tag a catalog directory name (`zh`, `pt-BR`)
 */
export function buildEntry(tag: string): ShippedLocaleEntry {
  const language = baseLanguageOf(tag)
  const defaultScript = likelyScript(language)
  const regionScripts: { region: string; script: string }[] = []
  for (const region of allRegionSubtags()) {
    const script = likelyScript(`${language}-${region}`)
    if (script !== '' && script !== defaultScript) regionScripts.push({ region: region.toLowerCase(), script })
  }
  return { tag, script: likelyScript(tag), defaultScript, regionScripts }
}

/**
 * Builds the table for every shipped catalog, dropping the pseudolocale and
 * sorting by tag so the generated file is stable across filesystem orderings.
 * @param locales catalog directory names (from `listLocales()`)
 */
export function buildShippedLocales(locales: readonly string[]): ShippedLocaleEntry[] {
  return locales
    .filter((locale) => locale !== PSEUDO_LOCALE)
    .map(buildEntry)
    .sort((a, b) => a.tag.localeCompare(b.tag))
}

/** Renders one entry as a Rust struct literal. */
function emitEntry(entry: ShippedLocaleEntry): string {
  const regions =
    entry.regionScripts.length === 0
      ? '&[]'
      : ['&[', ...entry.regionScripts.map((r) => `            ("${r.region}", "${r.script}"),`), '        ]'].join('\n')
  return [
    '    ShippedLocale {',
    `        tag: "${entry.tag}",`,
    `        script: "${entry.script}",`,
    `        default_script: "${entry.defaultScript}",`,
    `        region_scripts: ${regions},`,
    '    },',
  ].join('\n')
}

/**
 * Emits the whole generated Rust module. The `ShippedLocale` struct itself is
 * hand-written in `intl/mod.rs`; only the data lives here, so the doc comments
 * that explain the fields don't churn every time a catalog is added.
 * @param entries the table, already built and sorted
 */
export function emitRustModule(entries: readonly ShippedLocaleEntry[]): string {
  return `//! The message catalogs Cmdr ships, and the scripts their readers read.
//!
//! @generated by \`apps/desktop/scripts/gen-shipped-locales.ts\` from the catalog
//! directories under \`apps/desktop/src/lib/intl/messages/\`. DO NOT EDIT BY HAND:
//! run \`pnpm intl:shipped-locales\` from \`apps/desktop/\`, or just let the
//! \`shipped-locales-fresh\` check rewrite it on a local run.
//!
//! The \`${PSEUDO_LOCALE}\` pseudolocale is deliberately absent: auto-selection draws
//! only from this table, so leaving it out is what makes it unreachable.

use super::ShippedLocale;

/// Every catalog we ship, sorted by tag. See [\`ShippedLocale\`] for the fields.
//
// \`rustfmt::skip\`: layout here is the GENERATOR's output, and the freshness
// check diffs that output byte for byte. Letting rustfmt rewrap the table would
// make the two disagree forever, one reformatting what the other regenerates.
#[rustfmt::skip]
pub(crate) const SHIPPED_LOCALES: &[ShippedLocale] = &[
${entries.map(emitEntry).join('\n')}
];
`
}
