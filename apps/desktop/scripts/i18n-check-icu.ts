#!/usr/bin/env node
/**
 * ICU VALIDITY check (i18n maintenance, M3): ERROR class.
 *
 * Every NON-`en` ICU message must compile via `intl-messageformat` (the exact
 * engine the runtime uses). A stray unescaped `'`/`{`/`<`, an unclosed tag, or a
 * malformed `plural`/`select` THROWS at render time, so an invalid locale message
 * is a runtime crash, not a typo, so this FAILS the build (Go wrapper maps exit
 * 1 → ERROR).
 *
 * The raw `errors.*` family is EXCLUDED (`isRawKey`): it resolves through
 * `getMessage()` (a raw lookup, no ICU), and its `{system_settings}` tokens,
 * literal `<…>` text, markdown, and lone apostrophes deliberately are NOT valid
 * ICU: running them through the ICU parser would false-flag valid raw copy. The
 * raw family's structure is guarded by the parity check's `{token}` comparison
 * instead. (The pseudolocale generator makes the same split via `isRawKey`, so
 * the fixture's raw value stays raw and is correctly skipped here.)
 *
 * Run: `pnpm i18n:check-icu` (desktop) or `node scripts/i18n-check-icu.ts`.
 * Pass `--messages-root <dir>` to point at a fixture (used by the tests).
 */

import { parseMessage, isRawKey } from './i18n-catalog-lib.ts'
import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.ts'

/**
 * Returns the ICU parse error for a locale value, or `null` if it compiles (or is
 * a raw key, which isn't ICU). Exposed for unit tests.
 * @param key the message key (used to skip the raw `errors.*` family)
 * @param localeValue the locale's value
 */
export function icuError(key: string, localeValue: string): string | null {
  if (isRawKey(key)) return null // raw errors.* aren't ICU; the parity check guards their tokens
  const r = parseMessage(localeValue)
  if (r.ok) return null
  // Collapse newlines so a multi-line parser message stays on one finding line.
  return `invalid ICU: ${(r.error ?? 'parse failed').replace(/\s+/g, ' ').trim()}`
}

/**
 * Runs the ICU-validity check over the catalogs under `messagesRoot`.
 * @param opts.messagesRoot override the `messages/` root (for tests)
 * @param opts.write output sink, one line at a time (for tests)
 */
export function runIcuCheck(opts: { messagesRoot?: string; write?: (line: string) => void } = {}): number {
  return runLocaleCheck({
    title: 'ICU validity',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    summaryLine: (count) => `${String(count)} message(s) that don't compile as ICU (would crash at runtime):`,
    inspectLocale: ({ locale_catalog: localeCatalog, findings }) => {
      for (const [key, localeValue] of Object.entries(localeCatalog.messages)) {
        const detail = icuError(key, localeValue)
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
    process.exit(runIcuCheck({ messagesRoot }))
  } catch (err) {
    console.error(`Couldn't run the ICU-validity check: ${err instanceof Error ? err.message : String(err)}`)
    process.exit(EXIT_ERROR)
  }
}
