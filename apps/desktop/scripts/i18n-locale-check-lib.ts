/**
 * Reusable scaffolding for the per-locale i18n maintenance checks (stale,
 * placeholder/tag parity, ICU validity, plural coverage, coverage,
 * don't-translate tokens). They all follow the SAME shape and reuse the pieces
 * here, so none of them reinvents locale iteration, catalog loading, overlay
 * resolution, the per-key result accumulator, the human report format, or the
 * Node↔Go exit-code contract.
 *
 * ## The pattern every locale check follows
 *
 * Each check is a Node script that:
 *  1. For every NON-`en` locale (`localesToCheck`), loads that locale's catalog
 *     plus the catalog it's checked against (`resolveLocaleSource`), and inspects
 *     each key, collecting per-key issues into a `LocaleFindings` (via
 *     `newFindings` / `findings.add`).
 *  2. Hands every locale's findings to `reportFindings`, which prints a
 *     screenshot-coverage-style honest report and returns the process exit code:
 *     - `EXIT_CLEAN` (0): no locales to check, or all clean.
 *     - `EXIT_ISSUES` (1): at least one locale has a finding (which the Go wrapper
 *       maps to a WARN or an ERROR, per check).
 *     A genuine script error (can't read a catalog, a crash) throws / exits
 *     `EXIT_ERROR` (2), which the Go wrapper maps to a real check error.
 *
 * `runLocaleCheck` wires those steps together so a check body is just "given the
 * catalog this locale overrides and the locale's own catalog, what's wrong with
 * this locale?" The Go side (`scripts/check/checks/desktop-i18n-*.go`) runs the
 * script and maps the exit code with `RunCommand` + `errors.As(&exitErr)`, exactly
 * like `desktop-message-screenshots-fresh`.
 *
 * ## Full translations vs OVERLAYS
 *
 * A locale is either a full translation of `en` (`de`, `hu`) or an OVERLAY: a
 * regional variant whose language base also ships (`en-GB` over `en`, `pt-PT`
 * over `pt`), carrying ONLY the keys it forks. `resolveLocaleSource` decides
 * which, once per locale, and `runLocaleCheck` hands every check the answer plus
 * the catalog to compare against, so the six checks can never disagree about what
 * a locale overrides. The per-check rule table: `docs/guides/i18n.md` § Overlay
 * catalogs.
 *
 * Pure (no app/runtime imports beyond the catalog lib, no `window`/DOM, no
 * time/RNG): everything is driven off `loadCatalog` / `listLocales` from
 * `i18n-catalog-lib.ts`, with a `messagesRoot` override so tests point at a
 * fixture instead of the real catalogs.
 */

import { BASE_LOCALE, layerCatalogs, listLocales, loadCatalog, resolveLocaleSource } from './i18n-catalog-lib.ts'
import type { Catalog } from './i18n-catalog-lib.ts'

/** Exit codes shared by every locale check, mirrored by the Go wrappers. */
export const EXIT_CLEAN = 0
export const EXIT_ISSUES = 1
export const EXIT_ERROR = 2

/**
 * The non-`en` locales a check must inspect, sorted: full translations and
 * overlays alike. In a repo with only `en` this is empty, so every locale check
 * is a clean no-op.
 * @param messagesRoot override the `messages/` root (for tests)
 */
export function localesToCheck(messagesRoot?: string): string[] {
  return nonBaseLocales(listLocales(messagesRoot))
}

/** The checkable locales among `available`: everything but the base locale itself. */
function nonBaseLocales(available: readonly string[]): string[] {
  return available.filter((locale) => locale !== BASE_LOCALE)
}

/** One per-key issue: `detail` is a short, translator-facing reason. */
export interface Issue {
  key: string
  detail: string
}

/**
 * One locale's accumulated findings: a list of per-key issues, each a `{ key,
 * detail }` pair where `detail` is a short, translator-facing reason. A check adds
 * to it as it walks the locale's keys; `reportFindings` renders it. `isOverlay`
 * rides along so a summary line can phrase itself for the right kind of catalog.
 */
export interface LocaleFindings {
  locale: string
  isOverlay: boolean
  issues: Issue[]
}

/** A `LocaleFindings` plus its `add` recorder, returned by `newFindings`. */
export type FindingsAccumulator = LocaleFindings & { add(key: string, detail: string): void }

/**
 * Starts an empty findings accumulator for one locale.
 * @param locale the locale tag
 * @param isOverlay whether it's an overlay catalog (`resolveLocaleSource`)
 */
export function newFindings(locale: string, isOverlay = false): FindingsAccumulator {
  const issues: Issue[] = []
  return {
    locale,
    isOverlay,
    issues,
    /**
     * Records one issue against a key.
     * @param key
     * @param detail short reason, e.g. "source changed since translation"
     */
    add(key: string, detail: string) {
      issues.push({ key, detail })
    },
  }
}

/**
 * Options for `reportFindings`.
 *
 * - `title`: one-line check title, e.g. "Stale translations".
 * - `findings`: one entry per checked locale (issues may be empty).
 * - `summaryLine`: per-locale summary for a locale WITH issues, given its issue
 *   count and that locale's kind (default: "N stale key(s)"); the issue lines
 *   follow. The kind lets a check phrase an overlay's summary differently, since
 *   the same finding count means something different there.
 * - `write`: sink for one output line at a time (default `console.log`); tests
 *   pass a collector to assert on the rendered report.
 */
export interface ReportFindingsOptions {
  title: string
  findings: LocaleFindings[]
  summaryLine?: SummaryLine
  write?: (line: string) => void
}

/**
 * Renders the one-line summary above a locale's issue lines.
 * @param count how many issues that locale has
 * @param kind the locale's tag and whether it's an overlay
 */
export type SummaryLine = (count: number, kind: { locale: string; isOverlay: boolean }) => string

/**
 * Renders an honest, per-locale report (modeled on the screenshot coverage report:
 * say what's clean, list what isn't, no silent gaps) and returns the process exit
 * code for the whole run.
 *
 * @returns `EXIT_CLEAN` if no locales or all clean, else `EXIT_ISSUES`
 */
export function reportFindings({ title, findings, summaryLine, write }: ReportFindingsOptions): number {
  const out =
    write ??
    ((line: string) => {
      console.log(line)
    })
  if (findings.length === 0) {
    out(`${title}: no non-${BASE_LOCALE} locales to check.`)
    return EXIT_CLEAN
  }

  const summary: SummaryLine = summaryLine ?? ((count) => `${String(count)} stale key(s)`)
  let total = 0
  for (const { locale, isOverlay, issues } of findings) {
    if (issues.length === 0) {
      out(`${locale}: clean.`)
      continue
    }
    total += issues.length
    out(`${locale}: ${summary(issues.length, { locale, isOverlay })}`)
    for (const { key, detail } of issues) out(`  - ${key} → ${detail}`)
  }

  if (total === 0) {
    out(`${title}: all locales clean.`)
    return EXIT_CLEAN
  }
  return EXIT_ISSUES
}

/** The arguments `runLocaleCheck` hands a per-locale `inspectLocale` body. */
export interface InspectLocaleArgs {
  /** the locale being checked */
  locale: string
  /**
   * The catalog this locale is checked against: `en` for a full translation, and
   * for an overlay the catalog it overrides (its language base layered over
   * `en`), which is what a reader would see without the overlay. Resolved once
   * per locale by `resolveLocaleSource`.
   */
  source: Catalog
  /** the tag `source` came from (`en`, or the overlay's language base) */
  overrides: string
  /** whether this locale is an overlay (only its forked keys) or a full translation */
  isOverlay: boolean
  /** the locale's own catalog */
  catalog: Catalog
  findings: FindingsAccumulator
}

/**
 * Options for `runLocaleCheck`.
 *
 * - `title`: check title for the report.
 * - `inspectLocale`: per-locale check body (it mutates `findings`).
 * - `summaryLine`: see `reportFindings`.
 * - `messagesRoot`: override the `messages/` root (for tests).
 * - `write`: output sink, one line at a time (for tests).
 */
export interface RunLocaleCheckOptions {
  title: string
  inspectLocale: (args: InspectLocaleArgs) => void
  summaryLine?: SummaryLine
  messagesRoot?: string
  write?: (line: string) => void
  /**
   * Inspect `en` as well. Off by default, because most of these checks ask how a
   * locale stands against its SOURCE, and `en` is that source. A check whose rule
   * is about a catalog's own syntax rather than its relationship to a source (the
   * ICU/raw family grammar) applies to `en` too, and turns this on. `en` is then
   * handed itself as its source, which is the honest answer for such a rule.
   */
  includeBaseLocale?: boolean
}

/**
 * Wires the standard locale-check loop: for every non-`en` locale, resolve WHICH
 * catalog it's checked against (`resolveLocaleSource`), load both, and run
 * `inspectLocale`; then report. `inspectLocale` gets the source catalog, the
 * locale's own catalog, whether it's an overlay, and a fresh `findings`
 * accumulator to populate; it returns nothing (it mutates `findings`).
 *
 * Resolving the overlay ONCE here is what keeps the individual checks honest: a
 * check never re-derives "what does this locale override", it's handed the
 * answer, so all of them agree.
 *
 * @returns the process exit code (`EXIT_CLEAN` / `EXIT_ISSUES`)
 */
export function runLocaleCheck({
  title,
  inspectLocale,
  summaryLine,
  messagesRoot,
  write,
  includeBaseLocale = false,
}: RunLocaleCheckOptions): number {
  const available = listLocales(messagesRoot)
  const locales = includeBaseLocale ? available : nonBaseLocales(available)
  // One catalog read per tag, however many locales point at it as their source.
  const loaded = new Map<string, Catalog>()
  const load = (tag: string): Catalog => {
    const hit = loaded.get(tag)
    if (hit) return hit
    const fresh = loadCatalog(tag, messagesRoot)
    loaded.set(tag, fresh)
    return fresh
  }

  const findings: LocaleFindings[] = []
  for (const locale of locales) {
    const { overrides, isOverlay } = resolveLocaleSource(locale, available)
    const source = isOverlay ? layerCatalogs(load(BASE_LOCALE), load(overrides)) : load(BASE_LOCALE)
    const acc = newFindings(locale, isOverlay)
    inspectLocale({ locale, source, overrides, isOverlay, catalog: load(locale), findings: acc })
    findings.push(acc)
  }
  return reportFindings({ title, findings, summaryLine, write })
}
