#!/usr/bin/env node
/**
 * Generates `src-tauri/src/intl/shipped_locales.gen.rs`: the table of catalogs
 * Cmdr ships, the scripts their readers read, and CLDR's parent-locale
 * overrides, so the Rust locale resolver can walk a preference tag home without
 * carrying CLDR data of its own.
 *
 * Two sources. The catalogs come from the directories under
 * `src/lib/intl/messages/` (via `listLocales()`, which already excludes the
 * `screenshots/` sibling), and their script facts from Node's
 * `Intl.Locale(...).maximize()`. The parent-locale overrides come from the
 * `cldr-core` package, which is CLDR's own JSON distribution: `Intl` exposes
 * likely subtags but not parents, so this is the only way to learn that
 * `en-NZ`'s parent is `en-001`. Pure logic lives in
 * `gen-shipped-locales-lib.ts` (unit-tested); this CLI does the filesystem I/O.
 *
 * Run via `pnpm intl:shipped-locales` from the desktop app dir, or through the
 * `shipped-locales-fresh` check in the pipeline. Never hand-edit the output.
 *
 * The emitted tables carry `#[rustfmt::skip]`, so this script owns its layout
 * end to end and needs no Rust toolchain. Without that, rustfmt would rewrap the
 * tables and the freshness check would report permanent phantom drift, each tool
 * undoing the other.
 */

import { createRequire } from 'node:module'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { listLocales } from './i18n-catalog-lib.ts'
import { buildParentLocales, buildShippedLocales, emitRustModule } from './gen-shipped-locales-lib.ts'

const desktopDir = join(import.meta.dirname, '..')
const outFile = join(desktopDir, 'src-tauri', 'src', 'intl', 'shipped_locales.gen.rs')

/** The one corner of `parentLocales.json` we read. */
interface ParentLocalesFile {
  supplemental: { parentLocales: { parentLocale: Record<string, string> } }
}

/**
 * CLDR's parent-locale map, straight out of the package.
 *
 * `createRequire().resolve` rather than a JSON import: it finds the package
 * through normal resolution from this file, so the path survives pnpm's
 * symlinked store layout without hardcoding a `node_modules` prefix.
 *
 * The shape is checked rather than asserted, so a CLDR release that moves this
 * key fails here with a sentence instead of silently emitting an empty table
 * and dropping every regional English back onto US English.
 */
function readParentLocales(): Record<string, string> {
  const file = createRequire(import.meta.url).resolve('cldr-core/supplemental/parentLocales.json')
  const parsed: unknown = JSON.parse(readFileSync(file, 'utf8'))
  const parentLocale = (parsed as Partial<ParentLocalesFile>).supplemental?.parentLocales.parentLocale
  if (parentLocale === undefined) {
    throw new Error(`${file} has no supplemental.parentLocales.parentLocale; did cldr-core change shape?`)
  }
  return parentLocale
}

const locales = listLocales()
const entries = buildShippedLocales(locales)
const parentLocales = buildParentLocales(readParentLocales(), locales)
writeFileSync(outFile, emitRustModule(entries, parentLocales))

const withSplits = entries.filter((entry) => entry.regionScripts.length > 0).map((entry) => entry.tag)
console.log(
  `Wrote ${String(entries.length)} shipped locales and ${String(parentLocales.length)} CLDR parent overrides ` +
    `to shipped_locales.gen.rs` +
    (withSplits.length > 0 ? ` (script splits: ${withSplits.join(', ')})` : ''),
)
