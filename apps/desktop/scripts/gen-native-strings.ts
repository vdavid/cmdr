#!/usr/bin/env node
/**
 * Generates `src-tauri/src/intl/native_strings.gen.rs`: the per-locale table of
 * the user-facing strings Rust draws itself, so the native menu bar, the window
 * title, and the already-running alert speak the same language as the app.
 *
 * Source of truth is the message catalogs under `src/lib/intl/messages/` — the
 * same files the frontend reads and translators work in. Which keys ride along
 * is decided by `NATIVE_KEY_PREFIXES`; pure logic lives in
 * `gen-native-strings-lib.ts` (unit-tested), this CLI does the filesystem I/O.
 *
 * Run via `pnpm intl:native-strings` from the desktop app dir, or through the
 * `native-strings-fresh` check in the pipeline. Never hand-edit the output.
 *
 * The emitted table carries `#[rustfmt::skip]`, so this script owns its layout
 * end to end and needs no Rust toolchain. Without that, rustfmt would rewrap the
 * table and the freshness check would report permanent phantom drift.
 */

import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { listLocales, loadCatalog } from './i18n-catalog-lib.ts'
import { buildNativeStrings, emitRustModule, NATIVE_KEY_PREFIXES } from './gen-native-strings-lib.ts'

const desktopDir = join(import.meta.dirname, '..')
const outFile = join(desktopDir, 'src-tauri', 'src', 'intl', 'native_strings.gen.rs')

const catalogs: Record<string, Record<string, string>> = {}
for (const locale of listLocales()) catalogs[locale] = loadCatalog(locale).messages

const table = buildNativeStrings(catalogs)
writeFileSync(outFile, emitRustModule(table))

const english = table.find((locale) => locale.tag === 'en')?.entries.length ?? 0
const translated = table.filter((locale) => locale.tag !== 'en' && locale.entries.length > 0).map((l) => l.tag)
console.log(
  `Wrote ${String(english)} native strings (${NATIVE_KEY_PREFIXES.join(', ')}) for ${String(table.length)} locales ` +
    `to native_strings.gen.rs` +
    (translated.length > 0 ? ` (translated: ${translated.join(', ')})` : ' (English only so far)'),
)
