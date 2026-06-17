#!/usr/bin/env node
/**
 * STALE-TRANSLATION check (i18n maintenance, M2).
 *
 * A translated value in a non-`en` locale records, in `@key.sourceHash`, a hash
 * of the EXACT English value it was translated from (written by the pseudolocale
 * generator / a locale skeleton; see `i18n-catalog-lib.js` `sourceHash()` and
 * `messages/DETAILS.md` § `@key` schema). When the English source later changes,
 * the stored hash no longer matches the current English value's hash, so the
 * translation is STALE: it renders text translated from a sentence that no longer
 * exists. This check flags those.
 *
 * For every NON-`en` locale, for every key present in that locale:
 *  - the English key is gone           → stale ("English source removed").
 *  - no `@key.sourceHash` is stored    → stale ("no source hash recorded").
 *  - stored hash ≠ hash(current `en`)  → stale ("source changed since translation").
 *  - stale AND `reviewed: true`         → ALSO flagged (the human sign-off no longer
 *    applies; a re-translation needs a fresh review). The check never edits files;
 *    it reports that the `reviewed` flag is now meaningless so a human resets it.
 *
 * Two strictness modes, selected by the `CMDR_I18N_STALE_STRICT` env var:
 *  - NORMAL (unset): a stale finding exits 1, which the Go wrapper maps to a
 *    WARN. Stale translations are a maintenance signal, not a daily-dev build
 *    breaker (David's call), so normal `pnpm check` never fails on staleness.
 *  - RELEASE-STRICT (set, e.g. `CMDR_I18N_STALE_STRICT=1`): a stale finding
 *    exits 2 (`EXIT_ERROR`), which the Go wrapper maps to a build-failing ERROR.
 *    The release flow (`scripts/release.sh`) sets this so a release can NOT ship
 *    a stale translation. The check NEVER requires human review to pass: it may
 *    REPORT that a stale key's prior `reviewed` no longer applies, but review is
 *    not a gate in either mode.
 *
 * A genuine error (can't read a catalog) throws and exits 2 in both modes (the
 * Go wrapper tells the two-exit-2 cases apart by whether the env var is set). In
 * today's English-only repo there are no non-`en` locales, so it's a clean no-op
 * either way.
 *
 * Run: `pnpm i18n:check-stale` (desktop) or `node scripts/i18n-check-stale.js`.
 * Pass `--messages-root <dir>` to point at a fixture (used by the tests).
 */

import { sourceHash } from './i18n-catalog-lib.js'
import { EXIT_ERROR, EXIT_ISSUES, runLocaleCheck } from './i18n-locale-check-lib.js'

/** Env var the release flow sets to escalate a stale finding from WARN to a build-failing ERROR. */
export const STALE_STRICT_ENV = 'CMDR_I18N_STALE_STRICT'

/**
 * Classifies one locale key against the current English catalog. Returns a short
 * stale reason, or `null` if the key is fresh.
 * @param {string} key the message key present in the locale
 * @param {Record<string, string>} enMessages current English messages
 * @param {Record<string, unknown> | undefined} keyMetadata the locale's `@key` metadata (absent for a key with no metadata)
 * @returns {string | null} stale detail, or null if fresh
 */
export function staleReason(key, enMessages, keyMetadata) {
  const englishValue = enMessages[key]
  if (englishValue === undefined) return 'English source removed; drop this translated key'

  const stored = keyMetadata && typeof keyMetadata === 'object' ? keyMetadata['sourceHash'] : undefined
  if (typeof stored !== 'string' || stored === '') {
    return 'no source hash recorded; re-translate and stamp @key.sourceHash'
  }

  if (stored !== sourceHash(englishValue)) {
    const reviewed = keyMetadata && typeof keyMetadata === 'object' ? keyMetadata['reviewed'] === true : false
    return reviewed
      ? 'source changed since translation (the reviewed flag no longer applies; reset it and re-review)'
      : 'source changed since translation'
  }
  return null
}

/**
 * Runs the stale check over the catalogs under `messagesRoot` (default: the real
 * `messages/`). Returns the process exit code.
 *
 * Normal mode returns `EXIT_ISSUES` (1, WARN) on a stale finding; strict mode
 * (`strict: true`, set by the release flow) returns `EXIT_ERROR` (2, build-fail)
 * instead. A clean run returns `EXIT_CLEAN` (0) in both modes. Review is never a
 * gate: a stale key that carries `reviewed: true` is reported with a reset note,
 * but the absence of review never makes a clean key fail.
 * @param {object} [opts]
 * @param {string} [opts.messagesRoot] override the `messages/` root (for tests)
 * @param {boolean} [opts.strict] escalate a stale finding from WARN (exit 1) to ERROR (exit 2)
 * @param {(line: string) => void} [opts.write] output sink, one line at a time (for tests)
 * @returns {number}
 */
export function runStaleCheck(opts = {}) {
  const code = runLocaleCheck({
    title: 'Stale translations',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    summaryLine: (count) => `${String(count)} stale key(s) (source changed since translation):`,
    inspectLocale: ({ base, locale_catalog: localeCatalog, findings }) => {
      for (const key of Object.keys(localeCatalog.messages)) {
        const reason = staleReason(key, base.messages, localeCatalog.metadata[key])
        if (reason !== null) findings.add(key, reason)
      }
    },
  })
  // Release-strict: a stale finding (exit 1) becomes a build-failing ERROR (exit 2).
  // A clean run (exit 0) stays clean in both modes.
  if (opts.strict && code === EXIT_ISSUES) return EXIT_ERROR
  return code
}

// Run as a CLI (not when imported by tests).
if (import.meta.url === `file://${process.argv[1]}`) {
  const rootFlag = process.argv.indexOf('--messages-root')
  const messagesRoot = rootFlag !== -1 ? process.argv[rootFlag + 1] : undefined
  const strict = process.env[STALE_STRICT_ENV] === '1'
  try {
    process.exit(runStaleCheck({ messagesRoot, strict }))
  } catch (err) {
    console.error(`Couldn't run the stale check: ${err instanceof Error ? err.message : String(err)}`)
    process.exit(EXIT_ERROR)
  }
}
