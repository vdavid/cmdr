/**
 * Shared, language-agnostic catalog + ICU helper for the i18n maintenance
 * tooling: the pseudolocale generator, the stale check, and the locale-parity /
 * ICU-validity / plural-coverage checks all build on this single module.
 *
 * Three responsibilities, all pure/deterministic (no app/runtime imports beyond
 * the ICU parser, no `window`/DOM, no time/RNG):
 *
 *  1. Catalog I/O: load a locale's merged catalog (`messages/<locale>/*.json`),
 *     stripping the ARB-style `@key` metadata into a separate map; enumerate the
 *     available locales and each locale's key set.
 *  2. ICU parsing: parse each message to its AST with the SAME engine the
 *     runtime uses (`intl-messageformat`, see below), then extract the structure
 *     a translation must preserve: placeholder/argument names, `<tag>` names, and
 *     the `plural`/`select` categories per argument. A single source of truth for
 *     "what tokens/structure does this message have".
 *  3. Source hashing: `sourceHash(value)` is a git-style 7-char hex of an English
 *     value, stamped into `@key.sourceHash` by the pseudolocale generator and
 *     compared by the stale check.
 *
 * ## Why parse through `intl-messageformat`, not the parser package directly
 *
 * The runtime (`src/lib/intl/messages.svelte.ts`) formats every message through
 * `new IntlMessageFormat(value, locale)`. That class parses with
 * `@formatjs/icu-messageformat-parser` internally and exposes the parsed AST via
 * `.getAst()`. We go through `IntlMessageFormat` rather than importing the parser
 * package by name for two reasons: (a) it's the EXACT code path the runtime
 * takes, so a message we accept is a message the runtime can render (and a
 * message we reject would throw at runtime); (b) under pnpm's strict
 * node_modules the transitive `@formatjs/icu-messageformat-parser` isn't
 * resolvable by bare specifier from this package, whereas `intl-messageformat`
 * is a direct dependency. Construction throws a `SyntaxError` on invalid ICU,
 * which is exactly the `ok: false` signal the ICU-validity check needs.
 *
 * The `intl-messageformat` AST element `type` values (from
 * `@formatjs/icu-messageformat-parser`'s `TYPE` enum) we care about:
 *   0 literal · 1 argument (`{name}`) · 2 number · 3 date · 4 time
 *   5 select · 6 plural · 7 pound (`#`) · 8 tag (`<x>…</x>`)
 */

import { readFileSync, readdirSync, existsSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { createHash } from 'node:crypto'
import { IntlMessageFormat } from 'intl-messageformat'

/** AST element `type` constants (the `@formatjs` `TYPE` enum, inlined to avoid a deep import). */
const TYPE = Object.freeze({
  literal: 0,
  argument: 1,
  number: 2,
  date: 3,
  time: 4,
  select: 5,
  plural: 6,
  pound: 7,
  tag: 8,
})

/** A locale's merged catalog: renderable messages plus the separated `@key` metadata. */
export interface Catalog {
  messages: Record<string, string>
  metadata: Record<string, Record<string, unknown>>
}

/**
 * Whether a catalog entry key is ARB-style `@key` metadata (stripped before the
 * runtime/codegen see it). Mirrors the runtime's `stripMetadata`.
 */
export function isMetadataKey(key: string): boolean {
  return key.startsWith('@')
}

/**
 * Splits one parsed catalog file into renderable messages and `@key` metadata.
 * The runtime keeps only string-valued non-`@` entries as messages; everything
 * `@`-prefixed is metadata, keyed WITHOUT the leading `@` so it lines up with
 * its message key.
 * @param raw a parsed `<area>.json`
 */
export function splitCatalogFile(raw: Record<string, unknown>): Catalog {
  const messages: Record<string, string> = {}
  const metadata: Record<string, Record<string, unknown>> = {}
  for (const [key, value] of Object.entries(raw)) {
    if (isMetadataKey(key)) {
      const messageKey = key.slice(1)
      if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
        metadata[messageKey] = value as Record<string, unknown>
      }
      continue
    }
    if (typeof value === 'string') messages[key] = value
  }
  return { messages, metadata }
}

/**
 * Merges several parsed catalog files (filename → JSON) into one
 * `{ messages, metadata }` pair, matching the runtime's catalog merge. Later
 * files win on a key collision (same as `Object.assign`); in practice keys never
 * collide across area files (prefix ↔ filename is 1:1).
 */
export function mergeCatalogFiles(files: Record<string, Record<string, unknown>>): Catalog {
  const messages: Record<string, string> = {}
  const metadata: Record<string, Record<string, unknown>> = {}
  for (const raw of Object.values(files)) {
    const split = splitCatalogFile(raw)
    Object.assign(messages, split.messages)
    Object.assign(metadata, split.metadata)
  }
  return { messages, metadata }
}

/**
 * The absolute path to the `messages/` root (parent of each `<locale>/` dir),
 * resolved relative to this script. Override `messagesRoot` in tests.
 */
export function resolveMessagesRoot(messagesRoot?: string): string {
  if (messagesRoot) return messagesRoot
  // This file lives in `apps/desktop/scripts/`; messages live in
  // `apps/desktop/src/lib/intl/messages/`.
  return join(import.meta.dirname, '..', 'src', 'lib', 'intl', 'messages')
}

/**
 * Reserved sibling directories under `messages/` that are NOT locales. Today
 * just `screenshots/` (capture artifacts; it holds `*.json` so a "has JSON"
 * heuristic alone would misclassify it).
 */
export const NON_LOCALE_DIRS: Set<string> = new Set(['screenshots'])

/**
 * Lists the locale directories under `messages/` (each holding `<area>.json`
 * files), sorted. A locale is any direct subdirectory that holds at least one
 * `*.json` and isn't a reserved non-locale dir (`NON_LOCALE_DIRS`).
 * @param messagesRoot
 * @returns BCP-47-ish locale tags (the dir names), e.g. `['en', 'en-XA']`
 */
export function listLocales(messagesRoot?: string): string[] {
  const root = resolveMessagesRoot(messagesRoot)
  const locales: string[] = []
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if (!entry.isDirectory() || NON_LOCALE_DIRS.has(entry.name)) continue
    const dir = join(root, entry.name)
    const hasJson = readdirSync(dir).some((f) => f.endsWith('.json'))
    if (hasJson) locales.push(entry.name)
  }
  return locales.sort()
}

/**
 * Reads every `<area>.json` in one locale dir into a filename → parsed-JSON map.
 * @param locale the locale dir name (e.g. `en`)
 * @param messagesRoot
 */
export function readLocaleFiles(locale: string, messagesRoot?: string): Record<string, Record<string, unknown>> {
  const dir = join(resolveMessagesRoot(messagesRoot), locale)
  if (!existsSync(dir) || !statSync(dir).isDirectory()) {
    throw new Error(`No catalog directory for locale "${locale}" at ${dir}`)
  }
  const files: Record<string, Record<string, unknown>> = {}
  for (const name of readdirSync(dir)) {
    if (!name.endsWith('.json')) continue
    files[name] = JSON.parse(readFileSync(join(dir, name), 'utf8')) as Record<string, unknown>
  }
  return files
}

/**
 * Loads one locale's merged catalog from disk: its renderable messages and its
 * `@key` metadata, separated (the runtime never sees metadata).
 * @param locale the locale dir name (e.g. `en`, `en-XA`)
 * @param messagesRoot
 */
export function loadCatalog(locale: string, messagesRoot?: string): Catalog {
  return mergeCatalogFiles(readLocaleFiles(locale, messagesRoot))
}

/**
 * The structure extracted from an ICU message: placeholder/argument names,
 * `<tag>` names, and the category labels per argument, kept in SEPARATE maps for
 * `plural` vs `select`.
 */
interface MessageStructure {
  placeholders: Set<string>
  tags: Set<string>
  pluralCategories: Map<string, Set<string>>
  selectCategories: Map<string, Set<string>>
}

/**
 * One `intl-messageformat` AST element. Loosely typed (the exact shape varies by
 * `type`); we only read `type`, `value`, `options`, and `children`.
 */
interface AstElement {
  type: number
  value: string
  options?: Record<string, { value: AstElement[] }>
  children?: AstElement[]
}

/** A parsed-message analysis. See `parseMessage`. */
/**
 * Recursively walks an ICU AST, collecting placeholder/argument names, `<tag>`
 * names, and, per argument, the set of category labels used, kept in SEPARATE
 * maps for `plural` vs `select`. The distinction matters downstream: the
 * plural-coverage check compares a locale's `plural` categories against that
 * locale's required CLDR set, where `select` categories are an arbitrary,
 * message-defined enumeration that must match English exactly (and is covered by
 * placeholder/tag parity, not by CLDR coverage).
 */
function walkAst(ast: readonly AstElement[], acc: MessageStructure): void {
  for (const el of ast) {
    switch (el.type) {
      case TYPE.argument:
      case TYPE.number:
      case TYPE.date:
      case TYPE.time:
        // A simple placeholder `{name}` (optionally with a number/date/time
        // skeleton). The arg name is what a translation must preserve.
        acc.placeholders.add(el.value)
        break
      case TYPE.select:
      case TYPE.plural: {
        // `{arg, plural/select, cat {…} …}`. The arg name is a placeholder; each
        // branch label is a category; each branch body is itself an AST to walk
        // (placeholders/tags can nest inside a branch). Plural and select
        // categories go into separate maps (see the JSDoc above).
        acc.placeholders.add(el.value)
        const target = el.type === TYPE.plural ? acc.pluralCategories : acc.selectCategories
        const cats = target.get(el.value) ?? new Set<string>()
        for (const [category, branch] of Object.entries(el.options ?? {})) {
          cats.add(category)
          walkAst(branch.value, acc)
        }
        target.set(el.value, cats)
        break
      }
      case TYPE.tag:
        // `<name>…children…</name>`. Record the tag name and walk its children.
        acc.tags.add(el.value)
        walkAst(el.children ?? [], acc)
        break
      // literal (0) and pound (7) carry no structure a translation must match.
      default:
        break
    }
  }
}

/**
 * Parses one ICU message to its AST (via the runtime's `intl-messageformat`
 * engine) and extracts the structure a translation MUST preserve.
 *
 * On invalid ICU (a stray `'`/`{`/`<`, an unclosed tag, etc.) `IntlMessageFormat`
 * construction throws; this returns `{ ok: false, error }` with empty sets so
 * the ICU-validity check (M3) can flag it without crashing the run.
 *
 * @param value the ICU message string
 * @param locale locale tag for parsing (default `en`; the AST shape
 *   is locale-independent, so this only affects which CLDR plural set the engine
 *   would later use, irrelevant to structure extraction)
 */
export function parseMessage(value: string, locale = 'en'): MessageStructure & { ok: boolean; error?: string } {
  const acc: MessageStructure = {
    placeholders: new Set(),
    tags: new Set(),
    pluralCategories: new Map(),
    selectCategories: new Map(),
  }
  try {
    const ast = new IntlMessageFormat(value, locale).getAst() as unknown as readonly AstElement[]
    walkAst(ast, acc)
    return { ...acc, ok: true }
  } catch (err) {
    return { ...acc, ok: false, error: err instanceof Error ? err.message : String(err) }
  }
}

/**
 * A git-style short content hash of an English source value: the first 7 lowercase
 * hex chars of its SHA-256. Stamped into a non-`en` `@key.sourceHash` by the
 * pseudolocale generator (M1) to record which English value a translation was made
 * from, and compared by the stale check (M2): stored hash ≠ current English value's
 * hash ⇒ the translation is stale. Deterministic and git-independent (survives
 * rebases/reformats); hashes the exact string, so any byte change flips it.
 * @param englishValue the exact English message value
 * @returns 7-char lowercase hex
 */
export function sourceHash(englishValue: string): string {
  return createHash('sha256').update(englishValue, 'utf8').digest('hex').slice(0, 7)
}

/**
 * Brand/product/system WORDS that must survive translation verbatim: never
 * localized, never accented. Curated and explicit (extend deliberately, not by
 * pattern), derived from the brand glossary (`brand/copy/cmdr-copy.md`,
 * `docs/guides/branding.md`) and the product's external entities. Single source of
 * truth, shared by the don't-translate check (which warns when a translation drops
 * one) AND the pseudolocale generator (which keeps them verbatim so en-XA, a
 * faithful translation simulation, passes that check). Case-sensitive, whole-word.
 */
export const BRAND_WORDS: readonly string[] = Object.freeze([
  'Cmdr', // the product name
  'macOS', // Apple's OS name; never localized
  'GitHub', // external service
  'SMB', // protocol acronym
  'MTP', // protocol acronym
  'Tauri', // tech name (appears in About/credits)
  'Rust', // tech name
  'Svelte', // tech name
  // NOT here: "Quick Look" and other Apple FEATURE names Apple localizes per-OS
  // (fr "Coup d’œil", de "Übersicht", es "Vista rápida"). They must be translated
  // to match the user's macOS, not kept verbatim. Only Apple feature/product names
  // Apple itself keeps English (Spotlight, Mission Control, AirDrop, Siri, Time
  // Machine, Finder, ...) belong in this list — and none of those appear in copy
  // yet. See docs/guides/i18n-translation.md § Term-choice principles, principle 1.
])

/**
 * Substitution TOKENS that must appear verbatim (the raw error pipeline replaces
 * them by name with `.replaceAll('{token}', value)`). Mirrors
 * `system-strings.svelte.ts` `ENGLISH_DEFAULTS` in snake_case `{token}` form. Also
 * guarded structurally by the parity check's raw `{token}` comparison; the
 * don't-translate check lists them for a clearer, token-specific message.
 */
export const SYSTEM_TOKENS: readonly string[] = Object.freeze([
  '{system_settings}',
  '{privacy_and_security}',
  '{full_disk_access}',
  '{files_and_folders}',
  '{local_network}',
  '{appearance}',
])

/**
 * Whether `word` appears as a whole word in `value` (case-sensitive). Word
 * boundaries use lookarounds against ASCII alphanumerics, so "macOS" inside
 * "macOSes" doesn't count but "macOS." does. Shared by the don't-translate check
 * and the pseudolocale generator's brand-word protection.
 */
export function hasWholeWord(value: string, word: string): boolean {
  return wholeWordRegExp(word).test(value)
}

/**
 * A fresh whole-word matcher for `word` (global, so callers can iterate matches).
 * Regex metacharacters in `word` are escaped (none today, but safe).
 */
export function wholeWordRegExp(word: string): RegExp {
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return new RegExp(`(?<![A-Za-z0-9])${escaped}(?![A-Za-z0-9])`, 'g')
}

/**
 * Whether `word` is PRESENT in `value`, allowing a brand to take a natural
 * inflectional suffix. Inflecting languages append case/possessive endings to a
 * name (Hungarian "Cmdrben" = "in Cmdr", Swedish genitive "Cmdrs", Hungarian
 * "Cmdrről"); those are CORRECT translations, not a dropped brand. So we accept
 * `word` at a word start (no letter/digit immediately before) optionally followed
 * by a run of LOWERCASE letters (the suffix — including accented, via `\p{Ll}`
 * with the `u` flag) and then a non-letter/digit boundary.
 *
 * The suffix must be lowercase so an UPPERCASE compound ("CmdrFoo") doesn't count
 * as the brand: a new capital starts a new word, not an inflection. "MacCmdr"
 * (embedded, letter before) doesn't count either — the brand must be at a word
 * start. This is intentionally looser than `hasWholeWord`: use it for the
 * LOCALE-side presence test, where inflection is legitimate; keep `hasWholeWord`
 * for the ENGLISH side, where the brand appears bare.
 */
export function hasBrandPresent(value: string, word: string): boolean {
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return new RegExp(`(?<![\\p{L}\\p{N}])${escaped}\\p{Ll}*(?![\\p{L}\\p{N}])`, 'u').test(value)
}

/**
 * Whether a message key renders RAW (no ICU) at runtime. The `errors.*` family
 * is resolved through `getMessage()` (a raw lookup), NOT `t()`/`intl-messageformat`:
 * its `{system_settings}` substitution tokens, literal `<…>` text (e.g.
 * `<folder-path>`), markdown, and lone apostrophes deliberately bypass ICU grammar,
 * and several such values don't even parse as ICU. So the locale checks must NOT
 * run these through `parseMessage` (it would false-flag valid raw copy as invalid
 * ICU); they compare the raw `{token}` set instead. Single source of truth for the
 * raw/ICU split, reused by the pseudolocale generator and the locale checks.
 * See `src/lib/errors/CLAUDE.md` + `src/lib/intl/CLAUDE.md`.
 */
export function isRawKey(key: string): boolean {
  return key.startsWith('errors.')
}

/**
 * Extracts the set of brace-token names (`{system_settings}`, `{path}`, …) from a
 * RAW (non-ICU) message. The raw error pipeline substitutes these by name with
 * `.replaceAll('{token}', value)`, so a translation MUST preserve the exact token
 * set, exactly the role placeholder parity plays for ICU messages. Mirrors the
 * generator's `pseudoRaw` token handling: a `{…}` span (no nesting in raw error
 * tokens) is one token; everything else is literal.
 * @param value the raw English/locale message
 * @returns token names without the braces, e.g. `{ 'system_settings' }`
 */
export function rawTokens(value: string): Set<string> {
  const tokens = new Set<string>()
  const re = /\{([^{}]*)\}/g
  let match: RegExpExecArray | null
  while ((match = re.exec(value)) !== null) tokens.add(match[1])
  return tokens
}
