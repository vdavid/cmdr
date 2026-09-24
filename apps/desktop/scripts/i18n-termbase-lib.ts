/**
 * Shared, deterministic helpers for the termbase: the language-agnostic concept
 * registry (`docs/i18n/concepts.json`), each locale's rulings
 * (`docs/i18n/<tag>/terms.json`), its rationale journal (`decisions.md`), and the
 * `## Digest` of its `style.md`. Schemas and the why: `docs/i18n/termbase.md`.
 *
 * Two consumers, one vocabulary: the translation brief (`i18n-brief.ts`) decides
 * which terms are "in play" for a batch, and the termbase check
 * (`i18n-check-termbase.ts`) decides which shipped keys drift from a ruling. Both
 * must agree on what "this English copy uses this concept" means, so the matcher
 * lives here and nowhere else.
 *
 * Pure apart from the file reads (no time, no RNG, no git), with a `docsRoot`
 * override so tests point at a fixture.
 */

import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { isRawKey, visibleLiterals } from './i18n-catalog-lib.ts'

/** One recurring English concept, shared by every locale. */
export interface Concept {
  en: string
  match: string[]
  /** same syntax as `match`; a text it hits doesn't count for the concept (another English sense) */
  notMatch?: string[]
  sense: string
  distinct?: string[]
  note?: string
}

/** `concepts.json` (or a locale's `concepts-proposed.json`): concept ID → concept. */
export type Concepts = Record<string, Concept>

/** How sure a locale is of a ruling. */
export type Confidence = 'confirmed' | 'high' | 'tentative'

/** The values `Confidence` may take, for the schema check. */
export const CONFIDENCES: readonly Confidence[] = Object.freeze(['confirmed', 'high', 'tentative'])

/** A form a locale must not use, and why. */
export interface AvoidForm {
  form: string
  why: string
}

/** One locale's ruling for one concept. */
export interface Term {
  chosen: string
  accept?: string[]
  forms?: string
  avoid?: AvoidForm[]
  confidence: Confidence
  sources: string
  note?: string
  exceptions?: Record<string, string>
  /** one or more `decisions.md` headings, each exact or a unique prefix (`resolveDecision`) */
  decision?: string | string[]
}

/** A term's `decision` pointers as a list, whichever shape the file uses. */
export function decisionPointers(decision: unknown): string[] {
  if (typeof decision === 'string') return [decision]
  return Array.isArray(decision) ? decision.filter((entry): entry is string => typeof entry === 'string') : []
}

/**
 * Resolves one `decision` pointer to the heading it names: the exact heading when
 * one matches, else the only heading that starts with it. A prefix lets a pointer
 * survive the heading gaining a date or another cited key. Returns every candidate,
 * so the caller can tell "missing" (none) from "ambiguous" (several).
 */
export function resolveDecision(pointer: string, headings: Iterable<string>): string[] {
  const all = [...headings]
  if (all.includes(pointer)) return [pointer]
  return all.filter((heading) => heading.startsWith(pointer.trimEnd()))
}

/** `<tag>/terms.json`: concept ID → ruling. */
export type Termbase = Record<string, Term>

/** The `docs/i18n/` root, resolved relative to this script. Override `docsRoot` in tests. */
export function resolveDocsRoot(docsRoot?: string): string {
  if (docsRoot) return docsRoot
  // This file lives in `apps/desktop/scripts/`; the guides live in `docs/i18n/`.
  return join(import.meta.dirname, '..', '..', '..', 'docs', 'i18n')
}

/** Reads a JSON file, or `undefined` when it doesn't exist. A parse error throws, naming the file. */
export function readJsonIfPresent(path: string): unknown {
  if (!existsSync(path)) return undefined
  try {
    return JSON.parse(readFileSync(path, 'utf8')) as unknown
  } catch (error) {
    throw new Error(`Couldn't parse ${path}: ${error instanceof Error ? error.message : String(error)}`, {
      cause: error,
    })
  }
}

/** Reads a text file, or `undefined` when it doesn't exist. */
export function readTextIfPresent(path: string): string | undefined {
  return existsSync(path) ? readFileSync(path, 'utf8') : undefined
}

/** The shared registry path. */
export function conceptsPath(docsRoot?: string): string {
  return join(resolveDocsRoot(docsRoot), 'concepts.json')
}

/** A locale's proposed-concepts path (the parallel-migration staging file). */
export function proposedConceptsPath(tag: string, docsRoot?: string): string {
  return join(resolveDocsRoot(docsRoot), tag, 'concepts-proposed.json')
}

/** A locale's termbase path. */
export function termsPath(tag: string, docsRoot?: string): string {
  return join(resolveDocsRoot(docsRoot), tag, 'terms.json')
}

/** A locale's rationale-journal path. */
export function decisionsPath(tag: string, docsRoot?: string): string {
  return join(resolveDocsRoot(docsRoot), tag, 'decisions.md')
}

/** A locale's style-guide path. */
export function stylePath(tag: string, docsRoot?: string): string {
  return join(resolveDocsRoot(docsRoot), tag, 'style.md')
}

/**
 * The concepts a locale can see: the shared registry plus that locale's
 * `concepts-proposed.json`. Loose-typed on purpose: the schema is the check's job,
 * and the brief should still assemble around a half-written entry.
 */
export function loadConceptsFor(tag: string | undefined, docsRoot?: string): Concepts {
  const shared = (readJsonIfPresent(conceptsPath(docsRoot)) ?? {}) as Concepts
  if (tag === undefined) return shared
  const proposed = (readJsonIfPresent(proposedConceptsPath(tag, docsRoot)) ?? {}) as Concepts
  return { ...proposed, ...shared }
}

/** A locale's termbase, or `undefined` when it has none yet. */
export function loadTerms(tag: string, docsRoot?: string): Termbase | undefined {
  return readJsonIfPresent(termsPath(tag, docsRoot)) as Termbase | undefined
}

/**
 * Curly apostrophes and quotes as straight ones, one character for one (so match
 * offsets survive). Copy uses both (`isn’t` in French, `isn't` in a pattern), and
 * neither side should have to guess which.
 */
export function straightQuotes(text: string): string {
  return text.replace(/[‘’‚‛]/g, "'").replace(/[“”„‟]/g, '"')
}

/** Regex metacharacters, escaped. */
function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * Compiles a concept's `match` forms into one predicate over English copy:
 * case-insensitive, whole-word (a letter or digit on either side breaks the
 * match, accented letters included), any whitespace run standing in for a space in
 * a phrase, and a trailing `*` meaning "this prefix".
 */
export function compileMatch(forms: readonly string[]): (text: string) => boolean {
  const trimmed = forms.map((form) => straightQuotes(form.trim())).filter((form) => form.length > 0)
  const wholeValues = new Set(trimmed.filter((form) => form.startsWith('=')).map((form) => wholeValueOf(form.slice(1))))
  const alternatives = trimmed
    .filter((form) => !form.startsWith('='))
    .map((form) => {
      const prefix = form.endsWith('*')
      const body = (prefix ? form.slice(0, -1) : form).split(/\s+/).map(escapeRegExp).join('\\s+')
      return prefix ? body : `${body}(?![\\p{L}\\p{N}])`
    })
  const re = alternatives.length > 0 ? new RegExp(`(?<![\\p{L}\\p{N}])(?:${alternatives.join('|')})`, 'iu') : undefined
  return (raw) => {
    const text = straightQuotes(raw)
    return (re?.test(text) ?? false) || (wholeValues.size > 0 && wholeValues.has(wholeValueOf(text)))
  }
}

/**
 * The character spans of `text` that any of `forms` covers: every hit of a word or
 * phrase form, or the whole text for a matching `=` form. The brief uses it to tell
 * which content words of a key no concept speaks for.
 */
export function coveredSpans(forms: readonly string[], raw: string): [number, number][] {
  // One character for one, so the spans still index `raw`.
  const text = straightQuotes(raw)
  const spans: [number, number][] = []
  const whole = forms.filter((form) => form.trim().startsWith('='))
  if (whole.length > 0 && compileMatch(whole)(text)) spans.push([0, text.length])
  for (const form of forms.map((f) => straightQuotes(f.trim())).filter((f) => f.length > 0 && !f.startsWith('='))) {
    const prefix = form.endsWith('*')
    const body = (prefix ? form.slice(0, -1) : form).split(/\s+/).map(escapeRegExp).join('\\s+')
    const re = new RegExp(`(?<![\\p{L}\\p{N}])${body}${prefix ? '[\\p{L}\\p{N}]*' : '(?![\\p{L}\\p{N}])'}`, 'giu')
    for (const hit of text.matchAll(re)) spans.push([hit.index, hit.index + hit[0].length])
  }
  return spans
}

/** Edge punctuation and whitespace a whole-value (`=back`) match ignores: `Back…` and ` Back ` are `back`. */
const WHOLE_VALUE_EDGES = /^[\s\p{P}]+|[\s\p{P}]+$/gu

/** Normalizes a text or a `=form` body for whole-value comparison. */
function wholeValueOf(text: string): string {
  return text.replace(WHOLE_VALUE_EDGES, '').replace(/\s+/gu, ' ').toLowerCase()
}

/**
 * The English copy a concept is matched against: what the reader sees, with
 * placeholder names, plural/select categories, and tag names removed (a
 * `{count, plural, …}` must not read as the word "count"). A raw-family value, or
 * one that isn't valid ICU, drops its `{token}` spans instead. ICU's doubled
 * apostrophe reads as one. Markdown code spans go too: `` `ping <hostname>` `` in
 * an error suggestion is a command the reader types verbatim, so the locale keeps
 * it English and it says nothing about the concept "hostname".
 */
export function englishMatchText(key: string, value: string): string {
  const literals = isRawKey(key) ? undefined : visibleLiterals(value)
  const text = (literals ?? stripRawIdentifiers(value)).replace(/`[^`\n]*`/g, '')
  return isRawKey(key) ? text : text.replace(/''/g, "'")
}

/**
 * The identifier-shaped parts of a value the ICU engine didn't parse (a raw
 * family, or invalid ICU): `{token}` spans, `<name>` / `</name>` tags, and markdown
 * link targets (`](x-apple.systempreferences:…)`). None of them is copy, and a
 * concept like "name" or "folder" would otherwise match them dozens of times.
 */
function stripRawIdentifiers(value: string): string {
  return value
    .replace(/\{[^{}]*\}/g, '')
    .replace(/<\/?[A-Za-z][\w-]*>/g, '')
    .replace(/\]\([^)\s]*\)/g, ']')
}

/** Compiled matchers, one per concept, reused across a run. */
export type ConceptMatchers = Map<string, (text: string) => boolean>

/**
 * Compiles every concept's matcher once: its `match` hits AND its `notMatch`
 * doesn't. `notMatch` carries the English-sense exclusions ("come back" is not the
 * Back button) once for every locale, rather than as the same `exceptions` entry
 * repeated in each `terms.json`.
 */
export function compileConcepts(concepts: Concepts): ConceptMatchers {
  const matchers: ConceptMatchers = new Map()
  for (const [id, concept] of Object.entries(concepts)) {
    const hits = compileMatch(Array.isArray(concept.match) ? concept.match : [])
    const excluded = compileMatch(Array.isArray(concept.notMatch) ? concept.notMatch : [])
    matchers.set(id, (text) => hits(text) && !excluded(text))
  }
  return matchers
}

/** Every concept ID whose `match` hits `text`, sorted. */
export function conceptsInText(concepts: Concepts | ConceptMatchers, text: string): string[] {
  const matchers = concepts instanceof Map ? concepts : compileConcepts(concepts)
  const hits: string[] = []
  for (const [id, hit] of matchers) if (hit(text)) hits.push(id)
  return hits.sort()
}

/**
 * Whether a locale value uses a ruling: `chosen` or any `accept` form as a
 * case-insensitive substring of its visible copy. Substring on purpose, since
 * compounding and inflection (`Bewerkingenwachtrij`, `Cmdrben`) are exactly what a
 * whole-word test would miss.
 */
export function localeValueCarriesTerm(tag: string, value: string, term: Pick<Term, 'chosen' | 'accept'>): boolean {
  // The fallback (raw family, invalid ICU) still carries ICU's doubled apostrophe
  // on ICU-family keys, so `foto''s` has to read as `foto's` for the accept form.
  const visible = visibleLiterals(value, tag) ?? stripRawIdentifiers(value).replace(/''/g, "'")
  const text = straightQuotes(visible).toLocaleLowerCase(tag)
  const forms = [term.chosen, ...(term.accept ?? [])].filter((form) => typeof form === 'string' && form.length > 0)
  return forms.some((raw) => {
    const form = straightQuotes(raw).toLocaleLowerCase(tag)
    if (!form.endsWith('*')) return text.includes(form)
    // An explicit prefix: the form has to START a word, where a bare form may sit anywhere.
    return new RegExp(`(?<![\\p{L}\\p{N}])${escapeRegExp(form.slice(0, -1))}`, 'u').test(text)
  })
}

/** One `##` or `###` section of a `decisions.md`. */
export interface DecisionSection {
  /** heading text without the `#` marks */
  heading: string
  level: 2 | 3
  /** 1-based line of the heading */
  line: number
  /** everything under the heading up to the next heading of the same or a higher level */
  body: string
  /** the keys the heading cites, relative forms resolved, wildcards kept */
  citations: string[]
}

/** A heading line (`## x` or `### x`), outside a code fence. */
const HEADING = /^(#{2,3})\s+(.*\S)\s*$/

/** One backticked span. */
const BACKTICKED = /`([^`\n]+)`/g

/** A key segment, optionally ending in a `*` wildcard. */
const CITATION_SEGMENT = /^[A-Za-z_][A-Za-z0-9_-]*\*?$|^\*$/

/**
 * Pulls the key citations out of a heading. Headings cite keys in backticks and
 * abbreviate siblings: `` `settings.analytics.enabled.label`/`.description` `` or
 * `` `queue.row.statusAwaitingAnswer`/`awaitingAnswerTooltip` ``. A span that starts
 * with a dot, or is a single bare segment, replaces the same number of trailing
 * segments of the previous citation. A single segment with nothing before it
 * (`` `inspectFile` ``) isn't a citation.
 */
export function headingCitations(heading: string): string[] {
  const citations: string[] = []
  for (const match of heading.matchAll(BACKTICKED)) {
    const token = match[1].trim()
    const relative = token.startsWith('.')
    const segments = (relative ? token.slice(1) : token).split('.')
    if (!segments.every((segment) => CITATION_SEGMENT.test(segment))) continue
    if (relative || segments.length === 1) {
      const previous = citations.at(-1)
      if (previous === undefined) continue
      const base = previous.split('.')
      if (base.length <= segments.length) continue
      citations.push([...base.slice(0, base.length - segments.length), ...segments].join('.'))
      continue
    }
    citations.push(token)
  }
  return citations
}

/** Splits a `decisions.md` into its `##` / `###` sections. Headings inside code fences don't count. */
export function parseDecisions(markdown: string): DecisionSection[] {
  const lines = markdown.split('\n')
  const heads: { level: 2 | 3; index: number; heading: string }[] = []
  let fenced = false
  for (const [index, line] of lines.entries()) {
    if (/^\s*(```|~~~)/.test(line)) fenced = !fenced
    if (fenced) continue
    const match = HEADING.exec(line)
    if (match) heads.push({ level: match[1].length as 2 | 3, index, heading: match[2] })
  }
  return heads.map((head, position) => {
    const next = heads.slice(position + 1).find((other) => other.level <= head.level)
    const end = next ? next.index : lines.length
    return {
      heading: head.heading,
      level: head.level,
      line: head.index + 1,
      body: lines
        .slice(head.index + 1, end)
        .join('\n')
        .trim(),
      citations: headingCitations(head.heading),
    }
  })
}

/** Compiles one citation to a matcher over whole keys. See `sectionCitesKey`. */
function citationMatcher(citation: string, namespaces: ReadonlySet<string>): RegExp {
  const pattern = citation.split('*').map(escapeRegExp).join('[A-Za-z0-9_.-]*')
  const anchoredAtNamespace = namespaces.has(citation.split('.')[0])
  return new RegExp(anchoredAtNamespace ? `^${pattern}$` : `(?:^|\\.)${pattern}$`)
}

/**
 * Whether any of a section's citations covers `key`: the exact key, a `*` wildcard
 * (`queue.*`, `errors.mutation.trash*`), or, for a citation that doesn't start at a
 * real namespace (`rollbackConfirm.body`), a segment-aligned suffix of the key. A
 * citation that does start at a namespace is anchored there, so `servers.*` never
 * claims `settings.servers.title`.
 *
 * @param namespaces every English key's first segment
 */
export function sectionCitesKey(citations: readonly string[], key: string, namespaces: ReadonlySet<string>): boolean {
  return citations.some((citation) => citationMatcher(citation, namespaces).test(key))
}

/**
 * The `## Digest` section of a style guide: everything after that heading up to the
 * next `##`, trimmed. `undefined` when the guide has none.
 */
export function extractDigest(styleMarkdown: string): string | undefined {
  const lines = styleMarkdown.split('\n')
  const start = lines.findIndex((line) => /^##\s+Digest\s*$/.test(line))
  if (start === -1) return undefined
  let fenced = false
  let end = lines.length
  for (let index = start + 1; index < lines.length; index++) {
    if (/^\s*(```|~~~)/.test(lines[index])) fenced = !fenced
    if (!fenced && /^##\s/.test(lines[index])) {
      end = index
      break
    }
  }
  return lines
    .slice(start + 1, end)
    .join('\n')
    .trim()
}
