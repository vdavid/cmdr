#!/usr/bin/env node
/**
 * DON'T-TRANSLATE TOKENS check (i18n maintenance, M3): WARN class.
 *
 * Some tokens must survive translation verbatim: brand and product names, and the
 * system-label substitution tokens. If an English value contains one and the
 * locale's value dropped it, the translator likely localized something that
 * shouldn't be. That's a quality slip, not a crash, so warn-only.
 *
 * The curated lists (`BRAND_WORDS`, `SYSTEM_TOKENS`) live in `i18n-catalog-lib.ts`
 * as the single source of truth, shared with the pseudolocale generator (which
 * keeps them verbatim so en-XA passes this check). Two token kinds (NO
 * language-specific translations, only token NAMES):
 *  1. Brand/system WORDS: literal substrings that must appear verbatim. Derived
 *     from the brand glossary (`brand/copy/cmdr-copy.md`, `docs/guides/branding.md`)
 *     and the product's external entities. Matched whole-word, case-sensitively
 *     (so "macOS" must stay "macOS"). A locale legitimately omitting one (a
 *     reworded sentence) produces a soft warn for a human to confirm.
 *  2. Substitution TOKENS: `{system_settings}` and friends, the
 *     `expandSystemStrings` placeholders the raw error pipeline replaces by name
 *     (mirrors `src/lib/system-strings.svelte.ts` `ENGLISH_DEFAULTS`). These are
 *     ALSO guarded structurally by the parity check (raw `{token}` parity); listing
 *     them here adds a clearer, token-specific message for the common case.
 *
 * Warn-only by design (a judgment-call quality signal, per the spec's WARN class
 * with a `NotInCI` reason). English-only today → a clean no-op.
 *
 * Run: `pnpm i18n:check-dont-translate` (desktop) or
 * `node scripts/i18n-check-dont-translate.ts`. Pass `--messages-root <dir>` to
 * point at a fixture (used by the tests).
 */

import { BRAND_WORDS, SYSTEM_TOKENS, hasWholeWord, hasBrandPresent } from './i18n-catalog-lib.ts'
import { EXIT_ERROR, runLocaleCheck } from './i18n-locale-check-lib.ts'

export { BRAND_WORDS, SYSTEM_TOKENS }

/**
 * Lists the don't-translate tokens that English carries for a key but the locale's
 * value dropped. For brand words the two sides use DIFFERENT presence tests on
 * purpose: STRICT whole-word on the English side (does en carry the bare brand?),
 * but suffix-aware `hasBrandPresent` on the locale side (does the locale still carry
 * it, possibly inflected — "Cmdrben", "Cmdrs"?). So a genuine omission is still
 * flagged, but a brand that took a natural inflectional suffix is not a false drop.
 * System tokens are literal substrings on both sides. Exposed for unit tests.
 * @param englishValue the English value
 * @param localeValue the locale's value
 * @returns dropped tokens (sorted, deduped)
 */
export function droppedTokens(englishValue: string, localeValue: string): string[] {
  const dropped: string[] = []
  for (const word of BRAND_WORDS) {
    if (hasWholeWord(englishValue, word) && !hasBrandPresent(localeValue, word)) dropped.push(word)
  }
  for (const token of SYSTEM_TOKENS) {
    if (englishValue.includes(token) && !localeValue.includes(token)) dropped.push(token)
  }
  return dropped.sort()
}

/**
 * Runs the don't-translate check over the catalogs under `messagesRoot`.
 * @param opts.messagesRoot override the `messages/` root (for tests)
 * @param opts.write output sink, one line at a time (for tests)
 */
export function runDontTranslateCheck(opts: { messagesRoot?: string; write?: (line: string) => void } = {}): number {
  return runLocaleCheck({
    title: "Don't-translate tokens",
    messagesRoot: opts.messagesRoot,
    write: opts.write,
    summaryLine: (count) => `${String(count)} key(s) dropped a brand/system token that must stay verbatim:`,
    inspectLocale: ({ base, locale_catalog: localeCatalog, findings }) => {
      for (const [key, localeValue] of Object.entries(localeCatalog.messages)) {
        const englishValue = base.messages[key]
        // A locale key with no English counterpart can't drop an English token; skip it.
        // The record index is `string` to the types, but undefined at runtime when the key is absent.
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
        if (englishValue === undefined) continue
        const dropped = droppedTokens(englishValue, localeValue)
        if (dropped.length > 0) findings.add(key, `dropped ${dropped.join(', ')} (keep verbatim)`)
      }
    },
  })
}

// Run as a CLI (not when imported by tests).
if (import.meta.url === `file://${process.argv[1]}`) {
  const rootFlag = process.argv.indexOf('--messages-root')
  const messagesRoot = rootFlag !== -1 ? process.argv[rootFlag + 1] : undefined
  try {
    process.exit(runDontTranslateCheck({ messagesRoot }))
  } catch (err) {
    console.error(`Couldn't run the don't-translate check: ${err instanceof Error ? err.message : String(err)}`)
    process.exit(EXIT_ERROR)
  }
}
