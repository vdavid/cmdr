#!/usr/bin/env node
/**
 * PLURAL-CATEGORY COVERAGE check (i18n maintenance, M3) — ERROR class.
 *
 * Each language has its own set of CLDR plural categories: English needs `one`
 * and `other`; Polish needs `one`, `few`, `many`, and `other`; Japanese needs
 * only `other`. A `{count, plural, …}` message in a locale MUST provide a branch
 * for every category that LOCALE requires, or `count` values landing in a missing
 * category render the wrong branch (a grammatical error, or a throw). So this
 * FAILS the build (Go wrapper maps exit 1 → ERROR).
 *
 * The required set is data-driven, with NO bundled CLDR table:
 * `new Intl.PluralRules(locale).resolvedOptions().pluralCategories`. Two ICU
 * exemptions, both correct: `other` is always required and always present (ICU
 * mandates it); explicit `=N` literal branches (`=0`, `=1`) are exact-value
 * matches that sit ALONGSIDE the keyword categories, never substitute for them,
 * so they're ignored here (the catalog helper only surfaces keyword categories).
 *
 * `select` is NOT checked here — its categories are an arbitrary, message-defined
 * enumeration (covered by placeholder/tag parity, which requires the locale's
 * select branches to match English). Only `plural` args are CLDR-governed, which
 * is why the catalog helper keeps `pluralCategories` and `selectCategories`
 * separate. Raw `errors.*` keys carry no ICU plurals, so they contribute nothing.
 *
 * Run: `pnpm i18n:check-plural` (desktop) or `node scripts/i18n-check-plural.js`.
 * Pass `--messages-root <dir>` to point at a fixture (used by the tests).
 */

import { parseMessage } from './i18n-catalog-lib.js'
import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.js'

/**
 * The CLDR plural categories a locale requires, from the platform's `Intl`
 * (no bundled CLDR table). Cached per locale.
 * @type {Map<string, Set<string>>}
 */
const requiredCache = new Map()

/**
 * Returns the set of plural categories `locale` requires per CLDR.
 * @param {string} locale a BCP-47 tag
 * @returns {Set<string>}
 */
export function requiredPluralCategories(locale) {
  const cached = requiredCache.get(locale)
  if (cached) return cached
  const cats = new Set(new Intl.PluralRules(locale).resolvedOptions().pluralCategories)
  requiredCache.set(locale, cats)
  return cats
}

/**
 * Checks one locale message's plural args against the locale's required CLDR
 * categories and returns a short coverage detail, or `null` if every plural arg is
 * fully covered (and a parse-failure → null, since the ICU check owns that).
 * Exposed for unit tests.
 * @param {string} locale the BCP-47 tag (drives the required set)
 * @param {string} localeValue the locale's message value
 * @returns {string | null}
 */
export function pluralCoverageDetail(locale, localeValue) {
  const parsed = parseMessage(localeValue, locale)
  if (!parsed.ok || parsed.pluralCategories.size === 0) return null
  const required = requiredPluralCategories(locale)
  /** @type {string[]} */
  const parts = []
  for (const [arg, provided] of parsed.pluralCategories) {
    const missing = [...required].filter((c) => !provided.has(c)).sort()
    if (missing.length > 0) parts.push(`{${arg}} missing plural ${missing.length === 1 ? 'category' : 'categories'} ${missing.join(', ')}`)
  }
  return parts.length === 0 ? null : parts.join('; ')
}

/**
 * Runs the plural-coverage check over the catalogs under `messagesRoot`.
 * @param {object} [opts]
 * @param {string} [opts.messagesRoot] override the `messages/` root (for tests)
 * @param {(line: string) => void} [opts.write] output sink, one line at a time (for tests)
 * @returns {number}
 */
export function runPluralCheck(opts = {}) {
  return runLocaleCheck({
    title: 'Plural-category coverage',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    summaryLine: (count) => `${String(count)} plural message(s) missing a required CLDR category for this locale:`,
    inspectLocale: ({ locale, locale_catalog: localeCatalog, findings }) => {
      for (const [key, localeValue] of Object.entries(localeCatalog.messages)) {
        const detail = pluralCoverageDetail(locale, localeValue)
        if (detail !== null) findings.add(key, detail)
      }
    },
  })
}

// Run as a CLI (not when imported by tests).
if (import.meta.url === `file://${process.argv[1]}`) {
  const rootFlag = process.argv.indexOf('--messages-root')
  const messagesRoot = rootFlag !== -1 ? process.argv[rootFlag + 1] : undefined
  try {
    process.exit(runPluralCheck({ messagesRoot }))
  } catch (err) {
    console.error(`Couldn't run the plural-coverage check: ${err instanceof Error ? err.message : String(err)}`)
    process.exit(EXIT_ERROR)
  }
}
