/**
 * Shared, language-agnostic catalog + ICU helper for the i18n maintenance
 * tooling: the pseudolocale generator, the stale check, and the locale-parity /
 * ICU-validity / plural-coverage checks all build on this single module.
 *
 * Three responsibilities, all pure/deterministic (no app/runtime imports beyond
 * the ICU parser, no `window`/DOM, no time/RNG):
 *
 *  1. Catalog I/O — load a locale's merged catalog (`messages/<locale>/*.json`),
 *     stripping the ARB-style `@key` metadata into a separate map; enumerate the
 *     available locales and each locale's key set.
 *  2. ICU parsing — parse each message to its AST with the SAME engine the
 *     runtime uses (`intl-messageformat`, see below), then extract the structure
 *     a translation must preserve: placeholder/argument names, `<tag>` names, and
 *     the `plural`/`select` categories per argument. A single source of truth for
 *     "what tokens/structure does this message have".
 *  3. Source hashing — `sourceHash(value)`: a git-style 7-char hex of an English
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

/**
 * Whether a catalog entry key is ARB-style `@key` metadata (stripped before the
 * runtime/codegen see it). Mirrors the runtime's `stripMetadata`.
 * @param {string} key
 * @returns {boolean}
 */
export function isMetadataKey(key) {
  return key.startsWith('@')
}

/**
 * Splits one parsed catalog file into renderable messages and `@key` metadata.
 * The runtime keeps only string-valued non-`@` entries as messages; everything
 * `@`-prefixed is metadata, keyed WITHOUT the leading `@` so it lines up with
 * its message key.
 * @param {Record<string, unknown>} raw a parsed `<area>.json`
 * @returns {{ messages: Record<string, string>, metadata: Record<string, Record<string, unknown>> }}
 */
export function splitCatalogFile(raw) {
  /** @type {Record<string, string>} */
  const messages = {}
  /** @type {Record<string, Record<string, unknown>>} */
  const metadata = {}
  for (const [key, value] of Object.entries(raw)) {
    if (isMetadataKey(key)) {
      const messageKey = key.slice(1)
      if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
        metadata[messageKey] = /** @type {Record<string, unknown>} */ (value)
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
 * @param {Record<string, Record<string, unknown>>} files
 * @returns {{ messages: Record<string, string>, metadata: Record<string, Record<string, unknown>> }}
 */
export function mergeCatalogFiles(files) {
  /** @type {Record<string, string>} */
  const messages = {}
  /** @type {Record<string, Record<string, unknown>>} */
  const metadata = {}
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
 * @param {string} [messagesRoot]
 * @returns {string}
 */
export function resolveMessagesRoot(messagesRoot) {
  if (messagesRoot) return messagesRoot
  // This file lives in `apps/desktop/scripts/`; messages live in
  // `apps/desktop/src/lib/intl/messages/`.
  return join(import.meta.dirname, '..', 'src', 'lib', 'intl', 'messages')
}

/**
 * Reserved sibling directories under `messages/` that are NOT locales. Today
 * just `screenshots/` (capture artifacts; it holds `*.json` so a "has JSON"
 * heuristic alone would misclassify it).
 * @type {Set<string>}
 */
export const NON_LOCALE_DIRS = new Set(['screenshots'])

/**
 * Lists the locale directories under `messages/` (each holding `<area>.json`
 * files), sorted. A locale is any direct subdirectory that holds at least one
 * `*.json` and isn't a reserved non-locale dir (`NON_LOCALE_DIRS`).
 * @param {string} [messagesRoot]
 * @returns {string[]} BCP-47-ish locale tags (the dir names), e.g. `['en', 'en-XA']`
 */
export function listLocales(messagesRoot) {
  const root = resolveMessagesRoot(messagesRoot)
  /** @type {string[]} */
  const locales = []
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
 * @param {string} locale the locale dir name (e.g. `en`)
 * @param {string} [messagesRoot]
 * @returns {Record<string, Record<string, unknown>>}
 */
export function readLocaleFiles(locale, messagesRoot) {
  const dir = join(resolveMessagesRoot(messagesRoot), locale)
  if (!existsSync(dir) || !statSync(dir).isDirectory()) {
    throw new Error(`No catalog directory for locale "${locale}" at ${dir}`)
  }
  /** @type {Record<string, Record<string, unknown>>} */
  const files = {}
  for (const name of readdirSync(dir)) {
    if (!name.endsWith('.json')) continue
    files[name] = JSON.parse(readFileSync(join(dir, name), 'utf8'))
  }
  return files
}

/**
 * Loads one locale's merged catalog from disk: its renderable messages and its
 * `@key` metadata, separated (the runtime never sees metadata).
 * @param {string} locale the locale dir name (e.g. `en`, `en-XA`)
 * @param {string} [messagesRoot]
 * @returns {{ messages: Record<string, string>, metadata: Record<string, Record<string, unknown>> }}
 */
export function loadCatalog(locale, messagesRoot) {
  return mergeCatalogFiles(readLocaleFiles(locale, messagesRoot))
}

/** A parsed-message analysis. See `parseMessage`. */
/**
 * Recursively walks an ICU AST, collecting placeholder/argument names, `<tag>`
 * names, and, per `plural`/`select` argument, the set of category labels used.
 * @param {readonly any[]} ast
 * @param {{ placeholders: Set<string>, tags: Set<string>, pluralCategories: Map<string, Set<string>> }} acc
 */
function walkAst(ast, acc) {
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
        // (placeholders/tags can nest inside a branch).
        acc.placeholders.add(el.value)
        const cats = acc.pluralCategories.get(el.value) ?? new Set()
        for (const [category, branch] of Object.entries(el.options)) {
          cats.add(category)
          walkAst(/** @type {any} */ (branch).value, acc)
        }
        acc.pluralCategories.set(el.value, cats)
        break
      }
      case TYPE.tag:
        // `<name>…children…</name>`. Record the tag name and walk its children.
        acc.tags.add(el.value)
        walkAst(el.children, acc)
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
 * @param {string} value the ICU message string
 * @param {string} [locale] locale tag for parsing (default `en`; the AST shape
 *   is locale-independent, so this only affects which CLDR plural set the engine
 *   would later use — irrelevant to structure extraction)
 * @returns {{
 *   placeholders: Set<string>,
 *   tags: Set<string>,
 *   pluralCategories: Map<string, Set<string>>,
 *   ok: boolean,
 *   error?: string,
 * }}
 */
export function parseMessage(value, locale = 'en') {
  /** @type {{ placeholders: Set<string>, tags: Set<string>, pluralCategories: Map<string, Set<string>> }} */
  const acc = { placeholders: new Set(), tags: new Set(), pluralCategories: new Map() }
  try {
    const ast = new IntlMessageFormat(value, locale).getAst()
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
 * @param {string} englishValue the exact English message value
 * @returns {string} 7-char lowercase hex
 */
export function sourceHash(englishValue) {
  return createHash('sha256').update(englishValue, 'utf8').digest('hex').slice(0, 7)
}
