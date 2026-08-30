#!/usr/bin/env node
/**
 * ICU VALIDITY check (i18n maintenance): ERROR class.
 *
 * Every NON-`en` ICU message must compile via `intl-messageformat` (the exact
 * engine the runtime uses). A stray unescaped `'`/`{`/`<`, an unclosed tag, or a
 * malformed `plural`/`select` THROWS at render time, so an invalid locale message
 * is a runtime crash, not a typo, so this FAILS the build (Go wrapper maps exit
 * 1 → ERROR).
 *
 * The RAW families (`isRawKey`: `errors.*` plus the native `menu.*` /
 * `licensing.windowTitle.*` / `main.instanceLock.*` that Rust draws) are not
 * parsed as ICU. They resolve through `getMessage()` / `menu_t`, and their
 * `{system_settings}` tokens, literal `<…>` text, markdown, and lone apostrophes
 * deliberately are NOT valid ICU: running them through the parser would
 * false-flag valid raw copy. Their `{token}`s are guarded by the parity check.
 *
 * What they get INSTEAD is the mirror rule: a raw value must not carry ICU
 * escaping. `''` is ICU's escape for one apostrophe, and nothing collapses it on
 * the raw path, so `Cmdr can''t` puts two apostrophes in the real macOS menu bar.
 * The two rules together are one question, "is this value written in the grammar
 * of its own family?", which is why one check owns both.
 *
 * `en` is checked too (`includeBaseLocale`), unlike in the other locale checks:
 * this rule is about a catalog's own syntax, not about how a translation stands
 * against its source, so the base catalog is subject to it as much as any locale.
 *
 * Run: `pnpm i18n:check-icu` (desktop) or `node scripts/i18n-check-icu.ts`.
 * Pass `--messages-root <dir>` to point at a fixture (used by the tests).
 */

import { parseMessage, isRawKey } from './i18n-catalog-lib.ts'
import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.ts'

/**
 * Returns what's wrong with a value's syntax for ITS family, or `null` if it's
 * fine: an ICU parse error for an ICU key, ICU escaping for a raw one. Exposed
 * for unit tests.
 * @param key the message key (decides which family's grammar applies)
 * @param localeValue the locale's value
 */
export function icuError(key: string, localeValue: string): string | null {
  if (isRawKey(key)) {
    // No ICU engine on this path, so `''` is not an escape, it's two apostrophes
    // on screen (in the real macOS menu bar, for the native families).
    return localeValue.includes("''")
      ? "ICU-doubled apostrophe ('') in a raw value: nothing collapses it here, so it renders as two apostrophes; write a single '"
      : null
  }
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
    title: 'Message syntax',
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    includeBaseLocale: true,
    summaryLine: (count) =>
      `${String(count)} message(s) not written in their family's grammar (invalid ICU, or ICU escaping in a raw value):`,
    inspectLocale: ({ catalog, findings }) => {
      for (const [key, localeValue] of Object.entries(catalog.messages)) {
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
    console.error(`Couldn't run the message-syntax check: ${err instanceof Error ? err.message : String(err)}`)
    process.exit(EXIT_ERROR)
  }
}
