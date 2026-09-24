/**
 * Assembles a translation brief: everything a translator needs for ONE batch of
 * keys, and nothing else. The CLI (`i18n-brief.ts`) parses flags and resolves
 * paths; this module is pure apart from file reads, so tests drive it off fixtures.
 *
 * The brief replaces "load the whole glossary": a batch of ~30 strings needs the
 * locale's style digest, the rulings for the concepts its English actually uses,
 * how nearby shipped strings were translated, and the few decisions written about
 * those keys. Every section is selected BY the batch, so its size tracks the
 * batch, not the history of the locale. Sections, in order:
 *
 *  - header: languages, how the keys were chosen, the reference pile, the style guide
 *  - instructions: the translator's standing instructions, single-sourced from
 *    `docs/i18n/translator-instructions.md` and rendered for the language(s)
 *  - digest: the `## Digest` section of each language's style guide
 *  - keys: key, English, `@key` description, placeholders/tags, current value(s),
 *    and the content words no concept covers ("No concept yet")
 *  - terms: every concept whose `match` hits a batch key's English, plus those of
 *    its `distinct` neighbors whose words appear in the batch; sense once, then one
 *    ruling line per language
 *  - memory: per key, the nearest SHIPPED keys (`i18n-brief-memory.ts`); batch keys
 *    never appear here
 *  - decisions: `decisions.md` sections whose heading cites a batch key, most
 *    specific first, each capped with a pointer to the full section
 *
 * A blind run (`excludeTargetValues`) withholds the batch's current values and the
 * decisions, drops a ruling's batch-key `exceptions` and `decision` pointer, and
 * redacts the batch's shipped values wherever the digest or a ruling quotes them.
 *
 * Schemas: `docs/i18n/termbase.md`. Deterministic: no time, RNG, or git here.
 */

import { relative, isAbsolute, join } from 'node:path'
import {
  BASE_LOCALE,
  BRAND_WORDS,
  loadCatalog,
  parseMessage,
  isRawKey,
  rawTokens,
  visibleLiterals,
} from './i18n-catalog-lib.ts'
import type { Catalog } from './i18n-catalog-lib.ts'
import { buildMemoryIndex, contentWords, isGenericWord, nearestKeys } from './i18n-brief-memory.ts'
import type { MemoryIndex } from './i18n-brief-memory.ts'
import {
  compileConcepts,
  coveredSpans,
  decisionsPath,
  englishMatchText,
  extractDigest,
  loadConceptsFor,
  loadTerms,
  parseDecisions,
  readTextIfPresent,
  resolveDocsRoot,
  sectionCitesKey,
  stylePath,
} from './i18n-termbase-lib.ts'
import type { Concept, DecisionSection, Term, Termbase } from './i18n-termbase-lib.ts'

/** What to build a brief for. */
export interface BriefOptions {
  /** target locales, in output order */
  langs: string[]
  /** the batch: English keys, in catalog order */
  keys: string[]
  /** how the keys were chosen, for the header (`--missing`, `--keys a.*`) */
  selection: string
  messagesRoot?: string
  docsRoot?: string
  /** the reference pile's root; each language's pile is `<pileRoot>/<tag>/` */
  pileRoot: string
  /** paths in the brief are printed relative to this */
  repoRoot: string
  /** blind run: withhold the batch's current translations and the decision excerpts */
  excludeTargetValues?: boolean
}

/** One named section of the brief (`--stats` reports each). */
export interface BriefSection {
  name: string
  text: string
}

/** The assembled brief. */
export interface Brief {
  sections: BriefSection[]
}

/** Glob (`*` only) to an anchored regex over keys. */
function keyGlob(pattern: string): RegExp {
  const body = pattern
    .split('*')
    .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('.*')
  return new RegExp(`^${body}$`)
}

/**
 * Picks the batch. Every given selector narrows it: `patterns` (exact keys or `*`
 * globs), `missingIn` (keys absent from ANY of these locale catalogs), `changed`
 * (keys added or edited since a ref). Catalog order is kept.
 *
 * @throws when a pattern matches no English key, since a typo would otherwise
 *   produce a silently smaller batch
 */
export function selectKeys({
  en,
  patterns,
  missingIn,
  changed,
}: {
  en: Record<string, string>
  patterns?: readonly string[]
  missingIn?: readonly Record<string, string>[]
  changed?: ReadonlySet<string>
}): string[] {
  let keys = Object.keys(en)
  if (patterns && patterns.length > 0) {
    const globs = patterns.map((pattern) => ({ pattern, re: keyGlob(pattern) }))
    const dead = globs.filter(({ re }) => !keys.some((key) => re.test(key))).map(({ pattern }) => pattern)
    if (dead.length > 0) throw new Error(`No English key matches: ${dead.join(', ')}`)
    keys = keys.filter((key) => globs.some(({ re }) => re.test(key)))
  }
  if (missingIn) keys = keys.filter((key) => missingIn.some((locale) => !(key in locale)))
  if (changed) keys = keys.filter((key) => changed.has(key))
  return keys
}

/** Keys added, or whose value changed, between `previous` and `current`, in current order. */
export function changedKeys(current: Record<string, string>, previous: Record<string, string>): string[] {
  return Object.keys(current).filter((key) => previous[key] !== current[key])
}

/** Renders a path for the brief: repo-relative when it's inside the repo. */
function shown(path: string, repoRoot: string): string {
  const rel = relative(repoRoot, path)
  return rel.startsWith('..') || isAbsolute(rel) ? path : rel
}

/** Everything the section builders share, loaded once. */
interface BriefContext {
  opts: BriefOptions
  en: Catalog
  targets: Map<string, Record<string, string>>
  termbases: Map<string, Termbase | undefined>
  concepts: Record<string, Concept>
  batch: Set<string>
  multi: boolean
  memory: MemoryIndex
}

/** Placeholder and tag notes for a key: described ones from `@key.placeholders`, the rest bare. */
function placeholderNotes(key: string, value: string, metadata: Record<string, unknown> | undefined): string[] {
  const described = (metadata?.placeholders ?? {}) as Record<string, unknown>
  const names = isRawKey(key) ? rawTokens(value) : parseMessage(value).placeholders
  const tags = isRawKey(key) ? new Set<string>() : parseMessage(value).tags
  const notes: string[] = []
  for (const name of new Set([...Object.keys(described), ...names])) {
    const info = described[name]
    if (typeof info === 'string') notes.push(`{${name}}: ${info}`)
    else if (info && typeof info === 'object') {
      const { type, example, description } = info as Record<string, unknown>
      const parts = [description, type].filter((part) => typeof part === 'string')
      const ex = typeof example === 'string' || typeof example === 'number' ? `, e.g. ${JSON.stringify(example)}` : ''
      notes.push(`{${name}}: ${parts.join(', ')}${ex}`)
    } else notes.push(`{${name}}`)
  }
  for (const tag of tags) notes.push(`<${tag}>…</${tag}>`)
  return notes
}

function headerSection(ctx: BriefContext): BriefSection {
  const { opts } = ctx
  const docsRoot = resolveDocsRoot(opts.docsRoot)
  const langs = opts.langs.join(', ')
  const lines = [
    `# Translation brief: ${langs}, ${String(opts.keys.length)} ${opts.keys.length === 1 ? 'key' : 'keys'}`,
    '',
    `- Keys: selected by \`${opts.selection}\`.`,
  ]
  if (ctx.multi) {
    lines.push(`- Style: each language's digest is below; its full \`<tag>/style.md\` is the elaboration.`)
    lines.push(`- Reference pile: \`${join(opts.pileRoot, '<tag>')}/\`, mined only for terms marked "no ruling".`)
  } else {
    const tag = opts.langs[0]
    lines.push(
      `- Style: the digest below summarizes \`${shown(stylePath(tag, docsRoot), opts.repoRoot)}\`; read it in full too.`,
    )
    lines.push(`- Reference pile: \`${join(opts.pileRoot, tag)}/\`, mined only for terms marked "no ruling".`)
  }
  lines.push(`- Mining recipes: \`${shown(join(docsRoot, 'reference-pile', 'how-to-mine.md'), opts.repoRoot)}\`.`)
  if (opts.excludeTargetValues)
    lines.push("- Blind run: the batch keys' current translations and the decision excerpts are withheld.")
  return { name: 'header', text: lines.join('\n') }
}

/** "Dutch (nl)", or the bare tag when `Intl` doesn't know it. */
function languageName(tag: string): string {
  const name = new Intl.DisplayNames(['en'], { type: 'language', fallback: 'none' }).of(tag)
  return name ? `${name} (${tag})` : tag
}

/**
 * The translator's standing instructions: everything under `## Instructions` in
 * `translator-instructions.md`, with `{{LANGUAGE}}` and `{{TAG}}` filled in (the
 * tag reads `<tag>` for several languages). Single-sourced there, so the guide and
 * every brief can't drift apart.
 */
function instructionsSection(ctx: BriefContext): BriefSection {
  const markdown = readTextIfPresent(join(resolveDocsRoot(ctx.opts.docsRoot), 'translator-instructions.md'))
  const start = markdown?.search(/^## Instructions\s*$/m) ?? -1
  if (markdown === undefined || start === -1) return { name: 'instructions', text: '' }
  const body = markdown
    .slice(start)
    .replace(/^## Instructions\s*\n/, '')
    .trim()
    .replaceAll('{{LANGUAGE}}', ctx.opts.langs.map(languageName).join(', '))
    .replaceAll('{{TAG}}', ctx.multi ? '<tag>' : ctx.opts.langs[0])
  return { name: 'instructions', text: `## Instructions\n\n${body}` }
}

/** A word must appear in at least this many English keys to be a candidate term: concepts recur. */
const MIN_TERM_FREQUENCY = 3

/** Brand words, lowercased: kept verbatim, never a term to rule on. */
const BRANDS = new Set(BRAND_WORDS.map((word) => word.toLowerCase()))

/**
 * A key's content words no registered concept's `match` covers, in order: the
 * terms a translator has to mine the pile for, named so a missing concept
 * ("offline") can't slip by as if it were settled. Generic English, brands, and
 * words fewer than `MIN_TERM_FREQUENCY` keys use are left out.
 */
function uncoveredWords(ctx: BriefContext, key: string): string[] {
  const text = englishMatchText(key, ctx.en.messages[key])
  const content = new Set(contentWords(key, ctx.en.messages[key]))
  const spans = Object.values(ctx.concepts).flatMap((concept) =>
    Array.isArray(concept.match) ? coveredSpans(concept.match, text) : [],
  )
  const out: string[] = []
  for (const hit of text.matchAll(/\p{L}+/gu)) {
    const word = hit[0].toLowerCase()
    const end = hit.index + hit[0].length
    if (!content.has(word) || out.includes(word) || isGenericWord(word) || BRANDS.has(word)) continue
    if ((ctx.memory.df.get(word) ?? 0) < MIN_TERM_FREQUENCY) continue
    if (spans.some(([from, to]) => from < end && hit.index < to)) continue
    out.push(word)
  }
  return out
}

function digestSection(ctx: BriefContext): BriefSection {
  const blocks = ctx.opts.langs.map((tag) => {
    const path = stylePath(tag, ctx.opts.docsRoot)
    const markdown = readTextIfPresent(path)
    const digest = markdown === undefined ? undefined : extractDigest(markdown)
    const body = digest ?? `(no digest yet: read \`${shown(path, ctx.opts.repoRoot)}\` in full)`
    return `## Style digest: ${tag}\n\n${body}`
  })
  return { name: 'digest', text: blocks.join('\n\n') }
}

function keysSection(ctx: BriefContext): BriefSection {
  const lines = [`## Keys (${String(ctx.opts.keys.length)})`, '']
  for (const key of ctx.opts.keys) {
    const value = ctx.en.messages[key]
    const metadata = key in ctx.en.metadata ? ctx.en.metadata[key] : undefined
    lines.push(`- \`${key}\`: ${JSON.stringify(value)}`)
    if (typeof metadata?.description === 'string') lines.push(`  - Note: ${metadata.description}`)
    const notes = placeholderNotes(key, value, metadata)
    if (notes.length > 0) lines.push(`  - ${notes.join(' · ')}`)
    const uncovered = uncoveredWords(ctx, key)
    if (uncovered.length > 0) lines.push(`  - No concept yet: ${uncovered.join(', ')}`)
    if (ctx.opts.excludeTargetValues) continue
    for (const tag of ctx.opts.langs) {
      const current = ctx.targets.get(tag)?.[key]
      if (current !== undefined) lines.push(`  - ${tag} now: ${JSON.stringify(current)}`)
    }
  }
  return { name: 'keys', text: lines.join('\n') }
}

/** One language's ruling line for a concept. */
function rulingLine(ctx: BriefContext, tag: string, id: string): string {
  const term: Term | undefined = ctx.termbases.get(tag)?.[id]
  if (!term) return `- ${tag}: no ruling (mine the pile, then add one)`
  const parts = [`**${term.chosen}** (${term.confidence})`]
  if (term.accept?.length) parts.push(`accept: ${term.accept.join(', ')}`)
  if (term.forms) parts.push(`forms: ${term.forms}`)
  if (term.avoid?.length) parts.push(`avoid: ${term.avoid.map((a) => `${a.form} (${a.why})`).join('; ')}`)
  if (term.note) parts.push(`note: ${term.note}`)
  // A blind run drops both: a batch key's exception names how it deviates, and
  // the decision it points at is withheld anyway.
  if (!ctx.opts.excludeTargetValues) {
    const exceptions = Object.entries(term.exceptions ?? {}).filter(([key]) => ctx.batch.has(key))
    if (exceptions.length > 0) {
      parts.push(`exceptions here: ${exceptions.map(([key, why]) => `\`${key}\` (${why})`).join('; ')}`)
    }
    if (term.decision) parts.push(`decision: "${term.decision}"`)
  }
  return `- ${tag}: ${parts.join(' · ')}`
}

/** The concepts a batch puts in play: which keys hit each, and which neighbors ride along via `distinct`. */
interface ConceptsInPlay {
  /** concept ID → the batch keys whose English hits it */
  hitsBy: Map<string, string[]>
  /** a `distinct` neighbor no key hits → the hit concept that named it */
  neighbors: Map<string, string>
}

function conceptsInPlay(ctx: BriefContext): ConceptsInPlay {
  const matchers = compileConcepts(ctx.concepts)
  const hitsBy = new Map<string, string[]>()
  for (const key of ctx.opts.keys) {
    const text = englishMatchText(key, ctx.en.messages[key])
    for (const [id, hit] of matchers) {
      if (hit(text)) hitsBy.set(id, [...(hitsBy.get(id) ?? []), key])
    }
  }
  const batchText = ctx.opts.keys
    .map((key) => englishMatchText(key, ctx.en.messages[key]))
    .join('\n')
    .toLowerCase()
  const neighbors = new Map<string, string>()
  for (const id of [...hitsBy.keys()].sort()) {
    for (const other of ctx.concepts[id].distinct ?? []) {
      if (hitsBy.has(other) || neighbors.has(other) || !(other in ctx.concepts)) continue
      if (neighbors.size < MAX_NEIGHBORS && mentionedIn(ctx.concepts[other], batchText)) neighbors.set(other, id)
    }
  }
  return { hitsBy, neighbors }
}

/** How many easy-to-confuse neighbors a brief lists at most. */
const MAX_NEIGHBORS = 8

/**
 * Whether a neighbor is plausibly in play: the head word of one of its `match`
 * forms appears anywhere in the batch's English, loosely (a substring, so
 * `queue` counts for "Operation queue" even when the form is `=queue`). A neighbor
 * whose words the batch never uses can't be confused with anything in it.
 */
function mentionedIn(concept: Concept, batchText: string): boolean {
  const forms = Array.isArray(concept.match) ? concept.match : []
  return forms.some((form) => {
    const head = form.replace(/^=/, '').replace(/\*$/, '').trim().split(/\s+/)[0]
    return head.length > 0 && batchText.includes(head)
  })
}

/** A hit concept's block: sense and boundaries once, then a full ruling line per language. */
function conceptBlock(ctx: BriefContext, id: string, keys: readonly string[]): string[] {
  const concept = ctx.concepts[id]
  const about = [concept.sense]
  if (concept.note) about.push(concept.note)
  if (concept.distinct?.length) about.push(`Distinct from: ${concept.distinct.join(', ')}.`)
  about.push(`In: ${keys.map((key) => `\`${key}\``).join(', ')}.`)
  return ['', `### ${id}: "${concept.en}"`, about.join(' '), ...ctx.opts.langs.map((tag) => rulingLine(ctx, tag, id))]
}

/**
 * A `distinct` neighbor no batch key hits, in one line: its sense and each
 * language's chosen form. Enough to keep it apart from the concept that named it;
 * the full ruling is a `--keys` run away if a translator needs it.
 */
function neighborLine(ctx: BriefContext, id: string, namedBy: string): string {
  const concept = ctx.concepts[id]
  const rulings = ctx.opts.langs.map((tag) => {
    const term = ctx.termbases.get(tag)?.[id]
    return term ? `${tag}: **${term.chosen}**` : `${tag}: no ruling`
  })
  return `- \`${id}\` ("${concept.en}", vs ${namedBy}): ${concept.sense} ${rulings.join(' · ')}`
}

function termsSection(ctx: BriefContext): BriefSection {
  const { hitsBy, neighbors } = conceptsInPlay(ctx)
  const hit = [...hitsBy.keys()].sort()
  const lines = [`## Terms in play (${String(hit.length)})`]
  for (const id of hit) lines.push(...conceptBlock(ctx, id, hitsBy.get(id) ?? []))
  if (hit.length === 0) lines.push('', 'No registered concept appears in this batch.')
  if (neighbors.size > 0) {
    lines.push('', '### Easy to confuse with the above (not in this batch)', '')
    for (const id of [...neighbors.keys()].sort()) lines.push(neighborLine(ctx, id, neighbors.get(id) ?? ''))
  }
  return { name: 'terms', text: lines.join('\n') }
}

function memorySection(ctx: BriefContext): BriefSection {
  const count = ctx.multi ? 3 : 4
  const targets = ctx.opts.langs.map((tag) => ctx.targets.get(tag) ?? {})
  const index = ctx.memory
  const shownKeys = new Set<string>()
  const lines = [
    '## Translation memory',
    '',
    'Shipped neighbors of each key: siblings first, then the closest English elsewhere.',
  ]
  for (const key of ctx.opts.keys) {
    const near = nearestKeys({ key, en: ctx.en.messages, targets, batch: ctx.batch, count, index })
    if (near.length === 0) continue
    lines.push('', `Near \`${key}\`:`)
    for (const other of near) {
      if (shownKeys.has(other)) {
        lines.push(`- \`${other}\` (above)`)
        continue
      }
      shownKeys.add(other)
      const en = JSON.stringify(ctx.en.messages[other])
      if (!ctx.multi) {
        lines.push(`- \`${other}\`: ${en} → ${JSON.stringify(targets[0][other])}`)
        continue
      }
      lines.push(`- \`${other}\`: ${en}`)
      for (const tag of ctx.opts.langs) {
        const value = ctx.targets.get(tag)?.[other]
        if (value !== undefined) lines.push(`  - ${tag}: ${JSON.stringify(value)}`)
      }
    }
  }
  return { name: 'memory', text: lines.join('\n') }
}

/** Cuts an excerpt at a line break near `cap`, pointing at the rest. */
function capped(body: string, cap: number, pointer: string): string {
  if (body.length <= cap) return body
  const cut = body.lastIndexOf('\n', cap)
  const end = cut > cap / 2 ? cut : cap
  return `${body.slice(0, end).trimEnd()}\n… (cut; full section: ${pointer})`
}

/** The decision sections a language's batch keys pull in, most specific first. */
function rankedDecisions(ctx: BriefContext, sections: DecisionSection[]): DecisionSection[] {
  const namespaces = new Set(Object.keys(ctx.en.messages).map((key) => key.split('.')[0]))
  const allKeys = Object.keys(ctx.en.messages)
  const breadth = new Map<string, number>()
  const width = (citation: string) => {
    let n = breadth.get(citation)
    if (n === undefined) {
      n = allKeys.filter((key) => sectionCitesKey([citation], key, namespaces)).length
      breadth.set(citation, n)
    }
    return n
  }
  const scored = sections.flatMap((section) => {
    const relevant = section.citations.filter((citation) =>
      ctx.opts.keys.some((key) => sectionCitesKey([citation], key, namespaces)),
    )
    return relevant.length === 0 ? [] : [{ section, narrowest: Math.min(...relevant.map(width)) }]
  })
  // A `###` whose `##` also matches is already inside the parent's body.
  const matched = new Set(scored.map(({ section }) => section))
  const parentOfSection = (section: DecisionSection) =>
    section.level === 3 ? sections.slice(0, sections.indexOf(section)).findLast((s) => s.level === 2) : undefined
  return scored
    .filter(({ section }) => {
      const parent = parentOfSection(section)
      return parent === undefined || !matched.has(parent)
    })
    .sort((a, b) => a.narrowest - b.narrowest || a.section.line - b.section.line)
    .map(({ section }) => section)
}

function decisionsSection(ctx: BriefContext): BriefSection {
  const maxSections = ctx.multi ? 3 : 6
  const cap = ctx.multi ? 700 : 1500
  const blocks: string[] = []
  for (const tag of ctx.opts.langs) {
    const path = decisionsPath(tag, ctx.opts.docsRoot)
    const markdown = readTextIfPresent(path)
    if (markdown === undefined) continue
    const ranked = rankedDecisions(ctx, parseDecisions(markdown))
    if (ranked.length === 0) continue
    const file = shown(path, ctx.opts.repoRoot)
    const lines = [`## Decisions: ${tag}`]
    for (const section of ranked.slice(0, maxSections)) {
      const pointer = `${file}:${String(section.line)}`
      lines.push('', `### ${section.heading} (${pointer})`, '', capped(section.body, cap, pointer))
    }
    const rest = ranked.slice(maxSections)
    if (rest.length > 0) {
      lines.push('', `Also citing this batch: ${rest.map((s) => `"${s.heading}" (:${String(s.line)})`).join('; ')}.`)
    }
    blocks.push(lines.join('\n'))
  }
  return { name: 'decisions', text: blocks.join('\n\n') }
}

/** A shipped value shorter than this, with no space, is too generic to redact (`Sluit` is everywhere). */
const MIN_REDACTED_LENGTH = 10

/**
 * The batch's shipped values in the forms a doc might quote them: raw, with ICU's
 * doubled apostrophe read as one, and as its visible text. Longest first, so a
 * value is withheld before any shorter one inside it.
 */
function shippedValues(ctx: BriefContext): string[] {
  const values = new Set<string>()
  for (const tag of ctx.opts.langs) {
    for (const key of ctx.opts.keys) {
      const value = ctx.targets.get(tag)?.[key]
      if (value === undefined) continue
      for (const form of [value, value.replace(/''/g, "'"), visibleLiterals(value, tag)?.trim()]) {
        if (!form) continue
        // A rule often quotes one clause of a value rather than all of it.
        for (const part of [form, ...form.split(/\s*[.:;!?…]+(?:\s+|$)/)]) {
          const clause = part.trim()
          if (clause.length >= MIN_REDACTED_LENGTH || (clause.includes(' ') && clause === form)) values.add(clause)
        }
      }
    }
  }
  return [...values].sort((a, b) => b.length - a.length)
}

/**
 * Best-effort blind-run redaction: a ruling's `forms` or the style digest can quote
 * a batch key's exact shipped value, which would hand the translator the answer.
 */
function redactShipped(section: BriefSection, values: readonly string[]): BriefSection {
  let text = section.text
  for (const value of values) text = text.replaceAll(value, '[withheld]')
  return { ...section, text }
}

/** Loads everything once and builds every section. */
export function buildBrief(opts: BriefOptions): Brief {
  const en = loadCatalog(BASE_LOCALE, opts.messagesRoot)
  const targets = new Map<string, Record<string, string>>()
  const termbases = new Map<string, Termbase | undefined>()
  let concepts: Record<string, Concept> = loadConceptsFor(undefined, opts.docsRoot)
  for (const tag of opts.langs) {
    try {
      targets.set(tag, loadCatalog(tag, opts.messagesRoot).messages)
    } catch {
      targets.set(tag, {})
    }
    termbases.set(tag, loadTerms(tag, opts.docsRoot))
    concepts = { ...loadConceptsFor(tag, opts.docsRoot), ...concepts }
  }
  const ctx: BriefContext = {
    opts,
    en,
    targets,
    termbases,
    concepts,
    batch: new Set(opts.keys),
    multi: opts.langs.length > 1,
    memory: buildMemoryIndex(en.messages),
  }
  let sections = [
    headerSection(ctx),
    instructionsSection(ctx),
    digestSection(ctx),
    keysSection(ctx),
    termsSection(ctx),
    memorySection(ctx),
  ]
  if (opts.excludeTargetValues) {
    const values = shippedValues(ctx)
    sections = sections.map((section) =>
      section.name === 'digest' || section.name === 'terms' ? redactShipped(section, values) : section,
    )
  } else sections.push(decisionsSection(ctx))
  return { sections: sections.filter((section) => section.text.trim().length > 0) }
}

/** Joins the sections into the brief's markdown. */
export function renderBrief(brief: Brief): string {
  return `${brief.sections.map((section) => section.text).join('\n\n')}\n`
}

/**
 * A rough token count: about four ASCII characters per token, and a bit under one
 * token per non-ASCII character (accents, CJK). Good enough to compare runs.
 */
export function roughTokens(text: string): number {
  let ascii = 0
  let other = 0
  for (const char of text) {
    if (char.charCodeAt(0) < 128) ascii++
    else other++
  }
  return Math.round(ascii / 4 + other * 0.75)
}
