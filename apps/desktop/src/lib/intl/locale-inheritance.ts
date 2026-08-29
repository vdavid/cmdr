/**
 * WHICH CATALOG A LOCALE MAY INHERIT FROM. One rule, two halves: a catalog is
 * reachable for a locale only when it's the same LANGUAGE (so `pt-PT` may reach
 * the Brazilian `pt` catalog, and the `en-GB` overlay reaches US `en` for every
 * key it doesn't fork, deliberately) and the same SCRIPT (so `zh-Hant-TW` does
 * NOT reach the Simplified `zh` one).
 *
 * The script half is the guard: a fallback is only a kindness when it lands
 * somewhere the reader can actually READ. Dialect friction is a papercut a later
 * catalog fixes; an unreadable script is a wall, and English (a language the user
 * at least chose to list) beats it. ❌ Don't "fix" this by blocking regional
 * fallback too: the two cases pull in opposite directions on purpose. Canonical
 * rationale, and the same rule on the Rust side:
 * `apps/desktop/src-tauri/src/intl/DETAILS.md` § The script guard, and why regional
 * fallback survives it.
 *
 * Three layers obey this rule and MUST agree, so they all come through here:
 *  - the message runtime (`messages.svelte.ts` `resolveRaw`), per key;
 *  - the i18n check layer (`scripts/i18n-catalog-lib.ts` `resolveLocaleSource`),
 *    which is how it decides whether a catalog is an overlay of its base or a
 *    full translation of `en`;
 *  - Rust's auto-selection (`src-tauri/src/intl/mod.rs` `match_shipped`), which
 *    can't call `Intl`, so it reads the same CLDR answers off a generated table
 *    (`pnpm intl:shipped-locales`, built with `likelyScript` from right here).
 *
 * The script facts come from CLDR's likely-subtags data via `Intl.Locale`, which
 * knows that `zh` alone is Simplified, `zh-Hant` is Traditional, and so is
 * `zh-TW` even though it names no script.
 *
 * ❌ Keep this module dependency-free (no `$lib` alias, no DOM, no Svelte runes):
 * the build scripts import it directly under bare Node, which resolves neither
 * the alias nor anything the bundler would normally hand us.
 */

/**
 * The script a tag is written in per CLDR's likely subtags, lowercased, or `''`
 * when `Intl` can't resolve one (a malformed tag, or a language with no likely
 * script). Two tags with the same answer are mutually readable.
 * @param tag a BCP-47 tag (`zh-Hant-TW`, `pt`)
 */
export function likelyScript(tag: string): string {
  try {
    return new Intl.Locale(tag).maximize().script?.toLowerCase() ?? ''
  } catch {
    return ''
  }
}

/**
 * A tag's ancestors, nearest first, by dropping one subtag at a time:
 * `zh-Hant-TW` → `['zh-Hant', 'zh']`. Excludes the tag itself.
 * @param tag a BCP-47 tag
 */
export function ancestorTags(tag: string): string[] {
  const parts = tag.split('-')
  const ancestors: string[] = []
  for (let length = parts.length - 1; length >= 1; length--) ancestors.push(parts.slice(0, length).join('-'))
  return ancestors
}

/**
 * The catalogs `locale` may inherit from, nearest first: its ancestors that
 * actually exist AND read the same script. This is the rule in the file header,
 * as one function.
 *
 * `zh-Hant-TW` with `zh-Hant` and `zh` shipped yields `['zh-Hant']`: the
 * Traditional catalog is readable, the Simplified one is a wall. `en-GB` yields
 * `['en']`, `pt-PT` yields `['pt']`, and a `zh-Hant` with only Simplified `zh`
 * shipped yields `[]`, which leaves English as the honest answer.
 *
 * `en` is NOT special-cased: it lands here like any ancestor when it qualifies,
 * and callers append it as the final fallback regardless (the base catalog is
 * always complete, so no key ever dead-ends).
 *
 * @param locale the active locale tag
 * @param available every catalog tag that exists
 */
export function inheritableAncestors(locale: string, available: readonly string[]): string[] {
  const script = likelyScript(locale)
  return ancestorTags(locale).filter((tag) => available.includes(tag) && likelyScript(tag) === script)
}
