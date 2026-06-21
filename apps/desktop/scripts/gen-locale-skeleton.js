#!/usr/bin/env node
/**
 * Locale SKELETON generator: scaffolds a new translation locale ready for an
 * agent (or human) to translate in place.
 *
 * For each `messages/en/<area>.json` key it writes, into
 * `messages/<tag>/<area>.json`, the ENGLISH value verbatim plus a
 * `@key.sourceHash` of that English value. The result is a structurally-valid,
 * checks-passing locale whose values are still English — so a translator EDITS
 * each value in place (never recomputing the hash, which the script already got
 * right). Because the value starts byte-identical to English, the coverage check
 * (`i18n:check-coverage`) lists exactly the keys still untranslated, which is the
 * honest progress signal during a translation pass.
 *
 * This is the reference implementation of step 2 of "Add a new language" in
 * `docs/guides/i18n-translation.md` (mirror en's keys, stamp each sourceHash).
 * It deliberately reuses the same `sourceHash()` and catalog helpers as the
 * pseudolocale generator and the stale check, so the hashes agree.
 *
 * Run: `node scripts/gen-locale-skeleton.js <tag> [<tag> …]`
 *   e.g. `node scripts/gen-locale-skeleton.js de fr es`
 * Pass `--messages-root <dir>` to point at a fixture.
 *
 * Idempotent for keys: re-running overwrites each area file from `en` again, so
 * run it ONLY on a fresh locale (it would clobber existing translations). It
 * refuses to overwrite a non-empty existing locale dir unless `--force`.
 */

import { existsSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { isMetadataKey, readLocaleFiles, resolveMessagesRoot, sourceHash } from './i18n-catalog-lib.js'

const SOURCE_LOCALE = 'en'

/**
 * Builds the skeleton content for ONE area file: every English message paired
 * with its `@key.sourceHash`, in source order, exactly the interleaved
 * `key` / `@key` shape the pseudolocale and real locales use — but with the
 * English value left in place for the translator to overwrite.
 * @param {Record<string, unknown>} rawEnFile a parsed `en/<area>.json`
 * @returns {Record<string, unknown>}
 */
export function buildSkeletonFile(rawEnFile) {
  /** @type {Record<string, unknown>} */
  const out = {}
  for (const [key, value] of Object.entries(rawEnFile)) {
    if (isMetadataKey(key) || typeof value !== 'string') continue
    out[key] = value
    out[`@${key}`] = { sourceHash: sourceHash(value) }
  }
  return out
}

/**
 * Generates the full `<tag>/` skeleton from `en/`, one file per area.
 * @param {string} tag the BCP-47 locale tag (catalog dir name)
 * @param {object} [opts]
 * @param {string} [opts.messagesRoot] override the `messages/` root (for tests)
 * @param {boolean} [opts.force] overwrite a non-empty existing locale dir
 * @returns {{ files: number, keys: number }}
 */
export function generateSkeleton(tag, opts = {}) {
  if (tag === SOURCE_LOCALE) throw new Error(`Refusing to scaffold the source locale '${SOURCE_LOCALE}'.`)
  const root = resolveMessagesRoot(opts.messagesRoot)
  const outDir = join(root, tag)
  if (!opts.force && existsSync(outDir) && readdirSync(outDir).some((n) => n.endsWith('.json'))) {
    throw new Error(`'${tag}/' already has catalog files; refusing to clobber. Pass --force to overwrite.`)
  }
  const enFiles = readLocaleFiles(SOURCE_LOCALE, root)
  mkdirSync(outDir, { recursive: true })
  let keys = 0
  let files = 0
  for (const name of Object.keys(enFiles).sort()) {
    const skeleton = buildSkeletonFile(enFiles[name])
    writeFileSync(join(outDir, name), JSON.stringify(skeleton, null, 2) + '\n', 'utf8')
    files++
    keys += Object.keys(skeleton).filter((k) => !isMetadataKey(k)).length
  }
  return { files, keys }
}

// Run as a CLI (not when imported by tests).
if (import.meta.url === `file://${process.argv[1]}`) {
  const argv = process.argv.slice(2)
  const force = argv.includes('--force')
  const rootFlag = argv.indexOf('--messages-root')
  const messagesRoot = rootFlag !== -1 ? argv[rootFlag + 1] : undefined
  const tags = argv.filter((a, i) => !a.startsWith('--') && i !== (rootFlag !== -1 ? rootFlag + 1 : -1))
  if (tags.length === 0) {
    console.error('Usage: node scripts/gen-locale-skeleton.js <tag> [<tag> …] [--force] [--messages-root <dir>]')
    process.exit(1)
  }
  for (const tag of tags) {
    const { files, keys } = generateSkeleton(tag, { messagesRoot, force })
    console.log(
      `Scaffolded ${tag}/: ${String(keys)} keys across ${String(files)} area files (English values, ready to translate in place).`,
    )
  }
}
