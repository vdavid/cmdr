#!/usr/bin/env node
/**
 * TERM CONSISTENCY check (i18n maintenance): WARN class.
 *
 * Catches the defect where **one locale gives one English string two different
 * names**, so the app contradicts itself: the menu item said `命令選擇區…` while
 * the palette it opened said `指令面板`; the menu item `輸入授權金鑰…` opened a
 * dialog titled `輸入授權碼`. Nothing else in the suite sees this. Every other
 * i18n check reads ONE key at a time against its source; this one is the only
 * cross-key check, which is exactly why the class kept shipping.
 *
 * ## What it compares
 *
 * Two or more keys whose SOURCE value is the same (after `normalizeForComparison`)
 * are one term. If the locale renders them differently, that's a finding. It
 * compares what the user actually SEES:
 *
 *  - A full translation (`de`, `zh-Hant`) always has its own value for every key.
 *  - An OVERLAY (`en-GB` over `en`) is judged on its EFFECTIVE value: its own
 *    where it forks a key, the base catalog's where it doesn't. So a half-forked
 *    term ("colour" in one file, "color" still showing in another) is a finding
 *    here even though each key on its own looks fine. That is the overlay failure
 *    mode, and it's worse than not forking at all.
 *
 * ## Why the allowlist demands a REASON
 *
 * Plenty of same-English pairs SHOULD diverge, because English is doing two jobs
 * with one word: `Done` is a screen-reader word after a checklist step in one
 * place and an operation's lifecycle status in another; `Running` is a server
 * process in one and a task in progress in another. Those are right, and no
 * amount of cleverness lets a checker tell them from the drift.
 *
 * So the allowlist entry carries a `reason`, and an empty one doesn't count. That
 * is the whole design: the check can't decide, but it CAN force the boundary to be
 * written down once, next to the term, where the next translator reads it. A
 * silent allowlist would just be a mute button.
 *
 * ## A value that agrees with its label (`@key.agreesWith`)
 *
 * One recurring split is structural, not a judgment: a value word that takes the
 * gender and number of the label beside it (the managed card's per-row "Off":
 * es `Desactivadas` beside `Estadísticas de uso`, against the plain switch option
 * `Desactivado`). The `en` key says so once with `agreesWith: "<label key>"`, and
 * it then groups only with keys sharing both its English and its label, in every
 * locale, with no allowlist entry. A malformed or dangling one fails the run
 * (`agreementProblems`), since it would otherwise drop its key from comparison.
 *
 * ## Locales that haven't been triaged yet
 *
 * Nine locales predate this check and carry 20-40 divergences each. Listing 300
 * findings would train everyone to ignore the check, so `notYetReviewed` records a
 * per-locale COUNT: the check reports one line per such locale and only complains
 * when the number GROWS. It ratchets down on local runs as locales get cleaned, and
 * a locale in neither section is strict from its first day.
 *
 * A baselined locale is judged on the divergences that carry NO reasoned `reviewed`
 * entry, so the two sections compose: record why one split is right, the count drops
 * by one, and the baseline ratchets after it. Without that, the only way to move a
 * baselined locale would be a translation edit, which is the wrong answer whenever
 * the split is the honest one, and untriaged locales are exactly where those live.
 *
 * Run: `pnpm i18n:check-term-consistency` (desktop). Pass `--messages-root <dir>`
 * to point at a fixture (used by the tests).
 */

import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  BASE_LOCALE,
  isRawKey,
  layerCatalogs,
  listLocales,
  loadCatalog,
  resolveLocaleSource,
} from './i18n-catalog-lib.ts'
import type { Catalog } from './i18n-catalog-lib.ts'
import { EXIT_CLEAN, EXIT_ERROR, EXIT_ISSUES } from './i18n-locale-check-lib.ts'

/** Where the accepted splits and the not-yet-triaged baselines live. */
export const ALLOWLIST_PATH: string = join(import.meta.dirname, 'i18n-term-consistency-allowlist.json')

/** One accepted divergence: the normalized source value, and WHY it's right. */
export interface AllowEntry {
  source: string
  reason: string
}

/** The allowlist file's shape. */
export interface Allowlist {
  reviewed: Record<string, AllowEntry[]>
  notYetReviewed: Record<string, number>
}

/** One rendering of a source value, and the keys that carry it. */
export interface Rendering {
  value: string
  keys: string[]
}

/** One source value a locale renders two or more ways. */
export interface DivergenceFinding {
  source: string
  renderings: Rendering[]
}

/**
 * Sentence punctuation that DECORATES a term rather than belonging to it, in the
 * shapes the scripts we ship (or plan to) use: Latin and full-width terminators
 * and separators, the ellipsis, the openers Spanish puts in FRONT of a sentence,
 * the Arabic and Devanagari equivalents. Listed rather than taken from
 * `\p{P}` wholesale, because Unicode files `%`, `/`, `#`, and the brackets under
 * punctuation too, and those are part of a term: `{percent}%` must not normalize
 * to `{percent}`.
 */
const EDGE_PUNCTUATION = String.raw`\s.。．!！?？:：;；,，、…⋯¡¿؟،؛।॥`

/** Runs of `EDGE_PUNCTUATION` at either end of a value. */
const EDGE_RUN = new RegExp(`^[${EDGE_PUNCTUATION}]+|[${EDGE_PUNCTUATION}]+$`, 'gu')

/**
 * Normalizes a value for "is this the same string" comparison.
 *
 * ICU keys double their apostrophes (`doesn''t`) and the raw `errors.*` family
 * does not, so the same sentence in the two families is byte-different and would
 * otherwise read as a divergence. The straight and the curly apostrophe are one
 * too (`couldn't` and `couldn’t`), read as the curly one English writes. Punctuation WRAPPING the term is noise for this
 * question too: a label and the same label with a colon are one term, and so are
 * `Copied`, `Copié !`, and `¡Copiado!`. It's stripped from BOTH ends, since where
 * a script puts its marks is a property of the script, not of the term. CASE is
 * kept, because a sentence-case label and a Title Case menu item are genuinely
 * different house conventions.
 *
 * Gotcha: strip the ends AFTER collapsing whitespace, or French's narrow no-break
 * space before `!` survives as a trailing blank and the term stops matching its
 * plain sibling.
 *
 * @param value the raw catalog value
 * @param key the key it belongs to (decides ICU vs raw apostrophe handling)
 */
export function normalizeForComparison(value: string, key: string): string {
  const unescaped = isRawKey(key) ? value : value.replace(/''/g, "'")
  return unescaped.replace(/'/g, '’').replace(/\s+/gu, ' ').replace(EDGE_RUN, '')
}

/**
 * The label a key's value must agree with, from its `en` `@key.agreesWith`: a
 * value word like the managed card's per-row "Off", which takes the gender and
 * number of the row label beside it. English writes every such value the same
 * ("Off"), so without this they'd group with the plain switch option and with
 * each other, and every agreeing language would read as drift.
 */
function agreesWith(source: Catalog, key: string): string | undefined {
  if (!(key in source.metadata)) return undefined
  const target = source.metadata[key].agreesWith
  return typeof target === 'string' ? target : undefined
}

/**
 * Every malformed `@key.agreesWith` in the source catalog: one that isn't a
 * non-empty string, names the key itself, or names a key the catalog lacks.
 * A dangling one would quietly take its key out of comparison forever, so the
 * check refuses to run on one rather than warning.
 *
 * @param source the `en` catalog (or an overlay's layered source)
 * @returns one human-readable line per problem
 */
export function agreementProblems(source: Catalog): string[] {
  const problems: string[] = []
  for (const [key, meta] of Object.entries(source.metadata)) {
    if (!('agreesWith' in meta)) continue
    const target = meta.agreesWith
    if (typeof target !== 'string' || target.length === 0) {
      problems.push(`@${key}.agreesWith must name the label key its value agrees with`)
    } else if (target === key) {
      problems.push(`@${key}.agreesWith names the key itself; name the label beside it`)
    } else if (!(target in source.messages)) {
      problems.push(`@${key}.agreesWith names "${target}", which isn't a key`)
    }
  }
  return problems
}

/**
 * Every source value carried by two or more keys, mapped to its English and those
 * keys. Values shorter than two characters are skipped: a bare `/` or `%`
 * connector collides across unrelated surfaces and says nothing about terminology.
 * A key with `@key.agreesWith` groups only with keys that share both its English
 * AND its label, since agreeing with a different label is a different word.
 */
function groupBySourceValue(source: Catalog): Map<string, { text: string; keys: string[] }> {
  const groups = new Map<string, { text: string; keys: string[] }>()
  for (const [key, value] of Object.entries(source.messages)) {
    const normalized = normalizeForComparison(value, key)
    if (normalized.length < 2) continue
    const label = agreesWith(source, key)
    const groupKey = label === undefined ? normalized : `${normalized}\u0000${label}`
    const bucket = groups.get(groupKey)
    if (bucket) bucket.keys.push(key)
    else groups.set(groupKey, { text: normalized, keys: [key] })
  }
  return groups
}

/**
 * Finds every source value this locale renders two or more different ways.
 *
 * @param source the catalog the locale is checked against (`en`, or for an
 *   overlay the catalog it overrides)
 * @param catalog the locale's own catalog
 * @param isOverlay whether the locale carries only its forked keys, in which case
 *   an absent key renders the source value rather than nothing
 * @returns findings sorted by source value, each listing its renderings
 */
export function findDivergences(source: Catalog, catalog: Catalog, isOverlay: boolean): DivergenceFinding[] {
  const findings: DivergenceFinding[] = []
  for (const { text: normalizedSource, keys } of groupBySourceValue(source).values()) {
    if (keys.length < 2) continue

    // What the user actually sees for each key: the locale's own value, or for an
    // overlay the base value it falls through to. A full translation with a key
    // genuinely missing is the coverage check's problem, not ours, so we skip it.
    const byRendering = new Map<string, string[]>()
    for (const key of keys) {
      // What the locale renders: its own value, or for an overlay the source value
      // it falls through to. A full translation missing a key is the coverage
      // check's problem, not ours.
      let effective: string
      if (key in catalog.messages) effective = catalog.messages[key]
      else if (isOverlay && key in source.messages) effective = source.messages[key]
      else continue
      const normalized = normalizeForComparison(effective, key)
      const bucket = byRendering.get(normalized)
      if (bucket) bucket.push(key)
      else byRendering.set(normalized, [key])
    }

    if (byRendering.size < 2) continue
    findings.push({
      source: normalizedSource,
      renderings: [...byRendering].map(([value, renderedKeys]) => ({ value, keys: renderedKeys.sort() })),
    })
  }
  return findings.sort((a, b) => a.source.localeCompare(b.source))
}

/**
 * Whether a divergence is an accepted split. An entry with a blank reason does
 * NOT count: the reason is the point of the allowlist.
 *
 * @param source the normalized source value
 * @param entries the locale's accepted splits
 */
export function isAllowed(source: string, entries: readonly AllowEntry[]): boolean {
  return entries.some((entry) => entry.source === source && entry.reason.trim().length > 0)
}

/** Reads the allowlist, tolerating a missing file (everything is then strict). */
export function loadAllowlist(path: string = ALLOWLIST_PATH): Allowlist {
  try {
    const parsed = JSON.parse(readFileSync(path, 'utf8')) as Partial<Allowlist>
    return { reviewed: parsed.reviewed ?? {}, notYetReviewed: parsed.notYetReviewed ?? {} }
  } catch {
    return { reviewed: {}, notYetReviewed: {} }
  }
}

/**
 * One locale's outcome. `baseline` is set only for a not-yet-reviewed locale, and
 * it is compared against `unallowed`, never against every `divergences` entry.
 */
export interface LocaleOutcome {
  locale: string
  isOverlay: boolean
  divergences: DivergenceFinding[]
  unallowed: DivergenceFinding[]
  staleAllows: string[]
  baseline?: number
}

/**
 * Runs the check over every non-`en` locale.
 *
 * @param options.messagesRoot override the `messages/` root (for tests)
 * @param options.allowlist the accepted splits and baselines
 * @returns one outcome per locale, in locale order
 */
export function inspectLocales({
  messagesRoot,
  allowlist,
}: {
  messagesRoot?: string
  allowlist: Allowlist
}): LocaleOutcome[] {
  const available = listLocales(messagesRoot)
  const loaded = new Map<string, Catalog>()
  const load = (tag: string): Catalog => {
    const hit = loaded.get(tag)
    if (hit) return hit
    const fresh = loadCatalog(tag, messagesRoot)
    loaded.set(tag, fresh)
    return fresh
  }

  const base = load(BASE_LOCALE)
  const problems = agreementProblems(base)
  if (problems.length > 0) throw new Error(`Malformed @key.agreesWith in ${BASE_LOCALE}:\n  ${problems.join('\n  ')}`)

  const outcomes: LocaleOutcome[] = []
  for (const locale of available.filter((tag) => tag !== BASE_LOCALE)) {
    const { overrides, isOverlay } = resolveLocaleSource(locale, available)
    // `agreesWith` is a fact about the English key, so an overlay's source keeps
    // `en`'s metadata: layering would let the base locale's `@key` blocks replace it.
    const source = isOverlay
      ? { messages: layerCatalogs(base, load(overrides)).messages, metadata: base.metadata }
      : base
    const divergences = findDivergences(source, load(locale), isOverlay)

    const entries = locale in allowlist.reviewed ? allowlist.reviewed[locale] : []
    const unallowed = divergences.filter((finding) => !isAllowed(finding.source, entries))
    const live = new Set(divergences.map((finding) => finding.source))
    const staleAllows = entries.map((entry) => entry.source).filter((source) => !live.has(source))
    const baseline = locale in allowlist.notYetReviewed ? allowlist.notYetReviewed[locale] : undefined
    outcomes.push({ locale, isOverlay, divergences, unallowed, staleAllows, baseline })
  }
  return outcomes
}

/**
 * The summary sentence for a run with nothing to act on.
 *
 * A baselined locale exits clean on purpose: the count only ratchets down, and
 * the check stays warn-only. What it may NOT do is imply the divergences are
 * gone, so the all-clear names what is still waiting. The unqualified sentence
 * survives for the day every baseline reaches zero, which is the only time it's
 * true.
 *
 * @param outcomes per-locale results
 * @returns the line to print
 */
function allClearLine(outcomes: readonly LocaleOutcome[]): string {
  const awaiting = outcomes.filter(({ baseline }) => baseline !== undefined)
  const untriaged = awaiting.reduce((total, { unallowed }) => total + unallowed.length, 0)
  if (untriaged === 0) return 'Term consistency: every locale names one thing one way (or says why not).'
  return (
    `Term consistency: every triaged locale names one thing one way (or says why not). ` +
    `${String(untriaged)} divergences across ${String(awaiting.length)} ` +
    `${awaiting.length === 1 ? 'locale' : 'locales'} are still untriaged.`
  )
}

/**
 * Renders the report and returns the process exit code.
 *
 * @param outcomes per-locale results
 * @param write sink for one line at a time (default `console.log`)
 */
export function report(outcomes: readonly LocaleOutcome[], write?: (line: string) => void): number {
  const out =
    write ??
    ((line: string) => {
      console.log(line)
    })
  if (outcomes.length === 0) {
    out(`Term consistency: no non-${BASE_LOCALE} locales to check.`)
    return EXIT_CLEAN
  }

  let issues = 0
  for (const { locale, unallowed, staleAllows, baseline } of outcomes) {
    if (baseline !== undefined) {
      const count = unallowed.length
      if (count > baseline) {
        issues++
        out(`${locale}: ${String(count)} divergent terms, up from the recorded ${String(baseline)} (not yet triaged).`)
        out(`  - raise the baseline only with a reason, or fix the new divergence`)
      } else {
        const trend = count < baseline ? ` (down from ${String(baseline)}; ratchet the baseline)` : ''
        out(`${locale}: ${String(count)} divergent terms, not yet triaged${trend}.`)
      }
      issues += staleAllows.length
      for (const source of staleAllows) out(`  - stale allowlist entry: "${source}" no longer diverges; drop it`)
      continue
    }
    if (unallowed.length === 0 && staleAllows.length === 0) {
      out(`${locale}: clean.`)
      continue
    }
    issues += unallowed.length + staleAllows.length
    out(`${locale}: ${String(unallowed.length)} unexplained divergent ${unallowed.length === 1 ? 'term' : 'terms'}`)
    for (const { source, renderings } of unallowed) {
      out(`  - "${source}" renders ${String(renderings.length)} ways:`)
      for (const { value, keys } of renderings) out(`      ${JSON.stringify(value)} ← ${keys.join(', ')}`)
    }
    for (const source of staleAllows) {
      out(`  - stale allowlist entry: "${source}" no longer diverges; drop it`)
    }
  }

  if (issues === 0) {
    out(allClearLine(outcomes))
    return EXIT_CLEAN
  }
  return EXIT_ISSUES
}

/**
 * Ratchets `notYetReviewed` counts down to what the catalogs still leave
 * unexplained, so a cleaned-up locale can't keep spending slack it no longer needs.
 * A term that gained a reasoned `reviewed` entry counts as cleaned up, same as one
 * whose translation was fixed. Never raises a number and never touches `reviewed`.
 * Local runs only; CI reads the file as committed.
 *
 * @returns the locales whose baseline moved
 */
export function shrinkWrap(outcomes: readonly LocaleOutcome[], allowlist: Allowlist, path: string): string[] {
  const lowered: string[] = []
  for (const { locale, unallowed, baseline } of outcomes) {
    if (baseline === undefined || unallowed.length >= baseline) continue
    allowlist.notYetReviewed[locale] = unallowed.length
    lowered.push(locale)
  }
  if (lowered.length > 0) writeFileSync(path, `${JSON.stringify(allowlist, null, 2)}\n`)
  return lowered
}

function main(): void {
  const args = process.argv.slice(2)
  const rootFlag = args.indexOf('--messages-root')
  const messagesRoot = rootFlag === -1 ? undefined : args[rootFlag + 1]
  const allowlist = loadAllowlist()
  const outcomes = inspectLocales({ messagesRoot, allowlist })
  const code = report(outcomes)
  if (!process.env.CI && messagesRoot === undefined) {
    for (const locale of shrinkWrap(outcomes, allowlist, ALLOWLIST_PATH)) {
      console.log(`(ratcheted the ${locale} baseline down)`)
    }
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
