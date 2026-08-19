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
 * The rustfmt pass runs HERE rather than in the `package.json` script, because
 * the freshness check invokes this file directly: formatting anywhere else
 * would make the check's regenerate-and-diff report permanent phantom drift.
 */

import { writeFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import { join } from 'node:path'
import { listLocales } from './i18n-catalog-lib.ts'
import { buildShippedLocales, emitRustModule } from './gen-shipped-locales-lib.ts'

const desktopDir = join(import.meta.dirname, '..')
const outFile = join(desktopDir, 'src-tauri', 'src', 'intl', 'shipped_locales.gen.rs')

const entries = buildShippedLocales(listLocales())
writeFileSync(outFile, emitRustModule(entries))

// rustfmt picks up the workspace `rustfmt.toml` by walking up from the file, so
// no flags are needed. A missing rustfmt is fatal: silently skipping it would
// leave the file failing `cargo fmt --check` with no hint why.
const fmt = spawnSync('rustfmt', [outFile], { stdio: 'inherit' })
if (fmt.error || fmt.status !== 0) {
  console.error(`\nrustfmt failed on ${outFile}. Is the Rust toolchain installed?`)
  process.exit(1)
}

const withSplits = entries.filter((entry) => entry.regionScripts.length > 0).map((entry) => entry.tag)
console.log(
  `Wrote ${String(entries.length)} shipped locales to shipped_locales.gen.rs` +
    (withSplits.length > 0 ? ` (script splits: ${withSplits.join(', ')})` : ''),
)
