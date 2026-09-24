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
 *  - digest: the `## Digest` section of each language's style guide
 *  - keys: key, English, `@key` description, placeholders/tags, current value(s)
 *  - terms: every concept whose `match` hits a batch key's English, plus its
 *    `distinct` neighbors; sense once, then one ruling line per language
 *  - memory: per key, the nearest SHIPPED keys (siblings first, then IDF-weighted
 *    English word overlap), with their translations; batch keys never appear here
 *  - decisions: `decisions.md` sections whose heading cites a batch key, most
 *    specific first, each capped with a pointer to the full section
 *  - footer: where to write back
 *
 * Schemas: `docs/i18n/termbase.md`. Deterministic: no time, RNG, or git here.
 */

import { relative, isAbsolute, join } from 'node:path'
import { BASE_LOCALE, loadCatalog, parseMessage, isRawKey, rawTokens } from './i18n-catalog-lib.ts'
import type { Catalog } from './i18n-catalog-lib.ts'
import {
  compileConcepts,
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

/** Words too common to say two strings are related. */
const STOPWORDS = new Set(
  'the and for you your this that with from are was not but can will its has have into our out all any'.split(' '),
)

/** The content words of a key's English: lowercase, three letters or more, no stopwords. */
function contentWords(key: string, value: string): Set<string> {
  const words =
    englishMatchText(key, value)
      .toLowerCase()
      .match(/\p{L}+/gu) ?? []
  return new Set(words.filter((word) => word.length >= 3 && !STOPWORDS.has(word)))
}

/** Per-key content words plus their IDF, built once per run. */
export interface MemoryIndex {
  order: Map<string, number>
  words: Map<string, Set<string>>
  idf: Map<string, number>
}

/** Indexes the English catalog for `nearestKeys`. */
export function buildMemoryIndex(en: Record<string, string>): MemoryIndex {
  const order = new Map<string, number>()
  const words = new Map<string, Set<string>>()
  const df = new Map<string, number>()
  for (const [position, key] of Object.keys(en).entries()) {
    order.set(key, position)
    const set = contentWords(key, en[key])
    words.set(key, set)
    for (const word of set) df.set(word, (df.get(word) ?? 0) + 1)
  }
  const total = Math.max(order.size, 1)
  const idf = new Map([...df].map(([word, count]) => [word, Math.log(1 + total / count)]))
  return { order, words, idf }
}

/** Cosine-style similarity of two keys' content words, each word weighted by its IDF. */
function similarity(index: MemoryIndex, a: string, b: string): number {
  const wordsA = index.words.get(a) ?? new Set<string>()
  const wordsB = index.words.get(b) ?? new Set<string>()
  const weight = (word: string) => index.idf.get(word) ?? 0
  let shared = 0
  for (const word of wordsA) if (wordsB.has(word)) shared += weight(word)
  if (shared === 0) return 0
  const norm = (set: Set<string>) => [...set].reduce((sum, word) => sum + weight(word), 0)
  return shared / Math.sqrt(norm(wordsA) * norm(wordsB))
}

/** The parent path of a key (`a.b.c` → `a.b`). */
const parentOf = (key: string) => key.slice(0, Math.max(key.lastIndexOf('.'), 0))

/**
 * A key's translation memory: the nearest keys that at least one target locale
 * has shipped, excluding the batch. Same-parent siblings come first (they share a
 * dialog, so they share its voice), capped at half the slots so a strong match
 * elsewhere in the catalog, often the same phrase on another surface, still gets
 * in. Siblings rank by word overlap, then by distance in the catalog; the rest need
 * at least one shared content word.
 */
export function nearestKeys({
  key,
  en,
  targets,
  batch,
  count,
  index,
}: {
  key: string
  en: Record<string, string>
  targets: readonly Record<string, string>[]
  batch: ReadonlySet<string>
  count: number
  index?: MemoryIndex
}): string[] {
  const ix = index ?? buildMemoryIndex(en)
  const parent = parentOf(key)
  const position = ix.order.get(key) ?? 0
  const candidates = Object.keys(en).filter(
    (other) => other !== key && !batch.has(other) && targets.some((target) => other in target),
  )
  const scored = candidates.map((other) => ({
    key: other,
    score: similarity(ix, key, other),
    distance: Math.abs((ix.order.get(other) ?? 0) - position),
    sibling: parentOf(other) === parent,
  }))
  const byScore = (a: (typeof scored)[number], b: (typeof scored)[number]) =>
    b.score - a.score || a.distance - b.distance || a.key.localeCompare(b.key)
  const siblings = scored.filter((entry) => entry.sibling).sort(byScore)
  const picked = siblings.slice(0, Math.ceil(count / 2)).map((entry) => entry.key)
  const rest = scored
    .filter((entry) => entry.score > 0 && !picked.includes(entry.key))
    .sort(byScore)
    .map((entry) => entry.key)
  for (const other of [...rest, ...siblings.map((entry) => entry.key)]) {
    if (picked.length >= count) break
    if (!picked.includes(other)) picked.push(other)
  }
  return picked
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
  const exceptions = Object.entries(term.exceptions ?? {}).filter(([key]) => ctx.batch.has(key))
  if (exceptions.length > 0) {
    parts.push(`exceptions here: ${exceptions.map(([key, why]) => `\`${key}\` (${why})`).join('; ')}`)
  }
  if (term.decision) parts.push(`decision: "${term.decision}"`)
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
  const neighbors = new Map<string, string>()
  for (const id of [...hitsBy.keys()].sort()) {
    for (const other of ctx.concepts[id].distinct ?? []) {
      if (!hitsBy.has(other) && !neighbors.has(other) && other in ctx.concepts) neighbors.set(other, id)
    }
  }
  return { hitsBy, neighbors }
}

/** One concept's block: sense and boundaries once, then a ruling line per language. */
function conceptBlock(ctx: BriefContext, id: string, { hitsBy, neighbors }: ConceptsInPlay): string[] {
  const concept = ctx.concepts[id]
  const about = [concept.sense]
  if (concept.note) about.push(concept.note)
  const distinct = (concept.distinct ?? []).filter((other) => other !== neighbors.get(id))
  if (distinct.length > 0) about.push(`Distinct from: ${distinct.join(', ')}.`)
  const keys = hitsBy.get(id)
  about.push(
    keys
      ? `In: ${keys.map((key) => `\`${key}\``).join(', ')}.`
      : `Not in this batch; listed so it isn't confused with ${neighbors.get(id) ?? ''}.`,
  )
  return ['', `### ${id}: "${concept.en}"`, about.join(' '), ...ctx.opts.langs.map((tag) => rulingLine(ctx, tag, id))]
}

function termsSection(ctx: BriefContext): BriefSection {
  const inPlay = conceptsInPlay(ctx)
  const ids = [...[...inPlay.hitsBy.keys()].sort(), ...[...inPlay.neighbors.keys()].sort()]
  const lines = [`## Terms in play (${String(ids.length)})`, ...ids.flatMap((id) => conceptBlock(ctx, id, inPlay))]
  if (ids.length === 0) lines.push('', 'No registered concept appears in this batch.')
  return { name: 'terms', text: lines.join('\n') }
}

function memorySection(ctx: BriefContext): BriefSection {
  const count = ctx.multi ? 3 : 4
  const targets = ctx.opts.langs.map((tag) => ctx.targets.get(tag) ?? {})
  const index = buildMemoryIndex(ctx.en.messages)
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

function footerSection(ctx: BriefContext): BriefSection {
  const docs = shown(resolveDocsRoot(ctx.opts.docsRoot), ctx.opts.repoRoot)
  const tag = ctx.multi ? '<tag>' : ctx.opts.langs[0]
  const lines = [
    '## Writing back',
    '',
    `- A new or changed ruling: edit its entry in \`${docs}/${tag}/terms.json\`; a replaced form moves to \`avoid\` with its reason.`,
    `- A recurring concept with no entry: add it to \`${docs}/concepts.json\` (during the parallel migration, \`${docs}/${tag}/concepts-proposed.json\`).`,
    `- A key whose English uses a concept but whose translation rightly doesn't use the ruling: record it in that term's \`exceptions\` with the reason.`,
    `- Rationale worth more than a line: a section in \`${docs}/${tag}/decisions.md\` whose heading cites the keys in backticks; point the term's \`decision\` at it.`,
    `- Anything only a native reviewer can settle: \`${docs}/${tag}/review-queue.md\`.`,
    '- Then run `pnpm check i18n` from the repo root.',
  ]
  return { name: 'footer', text: lines.join('\n') }
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
  }
  const sections = [headerSection(ctx), digestSection(ctx), keysSection(ctx), termsSection(ctx), memorySection(ctx)]
  if (!opts.excludeTargetValues) sections.push(decisionsSection(ctx))
  sections.push(footerSection(ctx))
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
