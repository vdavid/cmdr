#!/usr/bin/env node
/**
 * KEY PARITY / UNTRANSLATED VISIBILITY check (i18n maintenance, M3): WARN class.
 *
 * Honest-coverage signal so a "100% translated" claim can be trusted (mirrors the
 * screenshot coverage report's "say what's covered, list what isn't" stance). Two
 * gaps, both warn-only (neither crashes, since the runtime falls back to English):
 *  - MISSING: an English key with no entry in the locale. The runtime silently
 *    renders English, so the gap is invisible without this check.
 *  - IDENTICAL: a locale value byte-identical to English. Usually means
 *    untranslated (copied through), though a few keys legitimately match (a bare
 *    brand token, a symbol). Reported as a softer "possibly untranslated" note so
 *    a human can confirm; it never fails anything.
 *
 * A key that legitimately stays identical (a brand name, a unit symbol, a
 * placeholder-only string, or a word the locale genuinely shares with English) is
 * EXEMPTED from the IDENTICAL signal by recording a non-empty
 * `@key.sameAsSourceJustification` on it in the locale catalog — the translator's
 * one-line reason it's deliberately identical. Present + non-empty → not a
 * finding. The exemption only suppresses IDENTICAL, never MISSING (a justification
 * can't excuse an absent key). See `messages/DETAILS.md` § `@key` schema and
 * `docs/guides/i18n-translation.md` § Deliberately-identical strings. The stale
 * check invalidates the justification once the English source changes (its
 * `sourceHash` stops matching), so it can't silently outlive the text it vouched
 * for.
 *
 * Warn-only by design: coverage is a maintenance/visibility metric, not a build
 * breaker (the spec lists it in the WARN class with a `NotInCI` reason like the
 * stale check). English-only today → a clean no-op.
 *
 * Run: `pnpm i18n:check-coverage` (desktop) or `node scripts/i18n-check-coverage.js`.
 * Pass `--messages-root <dir>` to point at a fixture (used by the tests).
 */

import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.js'

/**
 * Classifies one English key against a locale's catalog: `missing`,
 * `identical`, or `null` (translated, or deliberately-identical-and-justified).
 * Exposed for unit tests.
 *
 * An identical value is EXEMPT (returns `null`) when the locale's `@key` metadata
 * carries a non-empty `sameAsSourceJustification` string — the translator's reason
 * it's correctly identical (a brand, a unit, a placeholder-only string, a shared
 * word). The exemption applies ONLY to the identical case: a missing key is still
 * `missing` even with a justification recorded.
 * @param {string} key the English message key
 * @param {string} englishValue the English value
 * @param {Record<string, string>} localeMessages the locale's messages
 * @param {Record<string, Record<string, unknown>>} [localeMetadata] the locale's `@key` metadata
 * @returns {'missing' | 'identical' | null}
 */
export function coverageStatus(key, englishValue, localeMessages, localeMetadata = {}) {
  if (!(key in localeMessages)) return 'missing'
  if (localeMessages[key] === englishValue) {
    const justification = localeMetadata[key]?.['sameAsSourceJustification']
    if (typeof justification === 'string' && justification !== '') return null
    return 'identical'
  }
  return null
}

/**
 * Runs the coverage check over the catalogs under `messagesRoot`.
 * @param {object} [opts]
 * @param {string} [opts.messagesRoot] override the `messages/` root (for tests)
 * @param {(line: string) => void} [opts.write] output sink, one line at a time (for tests)
 * @returns {number}
 */
export function runCoverageCheck(opts = {}) {
  return runLocaleCheck({
    title: 'Translation coverage',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    summaryLine: (count) =>
      `${String(count)} key(s) not translated (missing → English fallback, or identical to English):`,
    inspectLocale: ({ base, locale_catalog: localeCatalog, findings }) => {
      for (const [key, englishValue] of Object.entries(base.messages)) {
        const status = coverageStatus(key, englishValue, localeCatalog.messages, localeCatalog.metadata)
        if (status === 'missing') findings.add(key, 'missing; renders the English fallback')
        else if (status === 'identical') findings.add(key, 'identical to English; possibly untranslated')
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
