#!/usr/bin/env node
/**
 * TRANSLATION COVERAGE check (i18n maintenance): ERROR class.
 *
 * Honest-coverage signal so a "100% translated" claim can be trusted (mirrors the
 * screenshot coverage report's "say what's covered, list what isn't" stance). It
 * reads a locale by KIND, and the two kinds have opposite rules.
 *
 * ## A FULL TRANSLATION (`de`, `hu`, and `en-XA`) must cover every English key
 *
 *  - MISSING: an English key with no entry in the locale. The runtime silently
 *    renders English, so the gap is invisible without this check.
 *  - IDENTICAL: a locale value byte-identical to English. Usually means
 *    untranslated (copied through), though a few keys legitimately match (a bare
 *    brand token, a symbol), which is what the justification below is for.
 *  - SOURCE-TEXT-ONLY: a value that isn't byte-identical, yet shows the reader
 *    nothing but English, because the ONLY thing it changed is which plural/select
 *    categories it branches on (`showsOnlySourceText`). Portuguese requires a
 *    `many` category English doesn't have, so a verbatim-English Portuguese counter
 *    (`one {dir} many {dirs} other {dirs}}`) has a CORRECT branch set and English
 *    branch TEXT. Byte comparison alone reads that as translated, which is how
 *    English counters shipped inside a locale this check called fully covered.
 *
 * A key that legitimately stays identical (a brand name, a unit symbol, a
 * placeholder-only string, or a word the locale genuinely shares with English) is
 * EXEMPTED from the IDENTICAL signal by recording a non-empty
 * `@key.sameAsSourceJustification` on it in the locale catalog: the translator's
 * one-line reason it's deliberately identical. Present + non-empty → not a
 * finding. The exemption only suppresses IDENTICAL, never MISSING (a justification
 * can't excuse an absent key). See `messages/DETAILS.md` § `@key` schema and
 * `docs/guides/i18n-translation.md` § Deliberately-identical strings. The stale
 * check invalidates the justification once the English source changes (its
 * `sourceHash` stops matching), so it can't silently outlive the text it vouched
 * for.
 *
 * ## An OVERLAY (`en-GB` over `en`, `pt-PT` over `pt`) must carry ONLY its forks
 *
 * A variant whose language base also ships is an overlay (`resolveLocaleSource`):
 * it holds the handful of keys that genuinely differ, and the runtime resolves
 * the rest through locale → language base → `en`. So the rules invert:
 *
 *  - A key ABSENT from an overlay is correct and expected, never a finding.
 *  - A value IDENTICAL to the catalog it overrides IS a finding: it forks nothing,
 *    so it's dead weight that also freezes a copy of a string that will drift.
 *    There's no justification escape hatch here, deliberately: the fix is always
 *    "delete the key", and a justification could only argue for keeping something
 *    that changes nothing.
 *  - A key in NEITHER the catalog it overrides nor `en` is a finding: an overlay
 *    must not invent keys (nothing would ever render it).
 *
 * ERROR, not a warn: a translation feature is exactly the kind of headline a
 * warn-only signal lets slip past a release, so coverage gaps block the build.
 * The full rule table across the checks: `docs/guides/i18n.md` § Overlay catalogs.
 *
 * Run: `pnpm i18n:check-coverage` (desktop) or `node scripts/i18n-check-coverage.ts`.
 * Pass `--messages-root <dir>` to point at a fixture (used by the tests).
 */

import { BASE_LOCALE, isRawKey, showsOnlySourceText } from './i18n-catalog-lib.ts'
import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.ts'

/**
 * Classifies one English key against a locale's catalog: `missing`, `identical`,
 * `sourceTextOnly`, or `null` (translated, or deliberately-English-and-justified).
 * Exposed for unit tests.
 *
 * `identical` is a byte match; `sourceTextOnly` is a value that shows the reader
 * only English text under a different plural/select branch set (see the file
 * header). Both are EXEMPT (return `null`) when the locale's `@key` metadata
 * carries a non-empty `sameAsSourceJustification` string — the translator's reason
 * it's correctly English (a brand, a unit, a placeholder-only string, a loanword
 * the locale genuinely shares). The exemption never applies to a missing key: that
 * stays `missing` even with a justification recorded.
 *
 * @param key the English message key
 * @param englishValue the English value
 * @param localeMessages the locale's messages
 * @param localeMetadata the locale's `@key` metadata
 */
export function coverageStatus(
  key: string,
  englishValue: string,
  localeMessages: Record<string, string>,
  localeMetadata: Record<string, Record<string, unknown>> = {},
): 'missing' | 'identical' | 'sourceTextOnly' | null {
  if (!(key in localeMessages)) return 'missing'
  const localeValue = localeMessages[key]
  const untranslated =
    localeValue === englishValue
      ? 'identical'
      : // The raw `errors.*` family never reaches the ICU engine, so "is this still
        // English" is a byte question there and an ICU parse would only misread it.
        !isRawKey(key) && showsOnlySourceText(englishValue, localeValue)
        ? 'sourceTextOnly'
        : null
  if (untranslated === null) return null
  // The record index types as non-nullish, but a key with no `@key` metadata is
  // undefined at runtime; the optional chain guards that.
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
  const justification = localeMetadata[key]?.['sameAsSourceJustification']
  if (typeof justification === 'string' && justification !== '') return null
  return untranslated
}

/**
 * Classifies one key of an OVERLAY catalog against the catalog it overrides:
 * `redundant` (byte-identical, so it forks nothing), `unknown` (the key exists in
 * neither the overridden catalog nor `en`, so nothing would render it), or `null`
 * (a genuine fork). Exposed for unit tests.
 *
 * `sameAsSourceJustification` is deliberately NOT honoured here: for a full
 * translation an identical value can be correct and needs vouching for, while on
 * an overlay it's always deletable, so an exemption would only preserve dead
 * weight. See the file header.
 *
 * @param key the overlay's message key
 * @param overlayValue the overlay's value
 * @param sourceMessages the messages of the catalog it overrides (its language
 *   base layered over `en`)
 */
export function overlayStatus(
  key: string,
  overlayValue: string,
  sourceMessages: Record<string, string>,
): 'redundant' | 'unknown' | null {
  const sourceValue = sourceMessages[key]
  // The record index is `string` to the types, but undefined at runtime when the key is absent.
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
  if (sourceValue === undefined) return 'unknown'
  return overlayValue === sourceValue ? 'redundant' : null
}

/**
 * Runs the coverage check over the catalogs under `messagesRoot`.
 * @param opts.messagesRoot override the `messages/` root (for tests)
 * @param opts.write output sink, one line at a time (for tests)
 */
export function runCoverageCheck(opts: { messagesRoot?: string; write?: (line: string) => void } = {}): number {
  return runLocaleCheck({
    title: 'Translation coverage',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    summaryLine: (count, { isOverlay }) =>
      isOverlay
        ? `${String(count)} key(s) an overlay shouldn't carry (identical to what it overrides, or unknown):`
        : `${String(count)} key(s) not translated (missing → English fallback, or identical to English):`,
    inspectLocale: ({ source, overrides, isOverlay, catalog, findings }) => {
      if (isOverlay) {
        // `en-GB` overrides `en` itself, so name one catalog, not "neither en nor en".
        const where = overrides === BASE_LOCALE ? BASE_LOCALE : `${overrides} or ${BASE_LOCALE}`
        for (const [key, overlayValue] of Object.entries(catalog.messages)) {
          const status = overlayStatus(key, overlayValue, source.messages)
          if (status === 'redundant') {
            findings.add(key, `identical to ${overrides}; delete it, the fallback already renders this`)
          } else if (status === 'unknown') {
            findings.add(key, `unknown key; it's not in ${where}, so nothing renders it`)
          }
        }
        return
      }
      for (const [key, englishValue] of Object.entries(source.messages)) {
        const status = coverageStatus(key, englishValue, catalog.messages, catalog.metadata)
        if (status === 'missing') findings.add(key, 'missing; renders the English fallback')
        else if (status === 'identical') findings.add(key, 'identical to English; possibly untranslated')
        else if (status === 'sourceTextOnly') {
          findings.add(key, 'every branch reads as English (only the plural categories differ); possibly untranslated')
        }
      }
    },
  })
}

// Run as a CLI (not when imported by tests).
if (import.meta.url === `file://${process.argv[1]}`) {
  const rootFlag = process.argv.indexOf('--messages-root')
  const messagesRoot = rootFlag !== -1 ? process.argv[rootFlag + 1] : undefined
  try {
    process.exit(runCoverageCheck({ messagesRoot }))
  } catch (err) {
    console.error(`Couldn't run the coverage check: ${err instanceof Error ? err.message : String(err)}`)
    process.exit(EXIT_ERROR)
  }
}
