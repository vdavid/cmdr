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
 * ## Why a kept key's `sourceHash` is NEVER refreshed here
 *
 * The stored hash records which English value a translation was made from, and
 * the stale check (`i18n-check-stale.ts`) is the only thing that tells a
 * translator "this locale owes you a re-translation" after a copy edit. Refreshing
 * it on sync would clear that warning without anyone having re-translated, which
 * is exactly the silent drift the `sourceHash` mechanism exists to prevent. So a
 * routine sync moves KEYS, never the translation state attached to them.
 *
 * The legitimate "English changed but the translation is still accurate" case is
 * an explicit, human-judged act: `--restamp <key>` (repeatable). It refreshes that
 * key's hash and DROPS `reviewed` / `sameAsSourceJustification`, both of which
 * vouched for the OLD English value and have to be re-earned.
 *
 * Idempotent: re-running on an already-synced locale is a no-op diff. Unlike
 * gen-locale-skeleton.ts (which scaffolds a fresh locale and refuses to clobber),
 * this MERGES into a translated locale and preserves its work.
 *
 * ## OVERLAY locales are skipped
 *
 * A regional variant whose language base ships (`en-GB`, `pt-PT`) carries ONLY
 * the keys it forks, so "bring it into key-parity with `en`" is precisely the
 * wrong thing to do: it would balloon a 60-key overlay into a full clone, and
 * `desktop-i18n-coverage` would then flag every cloned key as dead weight. So
 * `syncableLocales` filters them out of the sweep AND out of an explicit tag
 * list, with a note. A new key needs no overlay work: the variant inherits the
 * base language's translation. See `docs/guides/i18n.md` § Overlay catalogs.
 *
 * Run: node scripts/sync-locale-keys.ts <tag> [<tag> …]   (omit tags = every non-en locale)
 * Pass `--messages-root <dir>` to point at a fixture, `--restamp <key>` to refresh
 * one key's stored hash across the synced locales.
 */

import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { parseArgs } from 'node:util'
import {
  BASE_LOCALE,
  isMetadataKey,
  listLocales,
  readLocaleFiles,
  resolveLocaleSource,
  resolveMessagesRoot,
  sourceHash,
} from './i18n-catalog-lib.ts'

/**
 * `@key` fields that vouch for one specific English value and so can't outlive it:
 * a restamp drops them, and the stale check flags them while they're still attached
 * to a stale key. See `messages/DETAILS.md` § `@key` metadata schema.
 */
const SOURCE_BOUND_META: ReadonlySet<string> = new Set(['reviewed', 'sameAsSourceJustification'])

/**
 * Restamps one `@key` block: the hash becomes `currentHash` and every source-bound
 * vouch is dropped. Rebuilt rather than mutated so field order stays stable.
 */
function restampMeta(meta: Record<string, unknown>, currentHash: string): Record<string, unknown> {
  const kept = Object.entries(meta).filter(([field]) => !SOURCE_BOUND_META.has(field))
  return { ...Object.fromEntries(kept), sourceHash: currentHash }
}

/**
 * Merges ONE area file's keys: builds the synced `out` object (en order) and the
 * added/kept/dropped/restamped counts. An existing translation is carried over with
 * its `@key` block untouched; en keys missing from the locale are added as English
 * skeletons; locale keys no longer in en are dropped (counted, not carried).
 *
 * A key named in `restampKeys` is the exception: its stored hash is refreshed from
 * the current English and its source-bound metadata dropped. That only counts as a
 * restamp when the hash actually moved, so a typo'd or already-fresh key reports
 * honestly instead of looking like it did something.
 * @param en a parsed `en/<area>.json`
 * @param existing the locale's current parsed `<area>.json` (`{}` if absent)
 * @param restampKeys message keys whose stored hash the caller deliberately refreshes
 */
function mergeAreaFile(
  en: Record<string, unknown>,
  existing: Record<string, unknown>,
  restampKeys: ReadonlySet<string>,
): { out: Record<string, unknown>; added: number; kept: number; dropped: number; restamped: string[] } {
  const out: Record<string, unknown> = {}
  let added = 0
  let kept = 0
  let dropped = 0
  const restamped: string[] = []
  for (const [key, value] of Object.entries(en)) {
    if (isMetadataKey(key) || typeof value !== 'string') continue
    const metaKey = `@${key}`
    if (key in existing) {
      out[key] = existing[key]
      const existingMeta: Record<string, unknown> =
        typeof existing[metaKey] === 'object' && existing[metaKey] !== null
          ? { ...(existing[metaKey] as Record<string, unknown>) }
          : {}
      const currentHash = sourceHash(value)
      const shouldRestamp = restampKeys.has(key) && existingMeta['sourceHash'] !== currentHash
      if (shouldRestamp) restamped.push(key)
      const meta = shouldRestamp ? restampMeta(existingMeta, currentHash) : existingMeta
      // No empty `@key: {}` blocks: a key that never carried metadata keeps carrying
      // none, and the stale check calls out the missing hash rather than sync inventing one.
      if (Object.keys(meta).length > 0) out[metaKey] = meta
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
  return { out, added, kept, dropped, restamped }
}

/** Options for `syncLocale`. */
export interface SyncLocaleOptions {
  /** override the `messages/` root (for tests) */
  messagesRoot?: string
  /** message keys whose stored `sourceHash` to deliberately refresh (see the module docblock) */
  restampKeys?: Iterable<string>
}

/** What one locale's sync did. `restampedKeys` lists only keys whose hash actually moved. */
export interface SyncLocaleResult {
  added: number
  kept: number
  dropped: number
  restamped: number
  restampedKeys: string[]
  files: number
}

/**
 * Syncs ONE locale's catalog to `en`: key parity in `en` order, translations and
 * their `@key` blocks preserved, plus any deliberate `restampKeys` refresh.
 */
export function syncLocale(tag: string, opts: SyncLocaleOptions = {}): SyncLocaleResult {
  if (tag === BASE_LOCALE) throw new Error(`Refusing to sync the source locale '${BASE_LOCALE}'.`)
  const root = resolveMessagesRoot(opts.messagesRoot)
  const enFiles = readLocaleFiles(BASE_LOCALE, root)
  const localeDir = join(root, tag)
  const restampKeys = new Set(opts.restampKeys ?? [])
  let added = 0
  let kept = 0
  let dropped = 0
  let files = 0
  const restampedKeys = new Set<string>()
  for (const name of Object.keys(enFiles).sort()) {
    const en = enFiles[name]
    const localePath = join(localeDir, name)
    const existing: Record<string, unknown> = existsSync(localePath)
      ? (JSON.parse(readFileSync(localePath, 'utf8')) as Record<string, unknown>)
      : {}
    const merged = mergeAreaFile(en, existing, restampKeys)
    added += merged.added
    kept += merged.kept
    dropped += merged.dropped
    for (const key of merged.restamped) restampedKeys.add(key)
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
  return { added, kept, dropped, restamped: restampedKeys.size, restampedKeys: [...restampedKeys].sort(), files }
}

/** The parsed CLI arguments. */
export interface SyncArgs {
  tags: string[]
  messagesRoot: string | undefined
  restampKeys: string[]
}

/**
 * Parses the CLI arguments: positional locale tags, `--messages-root <dir>`, and a
 * repeatable `--restamp <key>`.
 * @param argv `process.argv.slice(2)`
 */
export function parseSyncArgs(argv: string[]): SyncArgs {
  const { values, positionals } = parseArgs({
    args: argv,
    options: {
      'messages-root': { type: 'string' },
      restamp: { type: 'string', multiple: true },
    },
    allowPositionals: true,
  })
  return { tags: positionals, messagesRoot: values['messages-root'], restampKeys: values.restamp ?? [] }
}

/**
 * The tags a sync run should actually touch: every non-base locale (or the ones
 * asked for), minus the OVERLAY catalogs, which must never be key-synced (see the
 * file header). Reports each skip through `note` so a run is never silently
 * partial.
 * Takes one object, not three positionals: `requested` and `available` are both
 * lists of tags and would be trivially swappable.
 * @param opts.requested explicit tags, or an empty list to sweep every non-base locale
 * @param opts.available every locale dir present (`listLocales`)
 * @param opts.note sink for one skip message per overlay
 */
export function syncableLocales(opts: {
  requested: readonly string[]
  available: readonly string[]
  note: (line: string) => void
}): string[] {
  const { requested, available, note } = opts
  const tags = requested.length > 0 ? requested : available.filter((tag) => tag !== BASE_LOCALE)
  return tags.filter((tag) => {
    if (!resolveLocaleSource(tag, available).isOverlay) return true
    note(
      `Skipped ${tag}/: it's an overlay, so it carries only the keys it forks (docs/guides/i18n.md § Overlay catalogs).`,
    )
    return false
  })
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const args = parseSyncArgs(process.argv.slice(2))
  const { messagesRoot, restampKeys } = args
  const tags = syncableLocales({
    requested: args.tags,
    available: listLocales(messagesRoot),
    note: (line) => {
      console.log(line)
    },
  })
  const restampedAnywhere = new Set<string>()
  for (const tag of tags) {
    const {
      added,
      kept,
      dropped,
      restamped,
      restampedKeys: done,
      files,
    } = syncLocale(tag, { messagesRoot, restampKeys })
    for (const key of done) restampedAnywhere.add(key)
    const restampNote = restamped > 0 ? `, ${String(restamped)} restamped` : ''
    console.log(
      `Synced ${tag}/: +${String(added)} new (English, to translate), ${String(kept)} kept, -${String(dropped)} dropped${restampNote}, across ${String(files)} files.`,
    )
  }
  for (const key of restampKeys) {
    if (!restampedAnywhere.has(key)) {
      console.warn(`  warning: nothing to restamp for '${key}' (no locale had it stale: misspelled, or already fresh)`)
    }
  }
}
