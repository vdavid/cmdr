#!/usr/bin/env node
/**
 * Deterministic PSEUDOLOCALE generator (the universal i18n test fixture).
 *
 * For every `messages/en/<area>.json` key it emits an accented, ~+40%-longer
 * value into `messages/en-XA/<area>.json` while PRESERVING EXACTLY: every
 * `{placeholder}` arg name, every `<tag>`/`</tag>` name, and all ICU
 * `plural`/`select`/`#` structure and category keywords (`one`, `other`, `=0`,
 * …). Only the literal human-readable text between/inside those is transformed.
 * Each key also gets a `@key.sourceHash` (from the M0 `sourceHash()` of the
 * English value), so the pseudolocale is a valid translated locale that the
 * stale check (M2) sees as fresh.
 *
 * ## What the pseudolocale is for
 *
 *  1. Overflow testing: an accented, deliberately-longer string makes clipping
 *     and layout breakage visible when the app is driven in this locale and
 *     screenshotted. Driving the app in `en-XA` needs the runtime locale
 *     resolver (lands with the first real locale); the screenshots driver gains
 *     a planned `--locale` axis then. For now `en-XA` is generated for the
 *     checks + future overflow, not yet rendered in-app.
 *  2. Check fixture: it's a complete, structurally-faithful non-`en` locale, so
 *     M2 (stale) and M3 (parity / ICU / plural / key) checks can run against a
 *     real locale before any human translation exists — clean run passes, a
 *     corrupted copy fails. A small committed hand-verifiable SUBSET lives in
 *     `test/fixtures/i18n-pseudolocale/` (the full `en-XA/` dir is gitignored).
 *
 * ## Determinism
 *
 * Same English in → byte-identical pseudo out. No RNG, no time: the accent map
 * is a fixed table and the expansion filler is derived deterministically from
 * the text length. Re-running is a no-op diff.
 *
 * ## Two transform paths (ICU vs raw)
 *
 * Most messages render through ICU (`t()` → `intl-messageformat`); the entire
 * `errors.*` family renders RAW through `getMessage()` (see
 * `src/lib/errors/CLAUDE.md` + `src/lib/intl/CLAUDE.md`): its `{system_settings}`
 * tokens, literal `<…>` (e.g. `<folder-path>`), markdown, and lone apostrophes
 * deliberately bypass ICU grammar, and several such values don't even parse as
 * ICU. So:
 *  - ICU path (every non-`errors.*` key): walk the ICU AST and transform only
 *    literal text nodes, then serialize back. Robust against nested plurals.
 *  - Raw path (every `errors.*` key): a tokenizer that accents only runs of
 *    ASCII letters, leaving every `{token}`, `<…>`, backtick/`**`/`\n`/`-`
 *    markdown char, and punctuation untouched — preserving the raw-pipeline
 *    `.replaceAll('{token}', …)` substitution targets exactly.
 *
 * The acceptance bar both paths must clear (asserted in the tests): for an ICU
 * key, `parseMessage(pseudo)` token sets (placeholders, tags, plural/select
 * categories) equal `parseMessage(en)`'s; for a raw key, the set of `{…}`
 * brace-tokens is preserved.
 *
 * Run: `pnpm i18n:pseudo` (desktop) / `pnpm i18n:pseudo` (root delegator).
 */

import { mkdirSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { IntlMessageFormat } from 'intl-messageformat'
import {
  BRAND_WORDS,
  isMetadataKey,
  isRawKey,
  readLocaleFiles,
  resolveMessagesRoot,
  sourceHash,
  wholeWordRegExp,
} from './i18n-catalog-lib.js'

// `isRawKey` is the shared raw/ICU split (single source: `i18n-catalog-lib.js`).
// Re-exported here because it's part of this module's tested surface and reads
// naturally alongside the pseudo value routing below.
export { isRawKey }

/** The generated pseudolocale tag (Google's accented-expanded convention; a BCP-47 private-use region). */
export const PSEUDO_LOCALE = 'en-XA'

/** The source locale the pseudolocale is derived from. */
const SOURCE_LOCALE = 'en'

/** AST element `type` constants (the `@formatjs` `TYPE` enum; mirrors `i18n-catalog-lib.js`). */
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
 * The ASCII→accented map. A fixed, deterministic table covering every ASCII
 * letter with a visually-similar accented/decorated glyph, so the output reads
 * as the same words (overflow + pseudo-translation are obvious) without being a
 * real language. Letters outside the table (none, here) and all non-letters pass
 * through untouched, which is what preserves markdown, digits, and punctuation.
 * @type {Readonly<Record<string, string>>}
 */
const ACCENT_MAP = Object.freeze({
  a: 'á',
  b: 'ḅ',
  c: 'ç',
  d: 'ḋ',
  e: 'é',
  f: 'ḟ',
  g: 'ǧ',
  h: 'ḣ',
  i: 'í',
  j: 'ǰ',
  k: 'ḱ',
  l: 'ļ',
  m: 'ṁ',
  n: 'ñ',
  o: 'ö',
  p: 'ṗ',
  q: 'ǫ',
  r: 'ŕ',
  s: 'š',
  t: 'ţ',
  u: 'ü',
  v: 'ṽ',
  w: 'ŵ',
  x: 'ẋ',
  y: 'ý',
  z: 'ž',
  A: 'Á',
  B: 'Ḅ',
  C: 'Ç',
  D: 'Ḋ',
  E: 'É',
  F: 'Ḟ',
  G: 'Ǧ',
  H: 'Ḣ',
  I: 'Í',
  J: 'J̌',
  K: 'Ḱ',
  L: 'Ļ',
  M: 'Ṁ',
  N: 'Ñ',
  O: 'Ö',
  P: 'Ṗ',
  Q: 'Ǫ',
  R: 'Ŕ',
  S: 'Š',
  T: 'Ţ',
  U: 'Ü',
  V: 'Ṽ',
  W: 'Ŵ',
  X: 'Ẋ',
  Y: 'Ý',
  Z: 'Ž',
})

/** Bracket markers wrapping each transformed text segment, making expansion + segment bounds visible. */
const OPEN = '⟦'
const CLOSE = '⟧'

/**
 * The deterministic filler alphabet for length expansion. Accented vowels (no
 * letters/digits/markup) so the padding is unmistakably pseudo and never looks
 * like a real word or collides with a placeholder/markdown char.
 */
const FILLER = 'áéíöü'

/** Target expansion ratio: pseudo text length ≈ +40% of the source text. */
const EXPANSION = 0.4

/**
 * Accents one string of literal text: ASCII letters → their accented twin via
 * `ACCENT_MAP`; everything else (spaces, digits, punctuation, markdown chars,
 * already-accented glyphs) is left exactly as-is.
 * @param {string} text
 * @returns {string}
 */
function accent(text) {
  let out = ''
  for (const ch of text) out += ACCENT_MAP[ch] ?? ch
  return out
}

/**
 * A deterministic filler run whose length depends only on `seedLen` (the source
 * segment's length), so the same input always yields the same padding. The run
 * cycles through `FILLER`; its visible character count is `ceil(seedLen * 0.4)`,
 * giving the segment ~+40% length. Empty/whitespace-only segments get no filler
 * (nothing meaningful to expand, and we must not invent text inside e.g. a
 * single-space literal between placeholders).
 * @param {string} segment the original literal segment (used for its length + a position-independent seed)
 * @returns {string}
 */
function filler(segment) {
  const visibleLen = [...segment.replace(/\s/g, '')].length
  if (visibleLen === 0) return ''
  const padCount = Math.ceil(visibleLen * EXPANSION)
  let run = ''
  for (let i = 0; i < padCount; i++) run += FILLER[i % FILLER.length]
  return run
}

/**
 * Splits a literal segment into brand-word spans (kept verbatim) and other spans
 * (to be accented/expanded). A faithful translation never localizes a brand/system
 * word (`Cmdr`, `macOS`, …; see `BRAND_WORDS`), and keeping them verbatim is what
 * lets en-XA pass the don't-translate check. Returns parts in source order, each
 * tagged `protected` (a brand word) or not.
 * @param {string} text
 * @returns {{ text: string, protected: boolean }[]}
 */
function splitBrandWords(text) {
  // One alternation of all brand words, whole-word. Build from each word's matcher
  // source so escaping/boundaries stay identical to the check's matching.
  const alternation = BRAND_WORDS.map((w) => wholeWordRegExp(w).source).join('|')
  const re = new RegExp(alternation, 'g')
  /** @type {{ text: string, protected: boolean }[]} */
  const parts = []
  let last = 0
  let m
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) parts.push({ text: text.slice(last, m.index), protected: false })
    parts.push({ text: m[0], protected: true })
    last = m.index + m[0].length
  }
  if (last < text.length) parts.push({ text: text.slice(last), protected: false })
  return parts
}

/**
 * Accents one non-brand span and appends a bracketed, deterministic filler run to
 * expand its length. The brackets wrap only the filler (not the whole span) so
 * adjacent placeholders stay adjacent to the real (accented) words, which is what
 * overflow testing wants. A span with no visible characters (pure whitespace) is
 * accented only (no brackets), preserving spacing around placeholders.
 * @param {string} text
 * @returns {string}
 */
function transformSpan(text) {
  const accented = accent(text)
  const pad = filler(text)
  if (pad === '') return accented
  // Keep any trailing whitespace OUTSIDE the bracket so word spacing survives.
  const trailingWs = /\s+$/.exec(text)?.[0] ?? ''
  const core = trailingWs ? accented.slice(0, accented.length - trailingWs.length) : accented
  return `${core}${OPEN}${pad}${CLOSE}${trailingWs}`
}

/**
 * Transforms ONE literal text segment: brand/system words (`BRAND_WORDS`) pass
 * through verbatim; every other span is accented and length-expanded. So the
 * pseudolocale reads as a translated, overflow-stressed string while preserving
 * the don't-translate tokens a real translator would keep.
 * @param {string} text
 * @returns {string}
 */
function transformText(text) {
  return splitBrandWords(text)
    .map((part) => (part.protected ? part.text : transformSpan(part.text)))
    .join('')
}

/**
 * Serializes one ICU AST element back to ICU source, transforming only literal
 * text (type 0) nodes. Placeholder names, tag names, plural/select keywords, and
 * `#` are emitted verbatim. We write our own serializer (rather than importing
 * `@formatjs`'s `printAST`) because that parser package isn't resolvable by bare
 * specifier under pnpm's strict layout (see `i18n-catalog-lib.js` header), and
 * writing it gives full control over apostrophe/`#` escaping. Correctness is
 * proven by the round-trip test (token-set equality + `ok:true`).
 * @param {any} el one AST element
 * @returns {string}
 */
function serializeElement(el) {
  switch (el.type) {
    case TYPE.literal:
      return escapeLiteral(transformText(el.value))
    case TYPE.argument:
      return `{${el.value}}`
    case TYPE.number:
      return serializeArgWithStyle(el, 'number')
    case TYPE.date:
      return serializeArgWithStyle(el, 'date')
    case TYPE.time:
      return serializeArgWithStyle(el, 'time')
    case TYPE.pound:
      return '#'
    case TYPE.tag:
      return `<${el.value}>${serializeAst(el.children)}</${el.value}>`
    case TYPE.select:
      return serializePluralOrSelect(el, 'select')
    case TYPE.plural:
      return serializePluralOrSelect(el, el.pluralType === 'ordinal' ? 'selectordinal' : 'plural')
    default:
      throw new Error(`Unknown ICU AST element type: ${String(el.type)}`)
  }
}

/**
 * Escapes ICU-significant characters in already-transformed literal text so the
 * serialized message re-parses to the same structure. ICU treats `{`, `}`, `#`
 * (inside a plural), and `<` as syntax and `'` as an escape introducer. Our
 * transformed text contains none of `{}<#` (the accent map leaves them as-is,
 * but English literal segments never contain a bare ICU-syntax brace — those are
 * placeholders, which are separate AST nodes), so the only character we must
 * guard is the apostrophe: per the ICU rule, a lone `'` is doubled to `''`,
 * which always renders as a single `'`.
 * @param {string} text transformed literal text
 * @returns {string}
 */
function escapeLiteral(text) {
  return text.replaceAll("'", "''")
}

/**
 * Serializes a `{name, number|date|time, style?}` element, preserving any style.
 * The catalog uses no styled number/date/time placeholders today (counts are
 * preformatted `*Text` strings), but this keeps the serializer total + robust.
 * A string style is emitted verbatim; a parsed skeleton object can't be
 * faithfully reconstructed from the parser's output, so it's an explicit error
 * rather than a silent corruption.
 * @param {any} el
 * @param {'number'|'date'|'time'} kind
 * @returns {string}
 */
function serializeArgWithStyle(el, kind) {
  if (el.style == null) return `{${el.value}, ${kind}}`
  if (typeof el.style === 'string') return `{${el.value}, ${kind}, ${el.style}}`
  throw new Error(
    `Cannot serialize a parsed ${kind} skeleton for {${el.value}}; the catalog should use string styles only.`,
  )
}

/**
 * Serializes a `plural`/`selectordinal`/`select` element: the arg name, the
 * keyword, the optional plural `offset:N`, then each category branch verbatim
 * (`one {…}`, `=0 {…}`, `other {…}`) with its body recursively serialized.
 * @param {any} el
 * @param {'plural'|'selectordinal'|'select'} keyword
 * @returns {string}
 */
function serializePluralOrSelect(el, keyword) {
  const offset = keyword !== 'select' && el.offset ? ` offset:${String(el.offset)}` : ''
  let branches = ''
  for (const [category, branch] of Object.entries(el.options)) {
    branches += ` ${category} {${serializeAst(/** @type {any} */ (branch).value)}}`
  }
  return `{${el.value}, ${keyword},${offset}${branches}}`
}

/**
 * Serializes a full AST (array of elements) back to an ICU message string.
 * @param {readonly any[]} ast
 * @returns {string}
 */
function serializeAst(ast) {
  let out = ''
  for (const el of ast) out += serializeElement(el)
  return out
}

/**
 * Transforms one ICU message via the AST path: parse → transform literal nodes →
 * serialize. Throws if the value doesn't parse as ICU (a real bug for a non-error
 * key; surfaced loudly rather than silently passed through).
 * @param {string} value the English ICU message
 * @returns {string} the pseudo ICU message
 */
export function pseudoIcu(value) {
  const ast = new IntlMessageFormat(value, SOURCE_LOCALE).getAst()
  return serializeAst(ast)
}

/**
 * Transforms one RAW (non-ICU) message via the tokenizer path: accent + expand
 * only runs of ASCII letters, leaving every `{token}`, `<…>`, markdown char,
 * digit, and punctuation untouched. Used for `errors.*` (rendered raw through
 * `getMessage()` + `interpolate()` + `expandSystemStrings()`), where `<…>` is
 * literal text and `{token}` is a `.replaceAll` substitution target.
 *
 * "Run of ASCII letters" = a maximal sequence of `[A-Za-z]`. Each run is its own
 * text segment (accented + bracket-expanded); everything between runs (spaces,
 * `{tokens}`, `<…>`, backticks, `**`, `\n`, `-`, digits) passes through. This
 * provably never touches a `{…}` token: `{`, `}`, and the token name's letters
 * are inside braces, but we transform a letter RUN, and a run that's part of a
 * token would alter the token — so we must NOT transform letters inside `{…}`.
 * We therefore skip whole `{…}` spans explicitly.
 * @param {string} value the raw English message
 * @returns {string} the pseudo raw message
 */
export function pseudoRaw(value) {
  let out = ''
  let i = 0
  while (i < value.length) {
    const ch = value[i]
    if (ch === '{') {
      // Copy the whole `{…}` token verbatim (no nesting in raw error tokens).
      const end = value.indexOf('}', i)
      if (end === -1) {
        out += value.slice(i)
        break
      }
      out += value.slice(i, end + 1)
      i = end + 1
      continue
    }
    if (/[A-Za-z]/.test(ch)) {
      // Consume a maximal ASCII-letter run and transform it as one text segment.
      let j = i
      while (j < value.length && /[A-Za-z]/.test(value[j])) j++
      out += transformText(value.slice(i, j))
      i = j
      continue
    }
    out += ch
    i++
  }
  return out
}

/**
 * Produces the pseudolocale value for one message, choosing the ICU or raw path
 * by key.
 * @param {string} key the message key
 * @param {string} value the English value
 * @returns {string}
 */
export function pseudoValue(key, value) {
  return isRawKey(key) ? pseudoRaw(value) : pseudoIcu(value)
}

/**
 * Builds the pseudolocale content for ONE area file: every English message → its
 * pseudo value, each paired with an `@key.sourceHash` of the English value, so a
 * locale's catalog file is interleaved `key` / `@key` exactly like `en` but with
 * only the maintenance metadata the stale check needs. Deterministic and
 * order-stable (keys in source order).
 * @param {Record<string, unknown>} rawEnFile a parsed `en/<area>.json`
 * @returns {Record<string, unknown>}
 */
export function buildPseudoFile(rawEnFile) {
  /** @type {Record<string, unknown>} */
  const out = {}
  for (const [key, value] of Object.entries(rawEnFile)) {
    if (isMetadataKey(key) || typeof value !== 'string') continue
    out[key] = pseudoValue(key, value)
    out[`@${key}`] = { sourceHash: sourceHash(value) }
  }
  return out
}

/**
 * Generates the full `en-XA/` pseudolocale from `en/`, writing one file per area.
 * @param {string} [messagesRoot] override the `messages/` root (for tests)
 * @returns {{ files: number, keys: number }}
 */
export function generatePseudolocale(messagesRoot) {
  const root = resolveMessagesRoot(messagesRoot)
  const enFiles = readLocaleFiles(SOURCE_LOCALE, root)
  const outDir = join(root, PSEUDO_LOCALE)
  mkdirSync(outDir, { recursive: true })
  let keys = 0
  let files = 0
  for (const name of Object.keys(enFiles).sort()) {
    const pseudo = buildPseudoFile(enFiles[name])
    writeFileSync(join(outDir, name), JSON.stringify(pseudo, null, 2) + '\n', 'utf8')
    files++
    keys += Object.keys(pseudo).filter((k) => !isMetadataKey(k)).length
  }
  return { files, keys }
}

// Stale `en-XA` files from a renamed/removed `en` area would linger; warn so the
// operator prunes them (rare — area files map 1:1 to `en`).
/**
 * Names of `en-XA/*.json` files with no matching `en/*.json` (orphans to prune).
 * @param {string} [messagesRoot]
 * @returns {string[]}
 */
function orphanPseudoFiles(messagesRoot) {
  const root = resolveMessagesRoot(messagesRoot)
  const enNames = new Set(Object.keys(readLocaleFiles(SOURCE_LOCALE, root)))
  const outDir = join(root, PSEUDO_LOCALE)
  /** @type {string[]} */
  const orphans = []
  for (const name of readdirSync(outDir)) {
    if (name.endsWith('.json') && !enNames.has(name)) orphans.push(name)
  }
  return orphans
}

// Run as a CLI (not when imported by tests).
if (import.meta.url === `file://${process.argv[1]}`) {
  const { files, keys } = generatePseudolocale()
  console.log(`Generated ${PSEUDO_LOCALE}: ${String(keys)} keys across ${String(files)} area files.`)
  const orphans = orphanPseudoFiles()
  if (orphans.length > 0) {
    console.warn(
      `\nWarning: ${String(orphans.length)} ${PSEUDO_LOCALE} file(s) have no matching en/ area and should be removed:\n` +
        orphans.map((n) => `  - ${n}`).join('\n'),
    )
  }
}
