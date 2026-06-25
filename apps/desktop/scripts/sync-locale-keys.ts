#!/usr/bin/env node
/**
 * Locale KEY SYNC: bring an existing locale catalog into key-parity with `en`
 * after the English catalog gained or lost keys (the "add strings to an existing
 * feature, then translate to all languages" loop in docs/guides/i18n-translation.md).
 *
 * For each area file, per key:
 *  - en key present in the locale  → keep the locale's value + `@key` verbatim
 *    (existing translations and their `sourceHash` are never touched).
 *  - en key MISSING in the locale  → add it with the ENGLISH value + a fresh
 *    `@key.sourceHash`, i.e. an untranslated skeleton entry the translator then
 *    edits in place (the coverage check lists it as identical-to-English until then).
 *  - locale key NOT in en (orphan) → drop it (and its `@key`).
 * Output key order follows `en` (source order), so a renamed/reordered en file
 * propagates cleanly. New area files in `en` are created; orphan locale files are
 * left alone (rare; warned).
 *
 * Idempotent: re-running on an already-synced locale is a no-op diff. Unlike
 * gen-locale-skeleton.ts (which scaffolds a fresh locale and refuses to clobber),
 * this MERGES into a translated locale and preserves its work.
 *
 * Run: node scripts/sync-locale-keys.ts <tag> [<tag> …]   (omit tags = every non-en locale)
 * Pass `--messages-root <dir>` to point at a fixture.
 */

import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { isMetadataKey, listLocales, readLocaleFiles, resolveMessagesRoot, sourceHash } from './i18n-catalog-lib.ts'

const SOURCE_LOCALE = 'en'

/**
 * Merges ONE area file's keys: builds the synced `out` object (en order) and the
 * added/kept/dropped counts. Existing translations + their `@key` fields are kept
 * verbatim (only the `sourceHash` is re-stamped from the current English value);
 * en keys missing from the locale are added as English skeletons; locale keys no
 * longer in en are dropped (counted, not carried).
 * @param en a parsed `en/<area>.json`
 * @param existing the locale's current parsed `<area>.json` (`{}` if absent)
 */
function mergeAreaFile(
  en: Record<string, unknown>,
  existing: Record<string, unknown>,
): { out: Record<string, unknown>; added: number; kept: number; dropped: number } {
  const out: Record<string, unknown> = {}
  let added = 0
  let kept = 0
  let dropped = 0
  for (const [key, value] of Object.entries(en)) {
    if (isMetadataKey(key) || typeof value !== 'string') continue
    const metaKey = `@${key}`
    if (key in existing) {
      out[key] = existing[key]
      // Re-stamp the hash from the current English value so it can't drift; keep any other @key fields (e.g. reviewed).
      const existingMeta = typeof existing[metaKey] === 'object' && existing[metaKey] ? existing[metaKey] : {}
      out[metaKey] = { ...existingMeta, sourceHash: sourceHash(value) }
      kept++
    } else {
      out[key] = value
      out[metaKey] = { sourceHash: sourceHash(value) }
      added++
    }
  }
  // Count orphans (locale keys no longer in en) that are being dropped.
  for (const key of Object.keys(existing)) {
    if (isMetadataKey(key)) continue
    if (!(key in en)) dropped++
  }
  return { out, added, kept, dropped }
}

/**
 * Syncs ONE locale's catalog to `en`. Returns per-file counts of added/kept/dropped keys.
 */
export function syncLocale(
  tag: string,
  opts: { messagesRoot?: string } = {},
): { added: number; kept: number; dropped: number; files: number } {
  if (tag === SOURCE_LOCALE) throw new Error(`Refusing to sync the source locale '${SOURCE_LOCALE}'.`)
  const root = resolveMessagesRoot(opts.messagesRoot)
  const enFiles = readLocaleFiles(SOURCE_LOCALE, root)
  const localeDir = join(root, tag)
  let added = 0
  let kept = 0
  let dropped = 0
  let files = 0
  for (const name of Object.keys(enFiles).sort()) {
    const en = enFiles[name]
    const localePath = join(localeDir, name)
    const existing: Record<string, unknown> = existsSync(localePath)
      ? (JSON.parse(readFileSync(localePath, 'utf8')) as Record<string, unknown>)
      : {}
    const merged = mergeAreaFile(en, existing)
    added += merged.added
    kept += merged.kept
    dropped += merged.dropped
    writeFileSync(localePath, JSON.stringify(merged.out, null, 2) + '\n', 'utf8')
    files++
  }
  // Warn about locale area files with no matching en file (orphans we don't auto-delete).
  if (existsSync(localeDir)) {
    const enNames = new Set(Object.keys(enFiles))
    for (const f of readdirSync(localeDir)) {
      if (f.endsWith('.json') && !enNames.has(f))
        console.warn(`  warning: ${tag}/${f} has no matching en/ file (left in place)`)
    }
  }
  return { added, kept, dropped, files }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const argv = process.argv.slice(2)
  const rootFlag = argv.indexOf('--messages-root')
  const messagesRoot = rootFlag !== -1 ? argv[rootFlag + 1] : undefined
  let tags = argv.filter((a, i) => !a.startsWith('--') && i !== (rootFlag !== -1 ? rootFlag + 1 : -1))
  if (tags.length === 0) tags = listLocales(messagesRoot).filter((l) => l !== SOURCE_LOCALE)
  for (const tag of tags) {
    const { added, kept, dropped, files } = syncLocale(tag, { messagesRoot })
    console.log(
      `Synced ${tag}/: +${String(added)} new (English, to translate), ${String(kept)} kept, -${String(dropped)} dropped, across ${String(files)} files.`,
    )
  }
}
