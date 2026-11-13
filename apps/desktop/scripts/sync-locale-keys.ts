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
 * `--restamp` does reach an overlay (`restampOverlay`): a fork's hash is the hash
 * of the value it overrides (`es` for `es-419`), which nobody should compute by hand.
 *
 * ## Termbase exceptions follow deleted keys out
 *
 * A locale's `docs/i18n/<tag>/terms.json` `exceptions` name catalog keys, and the
 * termbase check fails on one `en` no longer has. So a sync also drops each
 * exception naming a deleted key (`pruneTermbaseExceptions`), then runs oxfmt on
 * the termbases it rewrote. A retired key otherwise failed every locale that had
 * ruled on it.
 *
 * Run: node scripts/sync-locale-keys.ts <tag> [<tag> …]   (omit tags = every non-en locale)
 * Pass `--messages-root <dir>` to point at a fixture, `--restamp <key>` to refresh
 * one key's stored hash across the synced locales and the overlays' forks. A tag
 * that isn't a locale dir is refused.
 */

import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import {
  BASE_LOCALE,
  isMetadataKey,
  layerCatalogs,
  listLocales,
  loadCatalog,
  readLocaleFiles,
  resolveLocaleSource,
  resolveMessagesRoot,
  sourceHash,
} from './i18n-catalog-lib.ts'
import { termsPath } from './i18n-termbase-lib.ts'

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
export function restampMeta(meta: Record<string, unknown>, currentHash: string): Record<string, unknown> {
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

/**
 * Drops every `exceptions` entry in a locale's termbase that names a key `en` no
 * longer has, and an `exceptions` map left empty. Rewrites the file only when
 * something went, so an untouched termbase keeps its formatting byte for byte.
 * `dropped` lists `concept: key` pairs, sorted.
 */
export function pruneTermbaseExceptions(
  tag: string,
  opts: { messagesRoot?: string; docsRoot?: string } = {},
): { dropped: string[]; changed: boolean } {
  const path = termsPath(tag, opts.docsRoot)
  if (!existsSync(path)) return { dropped: [], changed: false }
  const english = loadCatalog(BASE_LOCALE, resolveMessagesRoot(opts.messagesRoot)).messages
  const termbase = JSON.parse(readFileSync(path, 'utf8')) as Record<string, Record<string, unknown>>
  const dropped: string[] = []
  for (const [id, term] of Object.entries(termbase)) {
    const exceptions = term.exceptions
    if (typeof exceptions !== 'object' || exceptions === null) continue
    const kept = Object.entries(exceptions as Record<string, unknown>).filter(([key]) => {
      if (key in english) return true
      dropped.push(`${id}: ${key}`)
      return false
    })
    if (kept.length === Object.keys(exceptions).length) continue
    if (kept.length > 0) term.exceptions = Object.fromEntries(kept)
    else delete term.exceptions
  }
  if (dropped.length === 0) return { dropped, changed: false }
  writeFileSync(path, JSON.stringify(termbase, null, 2) + '\n', 'utf8')
  return { dropped: dropped.sort(), changed: true }
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
 * Restamps an OVERLAY's forks: for each named key the overlay carries, the stored
 * hash becomes the hash of the value it overrides (the parent's, `es` for
 * `es-419`, falling back to `en` where the parent lacks the key, as the runtime
 * does), and the source-bound vouches go. That's the source the stale check holds
 * an overlay to. Never adds, drops, or reorders a key: an overlay carries only its
 * forks, so a key it doesn't fork is simply not there to stamp.
 */
export function restampOverlay(
  tag: string,
  opts: { messagesRoot?: string; restampKeys: Iterable<string> },
): { restampedKeys: string[] } {
  const root = resolveMessagesRoot(opts.messagesRoot)
  const { overrides, isOverlay } = resolveLocaleSource(tag, listLocales(root))
  if (!isOverlay) throw new Error(`${tag} isn't an overlay; sync it with syncLocale instead.`)
  const source = layerCatalogs(loadCatalog(BASE_LOCALE, root), loadCatalog(overrides, root)).messages
  const restampKeys = new Set(opts.restampKeys)
  const restampedKeys: string[] = []
  for (const [name, file] of Object.entries(readLocaleFiles(tag, root))) {
    let changed = false
    for (const key of Object.keys(file)) {
      const metaKey = `@${key}`
      if (isMetadataKey(key) || !restampKeys.has(key) || !(key in source)) continue
      const meta = (typeof file[metaKey] === 'object' && file[metaKey] !== null ? file[metaKey] : {}) as Record<
        string,
        unknown
      >
      const currentHash = sourceHash(source[key])
      if (meta['sourceHash'] === currentHash) continue
      file[metaKey] = restampMeta(meta, currentHash)
      restampedKeys.push(key)
      changed = true
    }
    if (changed) writeFileSync(join(root, tag, name), JSON.stringify(file, null, 2) + '\n', 'utf8')
  }
  return { restampedKeys: restampedKeys.sort() }
}

/**
 * The tags a sync run should actually touch: every non-base locale (or the ones
 * asked for), with the OVERLAY catalogs split out, since they must never be
 * key-synced (see the file header); `--restamp` still reaches them through
 * `restampOverlay`. Reports each skip through `note` so a run is never silently
 * partial.
 * Takes one object, not three positionals: `requested` and `available` are both
 * lists of tags and would be trivially swappable.
 * @param opts.requested explicit tags, or an empty list to sweep every non-base locale
 * @param opts.available every locale dir present (`listLocales`)
 * @param opts.note sink for one skip message per overlay
 * @throws when a requested tag is no locale dir: a quoted `"a.b --restamp c.d"`
 *   arrives as one positional and would otherwise reach the file system as a path
 */
export function syncableLocales(opts: {
  requested: readonly string[]
  available: readonly string[]
  note: (line: string) => void
}): { tags: string[]; overlays: string[] } {
  const { requested, available, note } = opts
  const unknown = requested.filter((tag) => !available.includes(tag))
  if (unknown.length > 0) {
    const looksLikeKeys = unknown.some((tag) => tag.includes('.') || /\s/.test(tag))
    const hint = looksLikeKeys ? ' Keys go after --restamp, one flag per key: --restamp a.b --restamp c.d.' : ''
    throw new Error(`Not a locale: ${unknown.join(', ')} (have: ${available.join(', ')}).${hint}`)
  }
  const tags = requested.length > 0 ? requested : available.filter((tag) => tag !== BASE_LOCALE)
  const overlays = tags.filter((tag) => resolveLocaleSource(tag, available).isOverlay)
  for (const tag of overlays) {
    note(
      `Skipped ${tag}/: it's an overlay, so it carries only the keys it forks (docs/guides/i18n.md § Overlay catalogs).`,
    )
  }
  return { tags: tags.filter((tag) => !overlays.includes(tag)), overlays }
}

/** The requested tags split into syncable ones and overlays, or exits with the reason. */
function resolveTags(requested: string[], messagesRoot: string | undefined): { tags: string[]; overlays: string[] } {
  try {
    return syncableLocales({
      requested,
      available: listLocales(messagesRoot),
      note: (line) => {
        console.log(line)
      },
    })
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(2)
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const args = parseSyncArgs(process.argv.slice(2))
  const { messagesRoot, restampKeys } = args
  const { tags, overlays } = resolveTags(args.tags, messagesRoot)
  const restampedAnywhere = new Set<string>()
  if (restampKeys.length > 0) {
    for (const tag of overlays) {
      const { restampedKeys: done } = restampOverlay(tag, { messagesRoot, restampKeys })
      for (const key of done) restampedAnywhere.add(key)
      if (done.length > 0) console.log(`Restamped ${tag}/ against the value each fork overrides: ${done.join(', ')}.`)
    }
  }
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
  const rewrittenTermbases: string[] = []
  // A fixture `--messages-root` would judge the real termbases against a fixture `en`.
  for (const tag of messagesRoot === undefined ? tags : []) {
    const { dropped, changed } = pruneTermbaseExceptions(tag)
    if (!changed) continue
    rewrittenTermbases.push(termsPath(tag))
    console.log(`Dropped ${tag} termbase exceptions naming deleted keys: ${dropped.join(', ')}.`)
  }
  if (rewrittenTermbases.length > 0) {
    // JSON.stringify expands what oxfmt keeps inline; restore the house shape.
    const desktopDir = join(dirname(fileURLToPath(import.meta.url)), '..')
    const fmt = spawnSync('pnpm', ['exec', 'oxfmt', ...rewrittenTermbases], { cwd: desktopDir, stdio: 'inherit' })
    if (fmt.status !== 0) console.warn('  warning: oxfmt did not exit cleanly; run `pnpm exec oxfmt` on the termbases.')
  }
  for (const key of restampKeys) {
    if (!restampedAnywhere.has(key)) {
      console.warn(`  warning: nothing to restamp for '${key}' (no locale had it stale: misspelled, or already fresh)`)
    }
  }
}
