#!/usr/bin/env node
/**
 * PLURAL-CATEGORY COVERAGE check (i18n maintenance, M3): ERROR class.
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
 * Gated on the ENGLISH source's own plural shape, which encodes the value domain
 * the message can ever see. A few English plurals are DELIBERATELY degenerate
 * (`{count, plural, other {…}}` with no `one`) because the count is constrained
 * by construction (e.g. `fileExplorer.network.reconnect.times` only renders for
 * three-or-more; one/two have their own words). When English uses only `other`,
 * `count` is never 1, so a translation matching that shape is correct and we
 * require only the categories English engaged. When English uses the full plural
 * (`one`/`other`, the normal case), the whole quantitative range is in play, so a
 * richer-CLDR locale (Polish: few/many) must cover its full required set. Without
 * this gate the check would flag English's own deliberate partial plurals.
 *
 * `select` is NOT checked here: its categories are an arbitrary, message-defined
 * enumeration (covered by placeholder/tag parity, which requires the locale's
 * select branches to match English). Only `plural` args are CLDR-governed, which
 * is why the catalog helper keeps `pluralCategories` and `selectCategories`
 * separate. Raw `errors.*` keys carry no ICU plurals, so they contribute nothing.
 *
 * Run: `pnpm i18n:check-plural` (desktop) or `node scripts/i18n-check-plural.ts`.
 * Pass `--messages-root <dir>` to point at a fixture (used by the tests).
 */

import { parseMessage } from './i18n-catalog-lib.ts'
import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.ts'

/**
 * The CLDR plural categories a locale requires, from the platform's `Intl`
 * (no bundled CLDR table). Cached per locale.
 */
const requiredCache = new Map<string, Set<string>>()

/**
 * Returns the set of plural categories `locale` requires per CLDR.
 * @param locale a BCP-47 tag
 */
export function requiredPluralCategories(locale: string): Set<string> {
  const cached = requiredCache.get(locale)
  if (cached) return cached
  const cats = new Set(new Intl.PluralRules(locale).resolvedOptions().pluralCategories)
  requiredCache.set(locale, cats)
  return cats
}

/**
 * Whether an English plural arg engaged the FULL plural (used a keyword category
 * beyond `other`), meaning the whole quantitative range is in play. A degenerate
 * `other`-only English plural restricts the value domain, so its translations need
 * only `other`.
 * @param englishCats the categories English used for this arg
 */
function englishUsesFullPlural(englishCats: Set<string>): boolean {
  return [...englishCats].some((c) => c !== 'other')
}

/**
 * Checks one locale message's plural args against the categories required for this
 * locale, gated on the English source's own plural shape (see the file header).
 * Returns a short coverage detail, or `null` if every plural arg is covered (a
 * parse failure → null, since the ICU check owns that). Exposed for unit tests.
 * @param locale the BCP-47 tag (drives the locale's CLDR set)
 * @param localeValue the locale's message value
 * @param englishPlurals English's plural categories per arg for this key (from
 *   `parseMessage(englishValue).pluralCategories`). Absent → treat English as
 *   engaging the full plural (the strict default for callers that don't supply it).
 */
export function pluralCoverageDetail(
  locale: string,
  localeValue: string,
  englishPlurals?: Map<string, Set<string>>,
): string | null {
  const parsed = parseMessage(localeValue, locale)
  if (!parsed.ok || parsed.pluralCategories.size === 0) return null
  const localeCldr = requiredPluralCategories(locale)
  const parts: string[] = []
  for (const [arg, provided] of parsed.pluralCategories) {
    const englishCats = englishPlurals?.get(arg)
    // If English engaged the full plural (or we have no English reference), require
    // the locale's full CLDR set. If English was degenerate (`other`-only), require
    // only what English engaged.
    const required = englishCats && !englishUsesFullPlural(englishCats) ? englishCats : localeCldr
    const missing = [...required].filter((c) => !provided.has(c)).sort()
    if (missing.length > 0) {
      parts.push(`{${arg}} missing plural ${missing.length === 1 ? 'category' : 'categories'} ${missing.join(', ')}`)
    }
  }
  return parts.length === 0 ? null : parts.join('; ')
}

/** Options for `runPluralCheck`. */
interface RunPluralCheckOptions {
  /** override the `messages/` root (for tests) */
  messagesRoot?: string
  /** output sink, one line at a time (for tests) */
  write?: (line: string) => void
}

/**
 * Runs the plural-coverage check over the catalogs under `messagesRoot`.
 */
export function runPluralCheck(opts: RunPluralCheckOptions = {}): number {
  return runLocaleCheck({
    title: 'Plural-category coverage',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    summaryLine: (count) => `${String(count)} plural message(s) missing a required CLDR category for this locale:`,
    inspectLocale: ({ locale, base, locale_catalog: localeCatalog, findings }) => {
      for (const [key, localeValue] of Object.entries(localeCatalog.messages)) {
        const englishValue = base.messages[key]
        // Parse English to learn its plural shape (degenerate vs full); absent en
        // key → leave undefined so the strict default applies. The record index is
        // `string` to the types, but undefined at runtime when the key is absent.
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
        const englishPlurals = englishValue === undefined ? undefined : parseMessage(englishValue).pluralCategories
        const detail = pluralCoverageDetail(locale, localeValue, englishPlurals)
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
