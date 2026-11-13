/**
 * Termbase write-back, the other half of `pnpm i18n:apply`: `pnpm i18n:apply-terms`
 * (`i18n-apply-terms.ts` is the CLI) patches one termbase file without anyone
 * hand-editing JSON or writing a script. The 2026-11-12 fan-out left a
 * `terms-<tag>.mjs` per translator, each re-sorting or re-indenting the whole file.
 *
 * Three targets, one patch shape:
 *
 *  - a locale's rulings, `docs/i18n/<tag>/terms.json`
 *  - the shared registry, `docs/i18n/concepts.json` (a `notMatch`, a new concept)
 *  - a parallel fan-out's staging file, `docs/i18n/<tag>/concepts-proposed.json`
 *
 * A patch maps a concept ID to the fields to change, or to `null` to delete the
 * entry. In an entry, a field's value replaces the stored one and `null` deletes
 * it, with two refinements so a patch never has to restate what it keeps:
 * `exceptions` merges key by key (`null` drops one), and any list field may be
 * `{ "add": […], "remove": […] }` (an `avoid` entry is removed by its `form`). A
 * new ID is appended at the end, so an edit never reorders the file.
 *
 * The patched file is schema-checked with the termbase check's own validators,
 * and the patch is refused when it adds a problem the file didn't have (a
 * pre-existing one is reported, not blamed on the patch). A `concepts.json` edit
 * also reports every locale exception it turns stale, and `dropStaleExceptions`
 * removes them, which is how a shared `notMatch` replaces the per-language
 * workarounds in one step.
 */

import { existsSync, readFileSync } from 'node:fs'
import { BASE_LOCALE, listLocales, loadCatalog, resolveLocaleSource } from './i18n-catalog-lib.ts'
import { decisionHeadings, staleExceptionErrors, validateConcepts, validateTerms } from './i18n-check-termbase.ts'
import {
  conceptsPath,
  englishMatchTexts,
  compileConcepts,
  loadConceptsFor,
  loadTerms,
  proposedConceptsPath,
  termsPath,
} from './i18n-termbase-lib.ts'
import type { Concepts, Termbase } from './i18n-termbase-lib.ts'

/** Which termbase file a patch edits. */
export type TermbaseTarget = { kind: 'terms'; tag: string } | { kind: 'concepts' } | { kind: 'proposed'; tag: string }

/** One concept's patch: field → new value, `null` to delete the field. */
export type EntryPatch = Record<string, unknown>

/** A whole patch file: concept ID → entry patch, or `null` to delete the entry. */
export type TermbasePatch = Record<string, EntryPatch | null>

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

/** A list edit, `{ add, remove }`, in place of a whole new list. */
function isListEdit(value: unknown): value is { add?: unknown[]; remove?: unknown[] } {
  if (!isRecord(value)) return false
  const fields = Object.keys(value)
  return (
    fields.length > 0 &&
    fields.every((field) => field === 'add' || field === 'remove') &&
    fields.every((field) => Array.isArray(value[field]))
  )
}

/** Whether a list item is the one a `remove` names: equal, or (for an `avoid` entry) the same `form`. */
function sameItem(item: unknown, named: unknown): boolean {
  if (isRecord(item) && typeof named === 'string') return item.form === named || item.key === named
  return JSON.stringify(item) === JSON.stringify(named)
}

/** Applies a `{ add, remove }` edit to a stored list (or none): removes first, then adds what isn't there yet. */
function editList(stored: unknown, edit: { add?: unknown[]; remove?: unknown[] }, at: string): unknown[] {
  const list = Array.isArray(stored) ? [...(stored as unknown[])] : []
  for (const named of edit.remove ?? []) {
    const index = list.findIndex((item) => sameItem(item, named))
    if (index === -1) throw new Error(`${at}: can't remove ${JSON.stringify(named)}: it isn't in the list`)
    list.splice(index, 1)
  }
  for (const item of edit.add ?? []) if (!list.some((existing) => sameItem(existing, item))) list.push(item)
  return list
}

/** Merges an `exceptions` patch into the stored map, key by key; `null` drops a key. */
function editExceptions(stored: unknown, patch: Record<string, unknown>, at: string): Map<string, unknown> {
  const merged = new Map(Object.entries(isRecord(stored) ? stored : {}))
  for (const [key, reason] of Object.entries(patch)) {
    if (reason !== null) merged.set(key, reason)
    else if (!merged.delete(key)) throw new Error(`${at}: can't drop exception "${key}": there isn't one`)
  }
  return merged
}

/** One entry with its patch applied. Field order is kept; a new field goes last. */
function patchEntry(
  stored: Record<string, unknown> | undefined,
  patch: EntryPatch,
  at: string,
): Record<string, unknown> {
  const entry = new Map(Object.entries(stored ?? {}))
  for (const [field, value] of Object.entries(patch)) {
    const here = `${at}.${field}`
    if (value === null) entry.delete(field)
    else if (field === 'exceptions' && isRecord(value)) {
      const merged = editExceptions(entry.get(field), value, here)
      if (merged.size > 0) entry.set(field, Object.fromEntries(merged))
      else entry.delete(field)
    } else if (isListEdit(value)) entry.set(field, editList(entry.get(field), value, here))
    else entry.set(field, value)
  }
  return Object.fromEntries(entry)
}

/**
 * The file with the patch applied: entries keep their order, a new ID goes at
 * the end, a `null` entry is deleted.
 * @throws on a patch that can't apply (deleting what isn't there, a malformed entry)
 */
export function applyPatch(file: Record<string, unknown>, patch: TermbasePatch): Record<string, unknown> {
  const out = new Map(Object.entries(file))
  for (const [id, entryPatch] of Object.entries(patch)) {
    if (entryPatch === null) {
      if (!out.delete(id)) throw new Error(`${id}: can't delete it: there's no such entry`)
      continue
    }
    if (!isRecord(entryPatch)) throw new Error(`${id}: a patch entry must be an object of fields, or null`)
    const stored = out.get(id)
    out.set(id, patchEntry(isRecord(stored) ? stored : undefined, entryPatch, id))
  }
  return Object.fromEntries(out)
}

/** The path a target names. */
export function targetPath(target: TermbaseTarget, docsRoot?: string): string {
  if (target.kind === 'concepts') return conceptsPath(docsRoot)
  if (target.kind === 'terms') return termsPath(target.tag, docsRoot)
  return proposedConceptsPath(target.tag, docsRoot)
}

/** Where the termbase and catalogs live, and an unwritten registry to judge a locale's rulings against. */
export interface TermsOptions {
  messagesRoot?: string
  docsRoot?: string
  /** the shared registry as a pending `concepts.json` edit leaves it, in place of the one on disk */
  concepts?: Concepts
}

/** Every schema problem in a target's content, as the termbase check would report it. */
export function targetErrors(target: TermbaseTarget, content: Record<string, unknown>, opts: TermsOptions): string[] {
  const en = loadCatalog(BASE_LOCALE, opts.messagesRoot).messages
  const enKeys = new Set(Object.keys(en))
  if (target.kind === 'concepts') return validateConcepts(content, 'concepts.json', { enKeys })
  const shared = loadConceptsFor(undefined, opts.docsRoot)
  if (target.kind === 'proposed') {
    const label = `${target.tag}/concepts-proposed.json`
    const clashes = Object.keys(content)
      .filter((id) => id in shared)
      .map((id) => `${label}: ${id}: already in concepts.json (propose a new ID)`)
    return [...validateConcepts(content, label, { alsoKnown: new Set(Object.keys(shared)), enKeys }), ...clashes]
  }
  const concepts = { ...loadConceptsFor(target.tag, opts.docsRoot), ...opts.concepts }
  const terms = content as Termbase
  return [
    ...validateTerms({
      tag: target.tag,
      terms,
      knownConcepts: new Set(Object.keys(concepts)),
      enKeys,
      decisionHeadings: decisionHeadings(target.tag, opts.docsRoot),
    }),
    ...staleExceptionErrors(target.tag, terms, concepts, en),
  ]
}

/** What one patch does to one file. */
export interface TermsPlan {
  path: string
  /** the patched content, to write */
  content: Record<string, unknown>
  /** problems the patch would add; any of these refuses it */
  added: string[]
  /** problems the file already had, reported but not blamed on the patch */
  preexisting: string[]
}

/** Plans a patch: the patched content and the problems it adds. Reads only. */
export function planTermsPatch(target: TermbaseTarget, patch: TermbasePatch, opts: TermsOptions = {}): TermsPlan {
  const path = targetPath(target, opts.docsRoot)
  if (target.kind === 'terms' && !existsSync(path)) {
    throw new Error(`${target.tag} has no terms.json yet: copy docs/i18n/_template/ first`)
  }
  const before = existsSync(path) ? (JSON.parse(readFileSync(path, 'utf8')) as Record<string, unknown>) : {}
  const content = applyPatch(before, patch)
  const was = new Set(targetErrors(target, before, opts))
  const now = targetErrors(target, content, opts)
  return {
    path,
    content,
    added: now.filter((line) => !was.has(line)),
    preexisting: now.filter((line) => was.has(line)),
  }
}

/** One locale exception a concept edit made stale: its English no longer matches the concept. */
export interface StaleException {
  tag: string
  concept: string
  key: string
}

/**
 * Every locale `exceptions` entry that `concepts` (a registry about to be
 * written) leaves stale. Reads each full translation's `terms.json`.
 */
export function staleExceptionsUnder(
  concepts: Concepts,
  opts: { messagesRoot?: string; docsRoot?: string } = {},
): StaleException[] {
  const en = loadCatalog(BASE_LOCALE, opts.messagesRoot).messages
  const matchers = compileConcepts(concepts)
  const available = listLocales(opts.messagesRoot)
  const stale: StaleException[] = []
  for (const tag of available) {
    if (tag === BASE_LOCALE || resolveLocaleSource(tag, available).isOverlay) continue
    const terms = loadTerms(tag, opts.docsRoot)
    if (!terms) continue
    for (const [concept, term] of Object.entries(terms)) {
      const hits = matchers.get(concept)
      if (!hits || !isRecord(term.exceptions)) continue
      for (const key of Object.keys(term.exceptions)) {
        if (!(key in en) || englishMatchTexts(key, en[key]).some((text) => hits(text, key))) continue
        stale.push({ tag, concept, key })
      }
    }
  }
  return stale
}

/** The per-locale patches that drop `stale` exceptions, tag → patch. */
export function dropExceptionPatches(stale: readonly StaleException[]): Map<string, TermbasePatch> {
  const patches = new Map<string, TermbasePatch>()
  for (const { tag, concept, key } of stale) {
    const patch = patches.get(tag) ?? {}
    const entry = (patch[concept] ?? { exceptions: {} }) as { exceptions: Record<string, null> }
    entry.exceptions[key] = null
    patch[concept] = entry
    patches.set(tag, patch)
  }
  return patches
}
