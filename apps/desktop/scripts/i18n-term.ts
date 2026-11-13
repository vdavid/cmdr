#!/usr/bin/env node
/**
 * `pnpm i18n:term`: every sense of an English word in the termbase, with each
 * language's ruling. One English word often names several concepts (`edit` vs
 * `text-editor`, `archive` vs `archive-chat`), and a translator choosing between
 * them needs every sense's definition side by side, which `concepts.json` alone
 * scatters across 600 entries.
 *
 *   pnpm i18n:term <word|concept-id> [--lang <tag>[,<tag>…]|all]
 *   pnpm i18n:term --key <message key> [--lang …]
 *
 * A word finds each concept whose ID it is, whose `match` reads it, or whose
 * headword contains it; then each sense prints its definition, note, match and
 * notMatch forms, distinct neighbors, and one ruling line per language. `--key`
 * answers "why does the termbase check flag this key?": which concepts its
 * English hits, branch by branch, and which ones a `notMatch` keeps out.
 * Default languages: every full translation with a `terms.json`.
 */

import { parseArgs } from 'node:util'
import { BASE_LOCALE, listLocales, loadCatalog, resolveLocaleSource } from './i18n-catalog-lib.ts'
import { EXIT_CLEAN, EXIT_ERROR } from './i18n-locale-check-lib.ts'
import {
  compileConcepts,
  compileMatch,
  englishMatchTexts,
  loadConceptsFor,
  loadTerms,
  notMatchKey,
  rulingParts,
} from './i18n-termbase-lib.ts'
import type { Concept, Concepts, Termbase } from './i18n-termbase-lib.ts'

/** The parsed flags. */
export interface TermArgs {
  word?: string
  key?: string
  langs?: string[]
  messagesRoot?: string
  docsRoot?: string
}

/**
 * Parses the flags: a word (or concept ID) or `--key`, and optionally `--lang`.
 * @param argv `process.argv.slice(2)`
 */
export function parseTermArgs(argv: string[]): TermArgs {
  const { values, positionals } = parseArgs({
    args: argv,
    options: {
      key: { type: 'string' },
      lang: { type: 'string' },
      'messages-root': { type: 'string' },
      'docs-root': { type: 'string' },
    },
    allowPositionals: true,
  })
  const word = positionals.join(' ').trim() || undefined
  if ((word === undefined) === (values.key === undefined)) {
    throw new Error('Pass a word or concept ID (pnpm i18n:term move), or --key <message key>.')
  }
  const lang = values.lang?.trim()
  return {
    word,
    key: values.key,
    langs: lang === undefined || lang === 'all' ? undefined : lang.split(',').map((tag) => tag.trim()),
    messagesRoot: values['messages-root'],
    docsRoot: values['docs-root'],
  }
}

/** The concept IDs a word names: its ID, a concept whose `match` reads it, or one whose headword contains it. */
export function conceptsForWord(concepts: Concepts, word: string): string[] {
  const lower = word.trim().toLowerCase()
  return Object.entries(concepts)
    .filter(
      ([id, concept]) =>
        id === lower ||
        compileMatch(Array.isArray(concept.match) ? concept.match : [])(word) ||
        concept.en
          .toLowerCase()
          .split(/[\s/()]+/)
          .includes(lower),
    )
    .map(([id]) => id)
}

/** What the concept matcher makes of one key: per concept, whether its English hits it or a `notMatch` keeps it out. */
export function keyVerdicts(concepts: Concepts, key: string, english: string): { hit: string[]; excluded: string[] } {
  const texts = englishMatchTexts(key, english)
  const withExclusions = compileConcepts(concepts)
  const hit: string[] = []
  const excluded: string[] = []
  for (const [id, concept] of Object.entries(concepts)) {
    const bare = compileMatch(Array.isArray(concept.match) ? concept.match : [])
    if (!texts.some((text) => bare(text))) continue
    const matcher = withExclusions.get(id)
    if (matcher && texts.some((text) => matcher(text, key))) hit.push(id)
    else excluded.push(id)
  }
  return { hit, excluded }
}

/** A concept's notMatch, printed: forms as they are, key-scoped entries by key. */
function notMatchSummary(concept: Concept): string | undefined {
  const entries: unknown[] = Array.isArray(concept.notMatch) ? concept.notMatch : []
  if (entries.length === 0) return undefined
  return entries.map((entry) => (typeof entry === 'string' ? entry : `key ${notMatchKey(entry) ?? '?'}`)).join(' | ')
}

/** One sense, printed: definition, boundaries, then a ruling line per language. */
export function senseBlock({
  id,
  concepts,
  termbases,
  key,
}: {
  id: string
  concepts: Concepts
  termbases: ReadonlyMap<string, Termbase | undefined>
  key?: string
}): string[] {
  const concept = concepts[id]
  const lines = [`## ${id}: "${concept.en}"`, concept.sense]
  if (concept.note) lines.push(`Note: ${concept.note}`)
  lines.push(`Match: ${(Array.isArray(concept.match) ? concept.match : []).join(' | ')}`)
  const notMatch = notMatchSummary(concept)
  if (notMatch) lines.push(`Not: ${notMatch}`)
  for (const other of concept.distinct ?? []) {
    const neighbor = concepts[other] as Concept | undefined
    lines.push(`Distinct from ${other}${neighbor ? `: ${neighbor.sense}` : ''}`)
  }
  for (const [tag, terms] of termbases) {
    const term = terms?.[id]
    if (!term) {
      lines.push(`- ${tag}: no ruling`)
      continue
    }
    const exceptionCount = Object.keys(term.exceptions ?? {}).length
    const parts = rulingParts(term, {
      exceptionLabel: 'exception here',
      exceptionKeys: key === undefined ? undefined : (name) => name === key,
    })
    if (key === undefined && exceptionCount > 0) parts.push(`${String(exceptionCount)} exception(s)`)
    lines.push(`- ${tag}: ${parts.join(' · ')}`)
  }
  return lines
}

/** The languages to print rulings for: the asked ones, or every full translation with a termbase. */
function termbasesFor(args: TermArgs): Map<string, Termbase | undefined> {
  const available = listLocales(args.messagesRoot)
  const full = available.filter((tag) => tag !== BASE_LOCALE && !resolveLocaleSource(tag, available).isOverlay)
  const langs = args.langs ?? full.filter((tag) => loadTerms(tag, args.docsRoot) !== undefined)
  const unknown = langs.filter((tag) => !full.includes(tag))
  if (unknown.length > 0) throw new Error(`Not a full translation: ${unknown.join(', ')} (have: ${full.join(', ')})`)
  return new Map(langs.map((tag) => [tag, loadTerms(tag, args.docsRoot)]))
}

function main(): number {
  const args = parseTermArgs(process.argv.slice(2))
  const concepts = loadConceptsFor(undefined, args.docsRoot)
  const termbases = termbasesFor(args)
  let ids: string[]
  if (args.key !== undefined) {
    const english = loadCatalog(BASE_LOCALE, args.messagesRoot).messages[args.key] as string | undefined
    if (english === undefined) throw new Error(`Not an English key: ${args.key}`)
    const { hit, excluded } = keyVerdicts(concepts, args.key, english)
    console.log(`# ${args.key}: ${JSON.stringify(english)}`)
    console.log(`Concepts it hits: ${hit.join(', ') || 'none'}`)
    if (excluded.length > 0) console.log(`Matched, but a notMatch keeps it out: ${excluded.join(', ')}`)
    ids = hit
  } else {
    ids = conceptsForWord(concepts, args.word ?? '')
    console.log(`# "${args.word ?? ''}": ${String(ids.length)} sense(s)`)
    if (ids.length === 0)
      console.log('No concept yet: mine the pile (pnpm i18n:pile), then add one with pnpm i18n:apply-terms.')
  }
  for (const id of ids) {
    console.log('')
    for (const line of senseBlock({ id, concepts, termbases, key: args.key })) console.log(line)
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
