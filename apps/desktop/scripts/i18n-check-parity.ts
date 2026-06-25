#!/usr/bin/env node
/**
 * PLACEHOLDER / TAG PARITY check (i18n maintenance, M3): ERROR class.
 *
 * A translation must preserve EXACTLY the substitution structure of its English
 * source: the same set of `{placeholders}` and `<tags>`. A missing, renamed, or
 * extra `{arg}`/`<tag>` is the #1 runtime CRASH class: `intl-messageformat`
 * throws on a `{name}` it has no value for, and the raw error pipeline silently
 * drops or mis-substitutes a token. So this is the one locale check that FAILS
 * the build (the Go wrapper maps exit 1 → ERROR), not a warn.
 *
 * Two comparison paths, by family (`isRawKey`):
 *  - ICU keys (every non-`errors.*`): parse both English and the locale value and
 *    require equal `placeholders` sets AND equal `tags` sets. (`select` categories
 *    ride along inside the message and are covered here via the arg name being a
 *    placeholder; plural-CATEGORY coverage is a separate check.)
 *  - Raw keys (`errors.*`): these bypass ICU (`getMessage()` raw lookup), so
 *    compare the `{token}` brace sets (`rawTokens`) instead: the raw-pipeline
 *    analogue of placeholder parity.
 *
 * A key MISSING from the locale isn't a parity failure (the runtime falls back to
 * English, no crash); the key-parity check surfaces missing keys as a warn. Here
 * we only inspect keys the locale actually defines.
 *
 * Run: `pnpm i18n:check-parity` (desktop) or `node scripts/i18n-check-parity.ts`.
 * Pass `--messages-root <dir>` to point at a fixture (used by the tests).
 */

import { parseMessage, isRawKey, rawTokens } from './i18n-catalog-lib.ts'
import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.ts'

/**
 * Renders a set as a stable, comma-joined string for a finding detail, or `(none)`.
 */
function fmtSet(set: Set<string>): string {
  return set.size === 0 ? '(none)' : [...set].sort().join(', ')
}

/**
 * The members in `a` but not `b`, as a sorted array.
 */
function difference(a: Set<string>, b: Set<string>): string[] {
  return [...a].filter((x) => !b.has(x)).sort()
}

/**
 * Compares one key's English vs locale substitution structure and returns a short
 * parity-failure detail, or `null` if they match. Exposed for unit tests.
 * @param key the message key
 * @param englishValue the English source value
 * @param localeValue the locale's value for the same key
 */
export function parityDetail(key: string, englishValue: string, localeValue: string): string | null {
  if (isRawKey(key)) {
    const en = rawTokens(englishValue)
    const loc = rawTokens(localeValue)
    const missing = difference(en, loc)
    const extra = difference(loc, en)
    if (missing.length === 0 && extra.length === 0) return null
    return `token mismatch: expected {${fmtSet(en)}}, got {${fmtSet(loc)}}`
  }

  const en = parseMessage(englishValue)
  const loc = parseMessage(localeValue)
  // An unparseable locale value is the ICU-validity check's job to report; here we
  // can still compare against English's tokens (the locale's sets are empty on a
  // parse failure, so a parse failure also shows up as a parity mismatch, and both
  // checks flagging it is fine and points at the same fix).
  const phMissing = difference(en.placeholders, loc.placeholders)
  const phExtra = difference(loc.placeholders, en.placeholders)
  const tagMissing = difference(en.tags, loc.tags)
  const tagExtra = difference(loc.tags, en.tags)
  if (phMissing.length === 0 && phExtra.length === 0 && tagMissing.length === 0 && tagExtra.length === 0) {
    return null
  }
  const parts: string[] = []
  if (phMissing.length > 0 || phExtra.length > 0) {
    parts.push(`placeholders expected {${fmtSet(en.placeholders)}}, got {${fmtSet(loc.placeholders)}}`)
  }
  if (tagMissing.length > 0 || tagExtra.length > 0) {
    parts.push(`tags expected <${fmtSet(en.tags)}>, got <${fmtSet(loc.tags)}>`)
  }
  return parts.join('; ')
}

/** Options for `runParityCheck`. */
interface RunParityCheckOptions {
  /** override the `messages/` root (for tests) */
  messagesRoot?: string
  /** output sink, one line at a time (for tests) */
  write?: (line: string) => void
}

/**
 * Runs the parity check over the catalogs under `messagesRoot`. Returns the
 * process exit code.
 */
export function runParityCheck(opts: RunParityCheckOptions = {}): number {
  return runLocaleCheck({
    title: 'Placeholder/tag parity',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    summaryLine: (count) => `${String(count)} key(s) with a placeholder/tag mismatch (would crash at runtime):`,
    inspectLocale: ({ base, locale_catalog: localeCatalog, findings }) => {
      for (const [key, localeValue] of Object.entries(localeCatalog.messages)) {
        const englishValue = base.messages[key]
        // The record index is `string` to the types, but undefined at runtime when the key is absent.
        // missing-from-en is the key-parity/stale check's concern.
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
        if (englishValue === undefined) continue
        const detail = parityDetail(key, englishValue, localeValue)
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
    process.exit(runParityCheck({ messagesRoot }))
  } catch (err) {
    console.error(`Couldn't run the parity check: ${err instanceof Error ? err.message : String(err)}`)
    process.exit(EXIT_ERROR)
  }
}
