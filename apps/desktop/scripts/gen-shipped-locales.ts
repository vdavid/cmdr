#!/usr/bin/env node
/**
 * Generates `src-tauri/src/intl/shipped_locales.gen.rs`: the table of catalogs
 * Cmdr ships and the scripts their readers read, so the Rust locale resolver
 * can refuse to cross a script boundary without carrying CLDR data of its own.
 *
 * Source of truth is the catalog directories under `src/lib/intl/messages/`
 * (via `listLocales()`, which already excludes the `screenshots/` sibling); the
 * script facts come from Node's `Intl.Locale(...).maximize()`. Pure logic lives
 * in `gen-shipped-locales-lib.ts` (unit-tested); this CLI does the filesystem
 * I/O and the rustfmt pass.
 *
 * Run via `pnpm intl:shipped-locales` from the desktop app dir, or through the
 * `shipped-locales-fresh` check in the pipeline. Never hand-edit the output.
 *
 * The emitted table carries `#[rustfmt::skip]`, so this script owns its layout
 * end to end and needs no Rust toolchain. Without that, rustfmt would rewrap the
 * table and the freshness check would report permanent phantom drift, each tool
 * undoing the other.
 */

import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { listLocales } from './i18n-catalog-lib.ts'
import { buildShippedLocales, emitRustModule } from './gen-shipped-locales-lib.ts'

const desktopDir = join(import.meta.dirname, '..')
const outFile = join(desktopDir, 'src-tauri', 'src', 'intl', 'shipped_locales.gen.rs')

const entries = buildShippedLocales(listLocales())
writeFileSync(outFile, emitRustModule(entries))

const withSplits = entries.filter((entry) => entry.regionScripts.length > 0).map((entry) => entry.tag)
console.log(
  `Wrote ${String(entries.length)} shipped locales to shipped_locales.gen.rs` +
    (withSplits.length > 0 ? ` (script splits: ${withSplits.join(', ')})` : ''),
)
