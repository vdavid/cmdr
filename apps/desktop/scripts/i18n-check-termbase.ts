#!/usr/bin/env node
/**
 * TERMBASE check (i18n maintenance): schema is ERROR, coverage drift is WARN.
 *
 * The termbase (`docs/i18n/concepts.json` plus each locale's `terms.json`) is what
 * the translation brief hands a translator as settled. Two ways it can lie:
 *
 *  1. **Schema.** A ruling for a concept that doesn't exist, a missing `chosen` /
 *     `confidence` / `sources`, a confidence outside the three words, an `exceptions`
 *     key the English catalog doesn't have, or a `decision` pointing at a heading
 *     `decisions.md` doesn't carry. Each is a broken pointer the brief would print
 *     as fact, so they fail the check (exit `EXIT_SCHEMA`).
 *  2. **Coverage drift.** A shipped key whose English uses a concept while its
 *     translation carries none of the ruling's `chosen` / `accept` forms and isn't
 *     listed in `exceptions`. Either the catalog drifted from the ruling or the
 *     ruling is missing a form or a boundary; both deserve a look, neither breaks
 *     the app. WARN, held to a per-locale COUNT baseline that only ratchets down
 *     (same design as `i18n-check-term-consistency.ts`'s `notYetReviewed`), so a
 *     half-cleaned locale reports one line rather than failing the build. A locale
 *     with no baseline is strict from its first `terms.json`.
 *
 * A platform block is judged per branch: a branch still carrying its English is
 * skipped (pending translation), and a ruling's optional `windows` / `other`
 * forms replace its base forms for that branch (`formsForBranch`).
 *
 * Settings search keyword lists (`isSearchKeywordsKey`) are outside the termbase:
 * never drift, and an `exceptions` entry naming one is a schema error.
 *
 * A locale without `terms.json` is skipped silently: it hasn't been migrated.
 *
 * Run: `pnpm i18n:check-termbase` (desktop). `--messages-root <dir>` and
 * `--docs-root <dir>` point at fixtures; `--list` prints every drifting key even
 * under the baseline (the cleanup view).
 */

import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { PlatformSelectBranch } from '../src/lib/intl/platform-select.ts'
import {
  BASE_LOCALE,
  isSearchKeywordsKey,
  judgedBranchPairs,
  listLocales,
  loadCatalog,
  resolveLocaleSource,
} from './i18n-catalog-lib.ts'
import { EXIT_CLEAN, EXIT_ERROR, EXIT_ISSUES } from './i18n-locale-check-lib.ts'
import {
  CONFIDENCES,
  OS_FORM_BRANCHES,
  compileConcepts,
  formsForBranch,
  isCasedForm,
  notMatchKey,
  conceptsPath,
  decisionsPath,
  duplicateKeyErrors,
  englishMatchText,
  englishMatchTexts,
  isNameKey,
  loadTerms,
  localeValueCarriesTerm,
  decisionPointers,
  parseDecisions,
  proposedConceptsPath,
  resolveDecision,
  readJsonIfPresent,
  readTextIfPresent,
  termsPath,
} from './i18n-termbase-lib.ts'
import type { Concepts, Term, Termbase } from './i18n-termbase-lib.ts'

/** Exit code for a schema error: the Go wrapper maps it to a failing check. */
export const EXIT_SCHEMA = 3

/** Where the per-locale drift baselines live. */
export const BASELINE_PATH: string = join(import.meta.dirname, 'i18n-termbase-baseline.json')

/** The baseline file's shape: per locale, how many drifting keys it may still carry, and how big `decisions.md` may be. */
export interface Baseline {
  $comment?: string
  drift: Record<string, number>
  /** locale → the byte size its `decisions.md` may not grow past; ratchets down only */
  decisionsBytes?: Record<string, number>
}

/** A concept ID: lowercase kebab-case. */
const CONCEPT_ID = /^[a-z0-9]+(?:-[a-z0-9]+)*$/

const CONCEPT_FIELDS = new Set(['en', 'match', 'notMatch', 'sense', 'distinct', 'note'])
const TERM_FIELDS = new Set([
  'chosen',
  'accept',
  'proseAccept',
  'forms',
  'avoid',
  'confidence',
  'sources',
  'note',
  'exceptions',
  'decision',
  ...OS_FORM_BRANCHES,
])
const OS_FORM_FIELDS = new Set(['chosen', 'accept', 'proseAccept', 'avoid', 'note'])

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)
const isNonEmptyString = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0
const isStringList = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((item) => typeof item === 'string')

/**
 * Schema-checks a concept registry.
 *
 * @param raw the parsed file
 * @param label the file as findings name it (`concepts.json`, `nl/concepts-proposed.json`)
 * @param alsoKnown IDs `distinct` may name besides this file's own (the shared registry, for a proposal file)
 * @returns one line per problem
 */
export function validateConcepts(
  raw: unknown,
  label: string,
  { alsoKnown = new Set(), enKeys }: { alsoKnown?: ReadonlySet<string>; enKeys?: ReadonlySet<string> } = {},
): string[] {
  if (!isRecord(raw)) return [`${label}: must be an object of concept ID → concept`]
  const known = new Set([...Object.keys(raw), ...alsoKnown])
  return Object.entries(raw).flatMap(([id, concept]) =>
    conceptErrors({ at: `${label}: ${id}`, id, concept, known, enKeys }),
  )
}

/** Problems with one concept entry, each prefixed with `at`. */
function conceptErrors({
  at,
  id,
  concept,
  known,
  enKeys,
}: {
  at: string
  id: string
  concept: unknown
  known: ReadonlySet<string>
  enKeys: ReadonlySet<string> | undefined
}): string[] {
  const errors = CONCEPT_ID.test(id) ? [] : [`${at}: the ID must be lowercase kebab-case`]
  if (!isRecord(concept)) return [...errors, `${at}: must be an object`]
  errors.push(...unknownFieldErrors(at, concept, CONCEPT_FIELDS))
  if (!isNonEmptyString(concept.en)) errors.push(`${at}: missing "en"`)
  if (!isNonEmptyString(concept.sense)) errors.push(`${at}: missing "sense"`)
  errors.push(...matchErrors(at, concept.match), ...distinctErrors(at, id, concept.distinct, known))
  if (concept.notMatch !== undefined) errors.push(...notMatchErrors(at, concept.notMatch, enKeys))
  if (concept.note !== undefined && typeof concept.note !== 'string') errors.push(`${at}: "note" must be a string`)
  return errors
}

/** Fields outside the schema: almost always a typo that would silently drop data. */
function unknownFieldErrors(at: string, entry: Record<string, unknown>, fields: ReadonlySet<string>): string[] {
  return Object.keys(entry)
    .filter((field) => !fields.has(field))
    .map((field) => `${at}: unknown field "${field}"`)
}

/** `match` must be a non-empty list of non-empty lowercase forms. */
function matchErrors(at: string, match: unknown): string[] {
  if (!isStringList(match) || match.length === 0) return [`${at}: "match" must be a non-empty list of strings`]
  return formErrors(at, 'match', match)
}

/**
 * A `match` / `notMatch` list: strings, each non-empty and lowercase, except a
 * `notMatch` form that's deliberately cased (`isCasedForm`: it matches case for case).
 */
function formErrors(at: string, field: 'match' | 'notMatch', forms: unknown): string[] {
  if (!isStringList(forms)) return [`${at}: "${field}" must be a list of strings`]
  return forms.flatMap((form) => [
    ...(form !== form.toLowerCase() && !(field === 'notMatch' && isCasedForm(form))
      ? [`${at}: ${field} form "${form}" must be lowercase`]
      : []),
    ...(form.replace(/^=/, '').trim().length === 0 ? [`${at}: ${field} has an empty form`] : []),
  ])
}

/**
 * `notMatch`: forms held to the `match` rules, plus key-scoped `{ key, why }`
 * entries naming a real English key (checked when `enKeys` is known) with a reason.
 */
function notMatchErrors(at: string, notMatch: unknown, enKeys: ReadonlySet<string> | undefined): string[] {
  if (!Array.isArray(notMatch)) return [`${at}: "notMatch" must be a list of strings`]
  const forms = notMatch.filter((entry): entry is string => typeof entry === 'string')
  const errors = formErrors(at, 'notMatch', forms)
  for (const entry of notMatch) {
    if (typeof entry === 'string') continue
    const key = notMatchKey(entry)
    if (!isRecord(entry) || key === undefined) {
      errors.push(`${at}: "notMatch" entries must be strings or { key, why } objects`)
      continue
    }
    errors.push(...unknownFieldErrors(`${at}: notMatch key "${key}"`, entry, NOT_MATCH_KEY_FIELDS))
    if (enKeys && !enKeys.has(key)) errors.push(`${at}: notMatch names "${key}", which isn't an English key`)
    if (!isNonEmptyString(entry.why)) errors.push(`${at}: notMatch key "${key}" needs a reason`)
  }
  return errors
}

const NOT_MATCH_KEY_FIELDS = new Set(['key', 'why'])

/** `distinct` must name other, known concepts. */
function distinctErrors(at: string, id: string, distinct: unknown, known: ReadonlySet<string>): string[] {
  if (distinct === undefined) return []
  if (!isStringList(distinct)) return [`${at}: "distinct" must be a list of concept IDs`]
  return distinct.flatMap((other) => [
    ...(known.has(other) ? [] : [`${at}: distinct names unknown concept "${other}"`]),
    ...(other === id ? [`${at}: distinct names itself`] : []),
  ])
}

/** The inputs `validateTerms` checks a termbase against. */
export interface ValidateTermsArgs {
  tag: string
  terms: unknown
  knownConcepts: ReadonlySet<string>
  enKeys: ReadonlySet<string>
  /** every `##` / `###` heading text in the locale's `decisions.md` */
  decisionHeadings: ReadonlySet<string>
}

/** Schema-checks one locale's `terms.json`. Returns one line per problem. */
export function validateTerms(args: ValidateTermsArgs): string[] {
  const label = `${args.tag}/terms.json`
  if (!isRecord(args.terms)) return [`${label}: must be an object of concept ID → ruling`]
  return Object.entries(args.terms).flatMap(([id, term]) => {
    const at = `${label}: ${id}`
    const errors = args.knownConcepts.has(id) ? [] : [`${at}: unknown concept (add it to concepts.json first)`]
    if (!isRecord(term)) return [...errors, `${at}: must be an object`]
    return [
      ...errors,
      ...unknownFieldErrors(at, term, TERM_FIELDS),
      ...termFieldErrors(at, term),
      ...avoidErrors(at, term.avoid),
      ...exceptionErrors(at, term.exceptions, args.enKeys),
      ...decisionErrors(at, term.decision, args),
      ...OS_FORM_BRANCHES.flatMap((branch) => osFormErrors(`${at}: ${branch}`, term[branch])),
    ]
  })
}

/** A ruling's optional per-OS forms (`windows`, `other`): an object with its own `chosen`, held to the base field rules. */
function osFormErrors(at: string, forms: unknown): string[] {
  if (forms === undefined) return []
  if (!isRecord(forms)) return [`${at}: must be an object`]
  const errors = unknownFieldErrors(at, forms, OS_FORM_FIELDS)
  if (!isNonEmptyString(forms.chosen)) errors.push(`${at}: missing "chosen"`)
  for (const field of ['accept', 'proseAccept'] as const) {
    if (forms[field] !== undefined && !isStringList(forms[field])) {
      errors.push(`${at}: "${field}" must be a list of strings`)
    }
  }
  if (forms.note !== undefined && typeof forms.note !== 'string') errors.push(`${at}: "note" must be a string`)
  return [...errors, ...avoidErrors(at, forms.avoid)]
}

/** A value for a message, whatever JSON handed us. */
const shownValue = (value: unknown): string => (value === undefined ? 'undefined' : JSON.stringify(value))

/** The required fields and the plain-typed optional ones. */
function termFieldErrors(at: string, term: Record<string, unknown>): string[] {
  const errors: string[] = []
  if (!isNonEmptyString(term.chosen)) errors.push(`${at}: missing "chosen"`)
  if (!isNonEmptyString(term.sources)) errors.push(`${at}: missing "sources"`)
  if (term.confidence === undefined) errors.push(`${at}: missing "confidence"`)
  else if (!CONFIDENCES.includes(term.confidence as never)) {
    errors.push(`${at}: confidence ${shownValue(term.confidence)} must be one of ${CONFIDENCES.join(', ')}`)
  }
  for (const field of ['accept', 'proseAccept'] as const) {
    if (term[field] !== undefined && !isStringList(term[field]))
      errors.push(`${at}: "${field}" must be a list of strings`)
  }
  for (const field of ['forms', 'note'] as const) {
    if (term[field] !== undefined && typeof term[field] !== 'string') errors.push(`${at}: "${field}" must be a string`)
  }
  return errors
}

/** Each `avoid` entry needs a form and a reason. */
function avoidErrors(at: string, avoid: unknown): string[] {
  if (avoid === undefined) return []
  if (!Array.isArray(avoid)) return [`${at}: "avoid" must be a list`]
  return avoid.flatMap((entry: unknown, index) =>
    isRecord(entry) && isNonEmptyString(entry.form) && isNonEmptyString(entry.why)
      ? []
      : [`${at}: avoid[${String(index)}] needs a "form" and a "why"`],
  )
}

/**
 * Each `exceptions` key must be a real English key, with a reason, and not a
 * search keyword list: drift already skips those, so the entry would excuse nothing.
 */
function exceptionErrors(at: string, exceptions: unknown, enKeys: ReadonlySet<string>): string[] {
  if (exceptions === undefined) return []
  if (!isRecord(exceptions)) return [`${at}: "exceptions" must be an object of key → reason`]
  return Object.entries(exceptions).flatMap(([key, reason]) => [
    ...(enKeys.has(key) ? [] : [`${at}: exception names "${key}", which isn't an English key`]),
    ...(isNonEmptyString(reason) ? [] : [`${at}: exception "${key}" needs a reason`]),
    ...(isSearchKeywordsKey(key)
      ? [`${at}: exception "${key}" is a search keyword list, which no ruling governs: drop it`]
      : []),
  ])
}

/** Each `decision` pointer must resolve to exactly one `decisions.md` heading (`resolveDecision`). */
function decisionErrors(at: string, decision: unknown, { tag, decisionHeadings }: ValidateTermsArgs): string[] {
  if (decision === undefined) return []
  const isList = Array.isArray(decision) && decision.every((entry) => typeof entry === 'string')
  if (typeof decision !== 'string' && !isList) return [`${at}: "decision" must be a heading string or a list of them`]
  return decisionPointers(decision).flatMap((pointer) => {
    const found = resolveDecision(pointer, decisionHeadings)
    if (found.length === 1) return []
    return found.length === 0
      ? [`${at}: decision ${shownValue(pointer)} matches no heading in ${tag}/decisions.md`]
      : [
          `${at}: decision ${shownValue(pointer)} matches ${String(found.length)} headings; lengthen it until one is left`,
        ]
  })
}

/** One shipped key (or one platform branch of it, named in `branch`) that drifts from a ruling. */
export interface DriftFinding {
  key: string
  concept: string
  value: string
  branch?: PlatformSelectBranch
}

/** The inputs `findDrift` compares. */
export interface FindDriftArgs {
  tag: string
  terms: Termbase
  concepts: Concepts
  en: Record<string, string>
  locale: Record<string, string>
}

/**
 * Every (key, concept) pair where the English uses the concept, the locale has a
 * value, that value carries none of the ruling's forms, and the key isn't one of
 * the ruling's `exceptions`. A missing key is the coverage check's business, and a
 * search keyword list (`isSearchKeywordsKey`) is free synonyms, never drift.
 *
 * A platform block is judged branch by branch (`judgedBranchPairs`): each
 * translated branch against the English same branch, with its OS's forms when the
 * ruling has them (`formsForBranch`), and a branch still carrying its English is
 * skipped (pending translation, the stale check's job).
 */
export function findDrift({ tag, terms, concepts, en, locale }: FindDriftArgs): DriftFinding[] {
  const ruled = Object.fromEntries(Object.keys(terms).flatMap((id) => (id in concepts ? [[id, concepts[id]]] : [])))
  const matchers = compileConcepts(ruled)
  const findings: DriftFinding[] = []
  for (const key of Object.keys(en).sort()) {
    if (!(key in locale) || isSearchKeywordsKey(key)) continue
    for (const { name, source, locale: value } of judgedBranchPairs(en[key], locale[key])) {
      const text = englishMatchText(key, source)
      for (const [id, hits] of matchers) {
        if (!hits(text, key)) continue
        const term = terms[id]
        if (term.exceptions && key in term.exceptions) continue
        if (carriesRuling({ tag, key, term, branch: name, source, value })) continue
        findings.push(name === null ? { key, concept: id, value } : { key, concept: id, value, branch: name })
      }
    }
  }
  return findings
}

/**
 * Whether one branch's translation uses its ruling: the forms of the OS that
 * reads it (`formsForBranch`), plus `proseAccept` when the English is prose. A
 * ruling without a `chosen` counts as carried: the schema half reports it, and
 * drift can't be judged without one.
 */
function carriesRuling({
  tag,
  key,
  term,
  branch,
  source,
  value,
}: {
  tag: string
  key: string
  term: Term
  branch: PlatformSelectBranch | null
  source: string
  value: string
}): boolean {
  const forms = formsForBranch(term, branch)
  const chosen: unknown = forms.chosen
  if (!isNonEmptyString(chosen)) return true
  const prose = forms.proseAccept?.length && !isNameKey(key, source) ? forms.proseAccept : []
  return localeValueCarriesTerm(tag, value, { chosen, accept: [...(forms.accept ?? []), ...prose] })
}

/** One migrated locale's drift, and the count it may carry. */
export interface LocaleOutcome {
  locale: string
  drift: DriftFinding[]
  baseline?: number
  /** the size of its `decisions.md` in bytes, when it has one */
  decisionsBytes?: number
  /** the size that file may not grow past (`Baseline.decisionsBytes`) */
  decisionsBudget?: number
}

/** The whole run: schema problems across every file, plus each migrated locale's drift. */
export interface TermbaseOutcome {
  schemaErrors: string[]
  locales: LocaleOutcome[]
}

/** Every `##` / `###` heading of a locale's `decisions.md`. */
export function decisionHeadings(tag: string, docsRoot?: string): Set<string> {
  const markdown = readTextIfPresent(decisionsPath(tag, docsRoot))
  return new Set(markdown === undefined ? [] : parseDecisions(markdown).map((section) => section.heading))
}

/**
 * Exceptions whose key's English no longer hits the concept (its `match` changed,
 * a `notMatch` now covers it, or the English was reworded). A stale exception
 * excuses nothing today and would silently excuse the key again if the English
 * ever matched, so it's an error: drop it, or fix the concept.
 */
export function staleExceptionErrors(
  tag: string,
  terms: Termbase,
  concepts: Concepts,
  en: Record<string, string>,
): string[] {
  const matchers = compileConcepts(concepts)
  const errors: string[] = []
  for (const [id, term] of Object.entries(terms)) {
    const hits = matchers.get(id)
    // A malformed `exceptions` is `validateTerms`' finding; nothing to judge here.
    const exceptions: unknown = term.exceptions
    if (!hits || !isRecord(exceptions)) continue
    for (const key of Object.keys(exceptions)) {
      if (!(key in en) || englishMatchTexts(key, en[key]).some((text) => hits(text, key))) continue
      errors.push(
        `${tag}/terms.json: ${id}: exception "${key}" is stale: its English no longer matches the concept, so drop it`,
      )
    }
  }
  return errors
}

/**
 * The concepts one locale's termbase may name: the shared registry plus its
 * `concepts-proposed.json` (the parallel-migration staging file), whose schema
 * problems and clashes with a shared ID land in `schemaErrors`.
 */
function localeConcepts({
  tag,
  shared,
  docsRoot,
  enKeys,
  schemaErrors,
}: {
  tag: string
  shared: Concepts
  docsRoot: string | undefined
  enKeys: ReadonlySet<string>
  schemaErrors: string[]
}): Concepts {
  const proposed = readJsonIfPresent(proposedConceptsPath(tag, docsRoot))
  if (proposed === undefined) return shared
  const label = `${tag}/concepts-proposed.json`
  schemaErrors.push(...validateConcepts(proposed, label, { alsoKnown: new Set(Object.keys(shared)), enKeys }))
  if (!isRecord(proposed)) return shared
  for (const id of Object.keys(proposed)) {
    if (id in shared) schemaErrors.push(`${label}: ${id}: already in concepts.json (propose a new ID)`)
  }
  return { ...(proposed as Concepts), ...shared }
}

/**
 * Runs the check over every full-translation locale that has a `terms.json`.
 *
 * @param options.messagesRoot override the `messages/` root (for tests)
 * @param options.docsRoot override the `docs/i18n/` root (for tests)
 * @param options.baseline the per-locale drift counts
 */
export function inspectTermbase({
  messagesRoot,
  docsRoot,
  baseline,
}: {
  messagesRoot?: string
  docsRoot?: string
  baseline: Baseline
}): TermbaseOutcome {
  const schemaErrors: string[] = []
  const duplicates = (path: string, label: string) => {
    const text = readTextIfPresent(path)
    if (text !== undefined) schemaErrors.push(...duplicateKeyErrors(text, label))
  }
  duplicates(conceptsPath(docsRoot), 'concepts.json')
  const en = loadCatalog(BASE_LOCALE, messagesRoot)
  const enKeys = new Set(Object.keys(en.messages))
  const sharedRaw = readJsonIfPresent(conceptsPath(docsRoot)) ?? {}
  schemaErrors.push(...validateConcepts(sharedRaw, 'concepts.json', { enKeys }))
  const shared = (isRecord(sharedRaw) ? sharedRaw : {}) as Concepts

  const available = listLocales(messagesRoot)
  const locales: LocaleOutcome[] = []
  for (const tag of available) {
    if (tag === BASE_LOCALE || resolveLocaleSource(tag, available).isOverlay) continue
    const terms = loadTerms(tag, docsRoot)
    if (terms === undefined) continue
    duplicates(termsPath(tag, docsRoot), `${tag}/terms.json`)
    duplicates(proposedConceptsPath(tag, docsRoot), `${tag}/concepts-proposed.json`)

    const concepts = localeConcepts({ tag, shared, docsRoot, enKeys, schemaErrors })
    schemaErrors.push(
      ...validateTerms({
        tag,
        terms,
        knownConcepts: new Set(Object.keys(concepts)),
        enKeys,
        decisionHeadings: decisionHeadings(tag, docsRoot),
      }),
    )
    schemaErrors.push(...staleExceptionErrors(tag, terms, concepts, en.messages))
    const drift = findDrift({ tag, terms, concepts, en: en.messages, locale: loadCatalog(tag, messagesRoot).messages })
    const decisions = readTextIfPresent(decisionsPath(tag, docsRoot))
    locales.push({
      locale: tag,
      drift,
      baseline: baseline.drift[tag],
      decisionsBytes: decisions === undefined ? undefined : Buffer.byteLength(decisions, 'utf8'),
      decisionsBudget: baseline.decisionsBytes?.[tag],
    })
  }
  return { schemaErrors, locales }
}

/** Reads the baseline, tolerating a missing file (every locale is then strict). */
export function loadBaseline(path: string = BASELINE_PATH): Baseline {
  const raw = readJsonIfPresent(path) as Partial<Baseline> | undefined
  return { ...raw, drift: raw?.drift ?? {}, decisionsBytes: raw?.decisionsBytes ?? {} }
}

/** Renders one drift finding. */
function driftLine({ key, concept, value, branch }: DriftFinding): string {
  const where = branch === undefined ? '' : `, ${branch} branch`
  return `  - ${key} (${concept}${where}): ${JSON.stringify(value)}`
}

/**
 * Renders the report and returns the process exit code: `EXIT_SCHEMA` on any
 * schema problem, else `EXIT_ISSUES` when a locale's drift grew past its baseline,
 * else `EXIT_CLEAN`.
 *
 * @param outcome the run's results
 * @param write sink for one line at a time (default `console.log`)
 * @param listAll print every drifting key, even under the baseline
 */
export function report(outcome: TermbaseOutcome, write?: (line: string) => void, listAll = false): number {
  const out =
    write ??
    ((line: string) => {
      console.log(line)
    })
  if (outcome.schemaErrors.length > 0) {
    out(`Termbase: ${String(outcome.schemaErrors.length)} schema problems. The brief would print these as settled:`)
    for (const error of outcome.schemaErrors) out(`  - ${error}`)
  }
  if (outcome.locales.length === 0) {
    out('Termbase: no locale has a terms.json yet.')
    return outcome.schemaErrors.length > 0 ? EXIT_SCHEMA : EXIT_CLEAN
  }

  const grown = outcome.locales.filter((locale) => {
    const drifted = reportLocale(locale, out, listAll)
    return reportDecisionsGrowth(locale, out) || drifted
  }).length
  if (outcome.schemaErrors.length > 0) return EXIT_SCHEMA
  if (grown > 0) return EXIT_ISSUES
  const carried = outcome.locales.reduce((total, { drift }) => total + drift.length, 0)
  const n = outcome.locales.length
  const locales = `${String(n)} ${n === 1 ? 'locale' : 'locales'}`
  out(
    carried === 0
      ? `Termbase: every ruled key in ${locales} uses its ruling.`
      : `Termbase: schema clean; ${String(carried)} drifting keys across ${locales} still wait for cleanup.`,
  )
  return EXIT_CLEAN
}

/**
 * Prints one locale's drift: every finding when it grew past the baseline (the
 * new one is among them), otherwise one line, plus the findings under `listAll`.
 *
 * @returns whether the locale grew past its baseline
 */
function reportLocale(
  { locale, drift, baseline }: LocaleOutcome,
  out: (line: string) => void,
  listAll: boolean,
): boolean {
  const allowed = baseline ?? 0
  const count = drift.length
  if (count > allowed) {
    const from = baseline === undefined ? 'no baseline' : `up from the recorded ${String(baseline)}`
    out(`${locale}: ${String(count)} keys drift from the termbase (${from}):`)
    for (const finding of drift) out(driftLine(finding))
    out(`  → use the ruling's form, add the form to "accept", or record the key in the term's "exceptions" with why`)
    return true
  }
  const trend = count < allowed ? ` (down from ${String(allowed)}; ratchet the baseline)` : ''
  out(
    count === 0
      ? `${locale}: every ruled key uses its ruling.`
      : `${locale}: ${String(count)} drifting keys, within the baseline${trend}.`,
  )
  if (listAll) for (const finding of drift) out(driftLine(finding))
  return false
}

/**
 * Warns when a locale's `decisions.md` grew past its byte budget. The file holds
 * distilled rulings, edited in place; growth usually means an appended story or
 * a superseded entry left beside its replacement.
 *
 * @returns whether it grew past the budget
 */
function reportDecisionsGrowth(
  { locale, decisionsBytes, decisionsBudget }: LocaleOutcome,
  out: (line: string) => void,
): boolean {
  if (decisionsBytes === undefined || decisionsBudget === undefined || decisionsBytes <= decisionsBudget) return false
  out(
    `${locale}/decisions.md grew to ${String(decisionsBytes)} bytes, past its budget of ${String(decisionsBudget)}: ` +
      `distill instead of appending (edit or replace the ruling, delete what's superseded). When new decisions need room, bump its decisionsBytes in i18n-termbase-baseline.json.`,
  )
  return true
}

/**
 * Ratchets one baseline map down to the current values: a lower value moves it,
 * a locale no longer measured is dropped, and with `recordNew` a locale measured
 * for the first time is recorded. Never raises a number.
 *
 * @returns the locales whose entry moved
 */
function ratchet(
  entries: Record<string, number>,
  current: ReadonlyMap<string, number>,
  recordNew: boolean,
): { kept: Record<string, number>; moved: string[] } {
  const moved: string[] = []
  const kept: Record<string, number> = {}
  for (const [locale, allowed] of Object.entries(entries)) {
    const value = current.get(locale)
    if (value === undefined || value < allowed) moved.push(locale)
    if (value !== undefined) kept[locale] = Math.min(value, allowed)
  }
  if (recordNew) {
    for (const [locale, value] of current) {
      if (locale in entries) continue
      kept[locale] = value
      moved.push(locale)
    }
  }
  return { kept: Object.fromEntries(Object.entries(kept).sort(([a], [b]) => a.localeCompare(b))), moved }
}

/**
 * Ratchets baselines down: drift to what's left, each `decisions.md` budget to
 * the file's current size (a file seen for the first time is recorded at its
 * size), and drops a locale that no longer has a termbase. Never raises a
 * number. Local runs only; CI reads the file as committed.
 *
 * @returns the locales whose baseline moved
 */
export function shrinkWrap(outcome: TermbaseOutcome, baseline: Baseline, path: string): string[] {
  const drift = ratchet(
    baseline.drift,
    new Map(outcome.locales.map(({ locale, drift }) => [locale, drift.length])),
    false,
  )
  const sizes = new Map(
    outcome.locales.flatMap(({ locale, decisionsBytes }) =>
      decisionsBytes === undefined ? [] : [[locale, decisionsBytes] as const],
    ),
  )
  const decisions = ratchet(baseline.decisionsBytes ?? {}, sizes, true)
  baseline.drift = drift.kept
  baseline.decisionsBytes = decisions.kept
  const moved = [...new Set([...drift.moved, ...decisions.moved])]
  if (moved.length > 0) writeFileSync(path, `${JSON.stringify(baseline, null, 2)}\n`)
  return moved.sort()
}

/** The value after a `--flag`, or `undefined`. */
function flagValue(args: readonly string[], flag: string): string | undefined {
  const index = args.indexOf(flag)
  return index === -1 ? undefined : args[index + 1]
}

function main(): void {
  const args = process.argv.slice(2)
  const messagesRoot = flagValue(args, '--messages-root')
  const docsRoot = flagValue(args, '--docs-root')
  const baseline = loadBaseline()
  const outcome = inspectTermbase({ messagesRoot, docsRoot, baseline })
  const code = report(outcome, undefined, args.includes('--list'))
  if (!process.env.CI && messagesRoot === undefined && docsRoot === undefined) {
    for (const locale of shrinkWrap(outcome, baseline, BASELINE_PATH)) console.log(`(ratcheted the ${locale} baseline)`)
  }
  process.exit(code)
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  try {
    main()
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(EXIT_ERROR)
  }
}
