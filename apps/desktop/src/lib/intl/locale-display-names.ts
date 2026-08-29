/**
 * The Language picker's row labels: each shipped locale named in its OWN
 * language, so the list is self-describing and no language name is ever
 * hardcoded. `de` → "Deutsch", `en-GB` → "British English".
 *
 * The label answers ONE question: "which of these rows is mine?" That question
 * is about the list in front of the user, which is why these labels are
 * computed against the shipped set rather than per tag in isolation.
 *
 * `Intl.DisplayNames` is not a number/date formatter, so this module is exempt
 * from the `no-raw-locale-format` rule.
 */

import { likelyScript } from './locale-inheritance'

/** The language subtag alone: `zh-Hant` → `zh`, `en-GB` → `en`. */
function baseLanguage(tag: string): string {
  return tag.split('-')[0].toLowerCase()
}

/**
 * The tag to ask `Intl.DisplayNames` for, which is not always the tag itself.
 *
 * CLDR names a locale by DIALECT, which resolves the region axis on its own:
 * `en`, `en-GB`, and `en-AU` come back as "English", "British English", and
 * "Australian English", all distinct with nothing added. It does NOT resolve the
 * script axis: bare `zh` is just "中文", which a Traditional reader can't tell
 * apart from the Traditional row beside it.
 *
 * So when a sibling catalog of the same language is written in a DIFFERENT
 * script, we ask for `<language>-<script>` instead and get "简体中文" against
 * "繁體中文". The script comes from `likelyScript()`, the same CLDR answer that
 * decides which catalogs may inherit from each other, so the picker and the
 * fallback chain can't disagree about what `zh` is.
 *
 * ❌ Don't decorate unconditionally. Maximizing every tag yields "Deutsch
 * (Lateinisch)" and "English (Latin)": a qualifier that distinguishes nothing is
 * noise, and it's why macOS writes "English" rather than "English (Latin)" too.
 * The rule is general, so a future `sr` beside a `sr-Latn` decorates itself with
 * no edit here, and a `pt-PT` beside `pt` correctly does NOT (same script; CLDR's
 * dialect names already separate them).
 */
function labelTag(tag: string, shipped: readonly string[]): string {
  const script = likelyScript(tag)
  if (script === '') return tag
  const language = baseLanguage(tag)
  const scriptIsDistinguishing = shipped.some(
    (other) => other !== tag && baseLanguage(other) === language && likelyScript(other) !== script,
  )
  return scriptIsDistinguishing ? `${language}-${script}` : tag
}

/**
 * The picker label for `tag`, in the locale's own language.
 *
 * Resolved in the option's OWN locale, so the `en` row reads "English" whatever
 * the app currently speaks: the way out for a user who can't read the current
 * language. Falls back to the raw tag when `Intl` resolves no name.
 *
 * ❌ No two rows may share a label; `locale-display-names.test.ts` asserts it
 * across the whole shipped set. If a new catalog collides, decide on the label
 * rather than letting the picker show one word twice.
 *
 * @param tag the locale to label
 * @param shipped every catalog tag on offer, which is what makes a qualifier
 *   necessary or noise
 */
export function localeDisplayName(tag: string, shipped: readonly string[]): string {
  try {
    const asked = labelTag(tag, shipped)
    const name = new Intl.DisplayNames([asked], { type: 'language' }).of(asked)
    if (name !== undefined && name !== asked) {
      // Capitalize the first letter: many languages lowercase their endonym, but
      // a selector option reads better title-first. Locale-aware via the tag.
      return name.charAt(0).toLocaleUpperCase(tag) + name.slice(1)
    }
  } catch {
    // fall through to the raw tag
  }
  return tag
}
