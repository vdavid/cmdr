/**
 * Pure logic for the old-WebKit boot guard's build-time data, factored out of
 * the config-time entry point (`svelte.config.js`) so it's unit-testable.
 *
 * The guard is an inline ES5 `<script>` in `src/app.html`. It runs before the
 * module bundle loads, on a WebKit that in the worst case can't PARSE that
 * bundle at all, so it can't call `t()` and can't import anything. This module
 * resolves the three `main.oldWebkit.*` keys against every shipped catalog and
 * hands the guard a plain object it can index by `navigator.language`, which is
 * how a screen outside the app still speaks 14 languages and still tracks the
 * catalog when a translator edits it.
 *
 * Two jobs beyond the lookup:
 *
 *  - **ICU is formatted here, not shipped.** The keys are ordinary ICU messages
 *    (not an `isRawKey` family), so their values carry ICU escaping: French
 *    `l''interface` would reach the screen with two apostrophes. Formatting each
 *    value with `intl-messageformat` and no arguments unescapes it exactly the
 *    way the runtime would.
 *  - **Locale matching is precomputed.** The guard can't run the script-boundary
 *    rule (`locale-inheritance.ts`), so `buildLocaleAliases` bakes the answer
 *    into a flat lowercase map: `zh-tw` lands on Traditional `zh-Hant`, never on
 *    Simplified `zh`. The guard only has to lowercase its tag and drop subtags
 *    until something matches.
 *
 * The pseudolocale is excluded for the same reason `gen-shipped-locales-lib.ts`
 * excludes it: it's gitignored and regenerated, so shipping it would make the
 * shell differ between a fresh clone and a machine that ran `pnpm i18n:pseudo`.
 */

import { IntlMessageFormat } from 'intl-messageformat'
import { inheritableAncestors, likelyScript } from '../src/lib/intl/locale-inheritance.ts'
import { BASE_LOCALE, GENERATED_LOCALES, baseLanguageOf, loadCatalog } from './i18n-catalog-lib.ts'
import { BOOT_GUARD_KEYS } from '../src/lib/utils/boot-guard-keys.ts'

/** The three strings one locale shows on the block screen. */
export interface BootGuardStrings {
  title: string
  body: string
  quit: string
}

/** Everything the inline guard needs, ready to serialize into the shell. */
export interface BootGuardData {
  /** Catalog tag → the resolved, ICU-formatted strings. Always carries `en`. */
  strings: Record<string, BootGuardStrings>
  /** Lowercased BCP-47 key → catalog tag, script boundaries already resolved. */
  aliases: Record<string, string>
  /** True when `VITE_CMDR_FORCE_OLD_WEBKIT=unsupported` asked for the block. */
  force: boolean
}

/**
 * A locale's merged messages, as `loadCatalog` returns them. Indexed as possibly
 * `undefined` because that's what a missing key actually is, and this module's
 * whole job is walking a chain until one isn't.
 */
type Messages = Record<string, string | undefined>

/**
 * The catalogs the guard ships, in the order the runtime would consult them for
 * `locale`: the locale itself, then every ancestor it may inherit from, then
 * `en`. Mirrors the message runtime's chain, so the guard never shows a string
 * the app wouldn't have shown.
 * @param locale the catalog tag being resolved
 * @param available every shipped catalog tag
 */
export function resolutionChain(locale: string, available: readonly string[]): string[] {
  const chain = [locale, ...inheritableAncestors(locale, available)]
  if (!chain.includes(BASE_LOCALE)) chain.push(BASE_LOCALE)
  return chain
}

/**
 * Resolves and formats one locale's three strings.
 * @param locale the catalog tag
 * @param available every shipped catalog tag
 * @param messagesOf reads a catalog's merged messages
 */
export function stringsFor(
  locale: string,
  available: readonly string[],
  messagesOf: (tag: string) => Messages,
): BootGuardStrings {
  const chain = resolutionChain(locale, available)
  const resolve = (key: string): string => {
    for (const tag of chain) {
      const value = messagesOf(tag)[key]
      if (value !== undefined) return String(new IntlMessageFormat(value, tag).format())
    }
    throw new Error(`No catalog in [${chain.join(', ')}] defines the boot-guard key "${key}"`)
  }
  return {
    title: resolve(BOOT_GUARD_KEYS.title),
    body: resolve(BOOT_GUARD_KEYS.body),
    quit: resolve(BOOT_GUARD_KEYS.quit),
  }
}

/**
 * Maps every lowercase BCP-47 key the guard could meet to a catalog tag.
 *
 * Three kinds of entry, and the third is the one that earns this function:
 *  - the catalog tags themselves (`en-gb` → `en-GB`),
 *  - `language-script` for each catalog (`zh-hans` → `zh`), so a tag that spells
 *    its script out still lands right,
 *  - `language-region` for every region whose likely script picks one catalog
 *    over its sibling, but ONLY for a language that ships more than one script.
 *    That's what sends `zh-tw` to Traditional instead of Simplified. Probing all
 *    1,676 region subtags for every language would be wasted work at config
 *    load; a single-script language can't have an ambiguous region anyway.
 *
 * @param locales every shipped catalog tag
 */
export function buildLocaleAliases(locales: readonly string[]): Record<string, string> {
  const aliases: Record<string, string> = {}
  const add = (key: string, tag: string): void => {
    const lower = key.toLowerCase()
    if (!(lower in aliases)) aliases[lower] = tag
  }

  for (const tag of locales) add(tag, tag)
  for (const tag of locales) {
    const script = likelyScript(tag)
    if (script !== '') add(`${baseLanguageOf(tag)}-${script}`, tag)
  }

  const byLanguage = new Map<string, string[]>()
  for (const tag of locales) {
    const language = baseLanguageOf(tag)
    byLanguage.set(language, [...(byLanguage.get(language) ?? []), tag])
  }
  for (const [language, tags] of byLanguage) {
    const scripts = new Set(tags.map((tag) => likelyScript(tag)))
    if (scripts.size < 2) continue
    for (const region of allRegionSubtags()) {
      const regionScript = likelyScript(`${language}-${region}`)
      const match = tags.find((tag) => likelyScript(tag) === regionScript)
      if (match !== undefined) add(`${language}-${region}`, match)
    }
  }
  return aliases
}

/**
 * Every region subtag worth probing: the 676 two-letter combinations plus the
 * 1,000 three-digit UN M49 codes. Enumerated rather than listed for the reason
 * `gen-shipped-locales-lib.ts` gives: CLDR's region set drifts with every ICU
 * update, and unknown codes maximize to the language default and contribute
 * nothing.
 */
function allRegionSubtags(): string[] {
  const regions: string[] = []
  for (let first = 65; first <= 90; first++) {
    for (let second = 65; second <= 90; second++) {
      regions.push(String.fromCharCode(first) + String.fromCharCode(second))
    }
  }
  for (let code = 0; code < 1000; code++) regions.push(String(code).padStart(3, '0'))
  return regions
}

/**
 * Builds the whole payload from the catalogs on disk.
 * @param opts.locales shipped catalog tags (defaults to reading `messages/`)
 * @param opts.messagesRoot override the `messages/` root (for tests)
 * @param opts.force bake in the forced block (the dev override)
 */
export function buildBootGuardData(opts: {
  locales: readonly string[]
  messagesRoot?: string
  force?: boolean
}): BootGuardData {
  const shipped = opts.locales.filter((locale) => !GENERATED_LOCALES.has(locale))
  const cache = new Map<string, Messages>()
  const messagesOf = (tag: string): Messages => {
    let messages = cache.get(tag)
    if (messages === undefined) {
      messages = loadCatalog(tag, opts.messagesRoot).messages
      cache.set(tag, messages)
    }
    return messages
  }

  const strings: Record<string, BootGuardStrings> = {}
  for (const locale of shipped) strings[locale] = stringsFor(locale, shipped, messagesOf)
  const aliases = buildLocaleAliases(shipped)
  pruneRedundant(strings, aliases)
  return { strings, aliases, force: opts.force === true }
}

/**
 * What the guard's own matcher lands on for `key` once its last subtag is
 * dropped: the first alias hit walking `zh-hant-tw` → `zh-hant` → `zh`, or
 * `undefined` when nothing shorter matches. Kept here so the pruning below can
 * only ever remove an entry the matcher would have resolved identically.
 * @param key a lowercase alias key
 * @param aliases the alias map
 */
function afterDroppingSubtag(key: string, aliases: Readonly<Record<string, string>>): string | undefined {
  const parts = key.split('-')
  for (let end = parts.length - 1; end > 0; end--) {
    const shorter = parts.slice(0, end).join('-')
    if (shorter in aliases) return aliases[shorter]
  }
  return undefined
}

/**
 * Drops every entry the guard's subtag walk would have resolved the same way,
 * then every strings block nothing points at any more.
 *
 * Without it the payload carries ~1,700 aliases (26 KB): the region probe emits
 * one entry per region subtag for a multi-script language, and all but a handful
 * repeat what dropping the region already gives. It also drops an overlay whose
 * copy hasn't forked (`en-GB` shows English here), and picks it back up
 * automatically the day it does.
 * @param strings tag → strings, mutated in place
 * @param aliases lowercase key → tag, mutated in place
 */
function pruneRedundant(strings: Record<string, BootGuardStrings>, aliases: Record<string, string>): void {
  const same = (a: BootGuardStrings, b: BootGuardStrings): boolean =>
    a.title === b.title && a.body === b.body && a.quit === b.quit

  for (const key of Object.keys(aliases).sort((a, b) => b.length - a.length)) {
    const tag = aliases[key]
    const next = afterDroppingSubtag(key, aliases)
    if (next === undefined) continue
    if (next === tag || same(strings[tag], strings[next])) aliases[key] = REDUNDANT
  }
  for (const key of Object.keys(aliases)) {
    if (aliases[key] === REDUNDANT) Reflect.deleteProperty(aliases, key)
  }

  const reachable = new Set(Object.values(aliases))
  for (const tag of Object.keys(strings)) {
    if (tag !== BASE_LOCALE && !reachable.has(tag)) Reflect.deleteProperty(strings, tag)
  }
}

/** Marks an alias for removal; no catalog tag can collide with it. */
const REDUNDANT = '\u0000redundant'

/** The marker in `src/app.html` that the payload replaces. */
export const BOOT_GUARD_MARKER = '/* cmdr:boot-guard-data */ null'

/**
 * Splices the payload into the shell.
 *
 * A missing marker is a hard failure rather than a pass-through: the shell would
 * still load and the app would still run, and the only thing lost would be the
 * screen that catches a WebKit nobody on the team has. That's exactly the kind
 * of breakage nothing else would report.
 * @param template the contents of `src/app.html`
 * @param data the payload
 */
export function injectBootGuardData(template: string, data: BootGuardData): string {
  if (!template.includes(BOOT_GUARD_MARKER)) {
    throw new Error(
      `The app shell no longer carries the boot-guard marker \`${BOOT_GUARD_MARKER}\`. ` +
        'Restore it inside the guard script in `src/app.html`, or the old-WebKit block screen ships with no strings.',
    )
  }
  return template.replace(BOOT_GUARD_MARKER, JSON.stringify(data))
}
