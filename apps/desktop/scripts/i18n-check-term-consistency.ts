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
 * ## Locales that haven't been triaged yet
 *
 * Nine locales predate this check and carry 20-40 divergences each. Listing 300
 * findings would train everyone to ignore the check, so `notYetReviewed` records a
 * per-locale COUNT: the check reports one line per such locale and only complains
 * when the number GROWS. It ratchets down on local runs as locales get cleaned, and
 * a locale in neither section is strict from its first day.
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
 * Normalizes a value for "is this the same string" comparison.
 *
 * ICU keys double their apostrophes (`doesn''t`) and the raw `errors.*` family
 * does not, so the same sentence in the two families is byte-different and would
 * otherwise read as a divergence. Trailing ellipsis (either shape) and sentence
 * punctuation are noise for this question too: a label and the same label with a
 * colon are one term. CASE is kept, because a sentence-case label and a Title
 * Case menu item are genuinely different house conventions.
 *
 * @param value the raw catalog value
 * @param key the key it belongs to (decides ICU vs raw apostrophe handling)
 */
export function normalizeForComparison(value: string, key: string): string {
  const unescaped = isRawKey(key) ? value : value.replace(/''/g, "'")
  return unescaped
    .replace(/…|⋯|\.\.\./g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/[.:!?。！：？]+$/u, '')
}

/**
 * Every source value carried by two or more keys, mapped to those keys. Values
 * shorter than two characters are skipped: a bare `/` or `%` connector collides
 * across unrelated surfaces and says nothing about terminology.
 */
function groupBySourceValue(source: Catalog): Map<string, string[]> {
  const groups = new Map<string, string[]>()
  for (const [key, value] of Object.entries(source.messages)) {
    const normalized = normalizeForComparison(value, key)
    if (normalized.length < 2) continue
    const bucket = groups.get(normalized)
    if (bucket) bucket.push(key)
    else groups.set(normalized, [key])
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
  for (const [normalizedSource, keys] of groupBySourceValue(source)) {
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

/** One locale's outcome. `baseline` is set only for a not-yet-reviewed locale. */
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

  const outcomes: LocaleOutcome[] = []
  for (const locale of available.filter((tag) => tag !== BASE_LOCALE)) {
    const { overrides, isOverlay } = resolveLocaleSource(locale, available)
    const source = isOverlay ? layerCatalogs(load(BASE_LOCALE), load(overrides)) : load(BASE_LOCALE)
    const divergences = findDivergences(source, load(locale), isOverlay)

    const baseline = locale in allowlist.notYetReviewed ? allowlist.notYetReviewed[locale] : undefined
    if (baseline !== undefined) {
      outcomes.push({ locale, isOverlay, divergences, unallowed: [], staleAllows: [], baseline })
      continue
    }
    const entries = locale in allowlist.reviewed ? allowlist.reviewed[locale] : []
    const unallowed = divergences.filter((finding) => !isAllowed(finding.source, entries))
    const live = new Set(divergences.map((finding) => finding.source))
    const staleAllows = entries.map((entry) => entry.source).filter((source) => !live.has(source))
    outcomes.push({ locale, isOverlay, divergences, unallowed, staleAllows })
  }
  return outcomes
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
  for (const { locale, divergences, unallowed, staleAllows, baseline } of outcomes) {
    if (baseline !== undefined) {
      const count = divergences.length
      if (count > baseline) {
        issues++
        out(`${locale}: ${String(count)} divergent terms, up from the recorded ${String(baseline)} (not yet triaged).`)
        out(`  - raise the baseline only with a reason, or fix the new divergence`)
      } else {
        const trend = count < baseline ? ` (down from ${String(baseline)}; ratchet the baseline)` : ''
        out(`${locale}: ${String(count)} divergent terms, not yet triaged${trend}.`)
      }
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
    out('Term consistency: every locale names one thing one way (or says why not).')
    return EXIT_CLEAN
  }
  return EXIT_ISSUES
}

/**
 * Ratchets `notYetReviewed` counts down to what the catalogs actually carry, so a
 * cleaned-up locale can't keep spending slack it no longer needs. Never raises a
 * number and never touches `reviewed`. Local runs only; CI reads the file as
 * committed.
 *
 * @returns the locales whose baseline moved
 */
export function shrinkWrap(outcomes: readonly LocaleOutcome[], allowlist: Allowlist, path: string): string[] {
  const lowered: string[] = []
  for (const { locale, divergences, baseline } of outcomes) {
    if (baseline === undefined || divergences.length >= baseline) continue
    allowlist.notYetReviewed[locale] = divergences.length
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
