#!/usr/bin/env node
/**
 * MECHANICS check (i18n maintenance): schema is ERROR, typography findings are WARN.
 *
 * Each locale declares its typography in `docs/i18n/<tag>/mechanics.json` (quote
 * pairs, apostrophes, required spacing, hedged-grammar patterns; schema in
 * `docs/i18n/termbase.md`). This check holds the locale's catalog VALUES to it:
 *
 *  - `straight-quote`: an ASCII `"`, which UI text never uses, whatever the locale.
 *  - `quote-mark`: a quotation mark (`QUOTE_MARKS`) outside the declared pairs and
 *    apostrophes (`„` in a locale that quotes with `«…»`).
 *  - `ellipsis`: three dots where `…` belongs.
 *  - `spacing` / `hedge`: a hit of one of the locale's own patterns.
 *
 * It reads what the reader sees: ICU values through the runtime's parser (so a
 * doubled `''` is one apostrophe, and a placeholder or plural category is never
 * text), each plural/select branch on its own, a raw family (`errors.*`, `menu.*`)
 * literally. A placeholder is `INSERT_MARK` in the scanned text, so a spacing rule
 * can require a space next to one. A markdown code span is a command the user
 * types verbatim, so it's never scanned. One finding per key and rule, however
 * many hits.
 *
 * Findings are held to a per-locale COUNT baseline
 * (`i18n-mechanics-baseline.json`) that only ratchets down, the same design as the
 * termbase drift baseline. A locale not listed is strict from its first
 * `mechanics.json`; `--adopt <tag>` records its current count once, so a locale
 * can declare its rules first and clean up after. An overlay without its own file
 * inherits its base language's. A locale with neither is skipped: not declared yet.
 *
 * Run: `pnpm i18n:check-mechanics` (desktop). `--list` prints every finding even
 * under the baseline; `--messages-root <dir>` / `--docs-root <dir>` point at fixtures.
 */

import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  INSERT_MARK,
  isRawKey,
  listLocales,
  loadCatalog,
  resolveLocaleSource,
  visibleTextSegments,
} from './i18n-catalog-lib.ts'
import { EXIT_CLEAN, EXIT_ERROR, EXIT_ISSUES } from './i18n-locale-check-lib.ts'
import { QUOTE_MARKS, SEPARATOR_QUOTES, loadMechanicsRaw } from './i18n-mechanics-lib.ts'
import type { Mechanics, MechanicsRule } from './i18n-mechanics-lib.ts'
import { readJsonIfPresent } from './i18n-termbase-lib.ts'

/** Exit code for a schema error: the Go wrapper maps it to a failing check. */
export const EXIT_SCHEMA = 3

/** Where the per-locale finding baselines live. */
export const BASELINE_PATH: string = join(import.meta.dirname, 'i18n-mechanics-baseline.json')

/** The baseline file's shape: locale → how many findings it may still carry. */
export interface MechanicsBaseline {
  $comment?: string
  findings: Record<string, number>
}

const FIELDS = new Set(['quotes', 'apostrophes', 'spacing', 'hedges', '$comment'])
const QUOTE_FIELDS = new Set(['primary', 'nested'])

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)
const isNonEmptyString = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0
// Code points on purpose: a quote mark is one, and none is a multi-code-point grapheme.
const isOneCharacter = (value: unknown): value is string => typeof value === 'string' && Array.from(value).length === 1

/** Schema-checks one locale's `mechanics.json`. Returns one line per problem. */
export function validateMechanics(raw: unknown, tag: string): string[] {
  const at = `${tag}/mechanics.json`
  if (!isRecord(raw)) return [`${at}: must be an object`]
  const errors = unknownFields(at, raw, FIELDS)
  const quotes = isRecord(raw.quotes) ? raw.quotes : {}
  errors.push(...unknownFields(`${at}: quotes`, quotes, QUOTE_FIELDS))
  errors.push(...pairErrors(at, 'primary', quotes.primary))
  if (quotes.nested !== undefined) errors.push(...pairErrors(at, 'nested', quotes.nested))
  if (raw.apostrophes !== undefined) {
    const ok = Array.isArray(raw.apostrophes) && raw.apostrophes.every(isOneCharacter)
    if (!ok) errors.push(`${at}: "apostrophes" must be a list of single characters`)
  }
  errors.push(...ruleErrors(at, 'spacing', raw.spacing), ...ruleErrors(at, 'hedges', raw.hedges))
  return errors
}

function unknownFields(at: string, entry: Record<string, unknown>, fields: ReadonlySet<string>): string[] {
  return Object.keys(entry)
    .filter((field) => !fields.has(field))
    .map((field) => `${at}: unknown field "${field}"`)
}

/** A quote pair: two marks from `QUOTE_MARKS`, never the straight double quote. */
function pairErrors(at: string, which: 'primary' | 'nested', pair: unknown): string[] {
  if (!Array.isArray(pair) || pair.length !== 2 || !pair.every(isOneCharacter)) {
    return [`${at}: "quotes.${which}" must be an opening and a closing quote mark`]
  }
  return pair.flatMap((mark: string) => {
    if (mark === '"') return [`${at}: quotes.${which} names the straight ", which UI text never uses`]
    const known = QUOTE_MARKS.has(mark) || SEPARATOR_QUOTES.has(mark)
    return known ? [] : [`${at}: "${mark}" isn't a quotation mark (quotes.${which})`]
  })
}

/** A `spacing` / `hedges` list: each entry a compiling pattern with a reason. */
function ruleErrors(at: string, field: 'spacing' | 'hedges', list: unknown): string[] {
  if (list === undefined) return []
  if (!Array.isArray(list)) return [`${at}: "${field}" must be a list`]
  return list.flatMap((rule: unknown, index) => {
    if (!isRecord(rule) || !isNonEmptyString(rule.pattern) || !isNonEmptyString(rule.why)) {
      return [`${at}: ${field}[${String(index)}] needs a "pattern" and a "why"`]
    }
    try {
      new RegExp(rule.pattern, 'u')
      return []
    } catch (error) {
      const why = error instanceof Error ? error.message : String(error)
      return [`${at}: ${field}[${String(index)}] pattern doesn't compile: ${why}`]
    }
  })
}

/** What kind of typography slip a finding is. */
export type MechanicsKind = 'straight-quote' | 'quote-mark' | 'ellipsis' | 'spacing' | 'hedge'

/** One key that breaks its locale's declared typography. */
export interface MechanicsIssue {
  key: string
  kind: MechanicsKind
  detail: string
}

/**
 * The text of a value a reader sees, as segments to scan (see the file comment):
 * ICU through the parser, a raw family or unparseable ICU literally with each
 * `{token}` an insert, and code spans and markdown link targets removed.
 */
function scannedSegments(key: string, value: string, tag: string): string[] {
  const unscanned = (text: string) => text.replace(/`[^`\n]*`/g, INSERT_MARK).replace(/\]\([^)\s]*\)/g, ']')
  const segments = isRawKey(key) ? undefined : visibleTextSegments(value, tag)
  if (segments) return segments.map(unscanned)
  const literal = value.replace(/\{[^{}]*\}/g, INSERT_MARK)
  // Unparseable ICU still means ICU: its doubled apostrophe is one on screen.
  return [unscanned(isRawKey(key) ? literal : literal.replace(/''/g, "'"))]
}

/** A locale's rules, compiled once. */
interface CompiledRule {
  kind: 'spacing' | 'hedge'
  re: RegExp
  why: string
}

function compileRules(mechanics: Mechanics): CompiledRule[] {
  const compile = (kind: CompiledRule['kind'], list: MechanicsRule[] | undefined) =>
    (list ?? []).map((rule) => ({ kind, re: new RegExp(rule.pattern, 'u'), why: rule.why }))
  return [...compile('spacing', mechanics.spacing), ...compile('hedge', mechanics.hedges)]
}

/** Every key of `messages` that breaks the declared typography, sorted by key. */
export function findMechanicsIssues({
  tag,
  mechanics,
  messages,
}: {
  tag: string
  mechanics: Mechanics
  messages: Record<string, string>
}): MechanicsIssue[] {
  const { primary, nested } = mechanics.quotes
  const allowed = new Set([...primary, ...(nested ?? []), ...(mechanics.apostrophes ?? [])])
  const rules = compileRules(mechanics)
  const issues: MechanicsIssue[] = []
  for (const key of Object.keys(messages).sort()) {
    const segments = scannedSegments(key, messages[key], tag)
    const text = segments.join('\n')
    const add = (kind: MechanicsKind, detail: string) => issues.push({ key, kind, detail })
    if (text.includes('"')) add('straight-quote', 'a straight " where the locale’s quotation marks belong')
    const foreign = [
      ...new Set(Array.from(text).filter((char) => char !== '"' && QUOTE_MARKS.has(char) && !allowed.has(char))),
    ]
    if (foreign.length > 0) add('quote-mark', `quotation marks outside the declared ones: ${foreign.join(' ')}`)
    if (text.includes('...')) add('ellipsis', 'three dots where … belongs')
    for (const rule of rules) {
      // Per segment, so a pattern never matches across two branches.
      const hit = segments.map((segment) => rule.re.exec(segment)).find((match) => match !== null)
      if (hit) add(rule.kind, `${rule.why}: ${JSON.stringify(hit[0].replaceAll(INSERT_MARK, '{…}'))}`)
    }
  }
  return issues
}

/** One declared locale's findings, and the count it may carry. */
export interface MechanicsLocaleOutcome {
  locale: string
  issues: MechanicsIssue[]
  baseline?: number
}

/** The whole run. */
export interface MechanicsOutcome {
  schemaErrors: string[]
  locales: MechanicsLocaleOutcome[]
}

/**
 * Runs the check over every locale with a `mechanics.json` of its own, or, for an
 * overlay, its base language's. A locale whose file has schema problems isn't
 * scanned: its patterns can't be trusted.
 */
export function inspectMechanics({
  messagesRoot,
  docsRoot,
  baseline,
}: {
  messagesRoot?: string
  docsRoot?: string
  baseline: MechanicsBaseline
}): MechanicsOutcome {
  const schemaErrors: string[] = []
  const locales: MechanicsLocaleOutcome[] = []
  const available = listLocales(messagesRoot)
  const validated = new Map<string, Mechanics | undefined>()
  const declared = (tag: string): Mechanics | undefined => {
    if (!validated.has(tag)) {
      const raw = loadMechanicsRaw(tag, docsRoot)
      const errors = raw === undefined ? [] : validateMechanics(raw, tag)
      schemaErrors.push(...errors)
      validated.set(tag, raw === undefined || errors.length > 0 ? undefined : (raw as Mechanics))
    }
    return validated.get(tag)
  }
  for (const tag of available) {
    const source = resolveLocaleSource(tag, available)
    const mechanics = declared(tag) ?? (source.isOverlay ? declared(source.overrides) : undefined)
    if (mechanics === undefined) continue
    const messages = loadCatalog(tag, messagesRoot).messages
    locales.push({
      locale: tag,
      issues: findMechanicsIssues({ tag, mechanics, messages }),
      baseline: baseline.findings[tag],
    })
  }
  return { schemaErrors, locales }
}

/** Reads the baseline, tolerating a missing file (every locale is then strict). */
export function loadBaseline(path: string = BASELINE_PATH): MechanicsBaseline {
  const raw = readJsonIfPresent(path) as Partial<MechanicsBaseline> | undefined
  return { ...raw, findings: raw?.findings ?? {} }
}

/** One finding as a report line. */
function issueLine({ key, kind, detail }: MechanicsIssue): string {
  return `  - ${key} (${kind}): ${detail}`
}

/**
 * Renders the report and returns the exit code: `EXIT_SCHEMA` on any schema
 * problem, else `EXIT_ISSUES` when a locale's findings grew past its baseline,
 * else `EXIT_CLEAN`.
 */
export function report(outcome: MechanicsOutcome, write?: (line: string) => void, listAll = false): number {
  const out =
    write ??
    ((line: string) => {
      console.log(line)
    })
  if (outcome.schemaErrors.length > 0) {
    out(`Mechanics: ${String(outcome.schemaErrors.length)} schema problems:`)
    for (const error of outcome.schemaErrors) out(`  - ${error}`)
  }
  if (outcome.locales.length === 0) {
    out('Mechanics: no locale has a mechanics.json yet.')
    return outcome.schemaErrors.length > 0 ? EXIT_SCHEMA : EXIT_CLEAN
  }
  const grown = outcome.locales.filter((locale) => reportLocale(locale, out, listAll)).length
  if (outcome.schemaErrors.length > 0) return EXIT_SCHEMA
  if (grown > 0) return EXIT_ISSUES
  const carried = outcome.locales.reduce((total, { issues }) => total + issues.length, 0)
  const n = outcome.locales.length
  const locales = `${String(n)} ${n === 1 ? 'locale' : 'locales'}`
  out(
    carried === 0
      ? `Mechanics: every value in ${locales} follows its declared typography.`
      : `Mechanics: schema clean; ${String(carried)} typography findings across ${locales} still wait for cleanup.`,
  )
  return EXIT_CLEAN
}

/** Prints one locale. Returns whether it grew past its baseline. */
function reportLocale(
  { locale, issues, baseline }: MechanicsLocaleOutcome,
  out: (line: string) => void,
  listAll: boolean,
): boolean {
  const allowed = baseline ?? 0
  const count = issues.length
  if (count > allowed) {
    const from = baseline === undefined ? 'no baseline' : `up from the recorded ${String(baseline)}`
    out(`${locale}: ${String(count)} typography findings (${from}):`)
    for (const issue of issues) out(issueLine(issue))
    out(`  → fix the value, or the locale's mechanics.json if a rule is wrong (docs/i18n/termbase.md)`)
    return true
  }
  const trend = count < allowed ? ` (down from ${String(allowed)}; ratchet the baseline)` : ''
  out(
    count === 0
      ? `${locale}: typography clean.`
      : `${locale}: ${String(count)} typography findings, within the baseline${trend}.`,
  )
  if (listAll) for (const issue of issues) out(issueLine(issue))
  return false
}

function save(baseline: MechanicsBaseline, path: string): void {
  writeFileSync(path, `${JSON.stringify(baseline, null, 2)}\n`)
}

/**
 * Ratchets baselines down to what's left and drops a locale that no longer
 * declares mechanics. Never raises a number. Local runs only.
 *
 * @returns the locales whose baseline moved
 */
export function shrinkWrap(outcome: MechanicsOutcome, baseline: MechanicsBaseline, path: string): string[] {
  const moved: string[] = []
  const counts = new Map(outcome.locales.map(({ locale, issues }) => [locale, issues.length]))
  const kept: Record<string, number> = {}
  for (const [locale, allowed] of Object.entries(baseline.findings)) {
    const count = counts.get(locale)
    if (count === undefined || count < allowed) moved.push(locale)
    if (count !== undefined) kept[locale] = Math.min(count, allowed)
  }
  baseline.findings = kept
  if (moved.length > 0) save(baseline, path)
  return moved.sort()
}

/**
 * Records a newly declared locale's current count, so it can adopt the check
 * before its cleanup lands. Only for a locale with no baseline yet: an existing
 * number only ratchets down.
 *
 * @returns the recorded count
 * @throws when the locale has a baseline already, or no mechanics to count against
 */
export function adopt(outcome: MechanicsOutcome, baseline: MechanicsBaseline, tag: string, path: string): number {
  if (tag in baseline.findings) {
    throw new Error(`${tag} already has a baseline (${String(baseline.findings[tag])}); it only ratchets down`)
  }
  const locale = outcome.locales.find((entry) => entry.locale === tag)
  if (!locale) throw new Error(`${tag} has no mechanics.json (or its schema is broken), so there's nothing to adopt`)
  baseline.findings = Object.fromEntries(
    Object.entries({ ...baseline.findings, [tag]: locale.issues.length }).sort(([a], [b]) => a.localeCompare(b)),
  )
  save(baseline, path)
  locale.baseline = locale.issues.length
  return locale.issues.length
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
  const outcome = inspectMechanics({ messagesRoot, docsRoot, baseline })
  const adopting = flagValue(args, '--adopt')
  if (adopting !== undefined) {
    const count = adopt(outcome, baseline, adopting, BASELINE_PATH)
    console.log(`(recorded ${adopting} at ${String(count)} findings; it ratchets down from here)`)
  }
  const code = report(outcome, undefined, args.includes('--list'))
  const fixture = messagesRoot !== undefined || docsRoot !== undefined
  if (!process.env.CI && !fixture && outcome.schemaErrors.length === 0) {
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
