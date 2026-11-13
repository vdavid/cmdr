#!/usr/bin/env node
/**
 * `pnpm i18n:pile`: what the reference pile says about a term, from any worktree.
 * The pile lives only in the MAIN clone (`_ignored/i18n/`); this finds it through
 * git's common dir, so no caller names that path. Search logic: `i18n-pile-lib.ts`.
 *
 *   --lang <tag>              the catalog tag (`pt` mines `pt-BR/`, `zh-Hant` mines `zh-Hant/` + `zh-TW/`)
 *   --term "<English>"        forward: how each source translates it (repeatable)
 *   --target "<word>"         reverse: which English each source translates this way (repeatable)
 *   (Pass every lookup in one call: a worktree guard refuses a shell loop over this command.)
 *   --key <message key>       (repeatable) forward, for each short branch of the key's English and each concept it hits
 *   --source macos,microsoft,linux,orthodox,git   only these sources (default: all)
 *   --limit <n>               hits per source and query (default 6; exact matches come first)
 *   --pile-root <dir>, --messages-root <dir>, --docs-root <dir>   fixtures
 *
 * Exit 0 with hits or without (an empty answer is an answer), 2 on a usage
 * error or a missing pile.
 */

import { parseArgs } from 'node:util'
import { BASE_LOCALE, loadCatalog } from './i18n-catalog-lib.ts'
import { EXIT_CLEAN, EXIT_ERROR } from './i18n-locale-check-lib.ts'
import { PILE_SOURCES, lookUp, normalizeLabel, presentFolders } from './i18n-pile-lib.ts'
import type { PileHit, PileQuery, PileSource } from './i18n-pile-lib.ts'
import { pileFolders, pileRoot } from './i18n-repo-lib.ts'
import { compileConcepts, englishMatchTexts, loadConceptsFor } from './i18n-termbase-lib.ts'

/** The parsed flags. */
export interface PileArgs {
  lang: string
  queries: PileQuery[]
  keys: string[]
  sources: PileSource[]
  limit: number
  pileRoot?: string
  messagesRoot?: string
  docsRoot?: string
}

/** `--source a,b`, checked against the known sources; every source when absent. */
function parseSources(flag: string | undefined): PileSource[] {
  const sources = flag ? flag.split(',').map((source) => source.trim()) : [...PILE_SOURCES]
  const unknown = sources.filter((source) => !PILE_SOURCES.includes(source as PileSource))
  if (unknown.length > 0)
    throw new Error(`Unknown source(s): ${unknown.join(', ')} (have: ${PILE_SOURCES.join(', ')}).`)
  return sources as PileSource[]
}

/** `--limit <n>`, a positive whole number; 6 when absent. */
function parseLimit(flag: string | undefined): number {
  const limit = flag === undefined ? 6 : Number(flag)
  if (!Number.isInteger(limit) || limit < 1) throw new Error('--limit takes a positive whole number.')
  return limit
}

/**
 * Parses the flags: one `--lang` and at least one of `--term`, `--target`, `--key`.
 * @param argv `process.argv.slice(2)`
 */
export function parsePileArgs(argv: string[]): PileArgs {
  const { values, positionals } = parseArgs({
    args: argv,
    options: {
      lang: { type: 'string' },
      term: { type: 'string', multiple: true },
      target: { type: 'string', multiple: true },
      key: { type: 'string', multiple: true },
      source: { type: 'string' },
      limit: { type: 'string' },
      'pile-root': { type: 'string' },
      'messages-root': { type: 'string' },
      'docs-root': { type: 'string' },
    },
    allowPositionals: true,
  })
  if (positionals.length > 0) {
    throw new Error(`Unexpected argument(s): ${positionals.join(' ')}. Quote a phrase: --term "Move to Trash".`)
  }
  const lang = values.lang?.trim()
  if (!lang || lang.includes(',')) throw new Error('Pass one --lang <tag>.')
  const queries: PileQuery[] = [
    ...(values.term ?? []).map((text) => ({ side: 'english' as const, text })),
    ...(values.target ?? []).map((text) => ({ side: 'target' as const, text })),
  ]
  if (queries.length === 0 && !values.key?.length)
    throw new Error('Pass --term "<English>", --target "<word>", or --key <key>.')
  const sources = parseSources(values.source)
  const limit = parseLimit(values.limit)
  return {
    lang,
    queries,
    keys: values.key ?? [],
    sources,
    limit,
    pileRoot: values['pile-root'],
    messagesRoot: values['messages-root'],
    docsRoot: values['docs-root'],
  }
}

/** How many words a branch may have and still be looked up whole: a label, not a sentence. */
const MAX_LABEL_WORDS = 6

/**
 * The forward queries a key stands for: each branch of its English short enough
 * to be a label, then the headword of every concept the key's English hits.
 */
export function keyQueries(key: string, opts: { messagesRoot?: string; docsRoot?: string } = {}): PileQuery[] {
  const en = loadCatalog(BASE_LOCALE, opts.messagesRoot).messages
  const value = en[key] as string | undefined
  if (value === undefined) throw new Error(`Not an English key: ${key}`)
  const texts = englishMatchTexts(key, value)
  const labels = texts.map((text) => text.trim()).filter((text) => text.split(/\s+/).length <= MAX_LABEL_WORDS)
  const concepts = loadConceptsFor(undefined, opts.docsRoot)
  const matchers = compileConcepts(concepts)
  const heads = [...matchers].filter(([, hit]) => texts.some((text) => hit(text, key))).map(([id]) => concepts[id].en)
  const unique = new Map([...labels, ...heads].map((text) => [normalizeLabel(text), text]))
  return [...unique.values()].map((text) => ({ side: 'english', text }))
}

function hitLine(hit: PileHit): string {
  const detail = hit.detail ? `  [${hit.detail.length > 140 ? `${hit.detail.slice(0, 140)}…` : hit.detail}]` : ''
  const pair = hit.english ? `${JSON.stringify(hit.english)} → ${JSON.stringify(hit.target)}` : hit.target
  return `  ${hit.exact ? '=' : '~'} ${pair}${detail}  (${hit.file})`
}

/** One query's report: hits grouped by source, capped, with what was left out counted. */
export function renderQuery(query: PileQuery, hits: readonly PileHit[], limit: number): string[] {
  const arrow = query.side === 'english' ? 'English' : 'translation'
  const lines = [`## ${JSON.stringify(query.text)} (by ${arrow})`]
  if (hits.length === 0) lines.push('  no hits in any source')
  for (const source of PILE_SOURCES) {
    const ofSource = hits.filter((hit) => hit.source === source)
    if (ofSource.length === 0) continue
    lines.push(`- ${source}:`)
    lines.push(...ofSource.slice(0, limit).map(hitLine))
    if (ofSource.length > limit) lines.push(`  … ${String(ofSource.length - limit)} more (--limit)`)
  }
  return lines
}

function main(): number {
  const args = parsePileArgs(process.argv.slice(2))
  const root = args.pileRoot ?? pileRoot()
  const { present, missing } = presentFolders(root, pileFolders(args.lang))
  if (present.length === 0) {
    console.error(`No pile folder for ${args.lang} under ${root} (looked for ${missing.join(', ')}).`)
    return EXIT_ERROR
  }
  const queries = [...args.queries, ...args.keys.flatMap((key) => keyQueries(key, args))]
  console.log(`# Reference pile for ${args.lang}: ${present.map((folder) => `${root}/${folder}/`).join(' + ')}`)
  console.log('`=` an exact match, `~` one that contains the words. Weigh macOS over Microsoft over the rest.')
  for (const query of queries) {
    console.log('')
    const hits = lookUp({ pileRootDir: root, folders: present, query, sources: args.sources })
    for (const line of renderQuery(query, hits, args.limit)) console.log(line)
  }
  return EXIT_CLEAN
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  try {
    process.exit(main())
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(EXIT_ERROR)
  }
}
