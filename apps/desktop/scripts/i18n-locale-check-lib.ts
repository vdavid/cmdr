/**
 * Reusable scaffolding for the per-locale i18n maintenance checks.
 *
 * The stale check (M2) is the first consumer; the M3 checks (placeholder/tag
 * parity, ICU validity, plural coverage, key parity, don't-translate tokens) all
 * follow the SAME shape and reuse the pieces here, so M3 never reinvents locale
 * iteration, en+locale catalog loading, the per-key result accumulator, the human
 * report format, or the Node↔Go exit-code contract.
 *
 * ## The pattern an M3 check follows
 *
 * Each check is a Node script that:
 *  1. Loads the `en` base catalog once (`loadBaseCatalog`).
 *  2. For every NON-`en` locale (`localesToCheck`), loads that locale's catalog
 *     and inspects each of its keys, collecting per-key issues into a
 *     `LocaleFindings` (via `newFindings` / `findings.add`).
 *  3. Hands every locale's findings to `reportFindings`, which prints a
 *     screenshot-coverage-style honest report and returns the process exit code:
 *     - `EXIT_CLEAN` (0): no locales to check, or all clean.
 *     - `EXIT_ISSUES` (1): at least one locale has a finding (the warn signal the
 *       Go wrapper maps to a WARN, never a build failure).
 *     A genuine script error (can't read a catalog, a crash) throws / exits
 *     `EXIT_ERROR` (2), which the Go wrapper maps to a real check error.
 *
 * `runLocaleCheck` wires those three steps together so a check body is just "given
 * the en catalog and one locale's catalog, what's wrong with this locale?" The Go
 * side (`scripts/check/checks/desktop-i18n-*.go`) runs the script and maps the
 * exit code with `RunCommand` + `errors.As(&exitErr)`, exactly like
 * `desktop-message-screenshots-fresh`.
 *
 * Pure (no app/runtime imports beyond the M0 catalog lib, no `window`/DOM, no
 * time/RNG): everything is driven off `loadCatalog` / `listLocales` from
 * `i18n-catalog-lib.ts`, with a `messagesRoot` override so tests point at the
 * committed fixture instead of the real catalogs.
 */

import { listLocales, loadCatalog } from './i18n-catalog-lib.ts'
import type { Catalog } from './i18n-catalog-lib.ts'

/** The base (source) locale every other locale is checked against. */
export const BASE_LOCALE = 'en'

/** Exit codes shared by every locale check, mirrored by the Go wrappers. */
export const EXIT_CLEAN = 0
export const EXIT_ISSUES = 1
export const EXIT_ERROR = 2

/**
 * The non-`en` locales a check must inspect, sorted. In a repo with only `en`
 * (today's shipping state) this is empty, so every locale check is a clean no-op.
 * @param messagesRoot override the `messages/` root (for tests)
 */
export function localesToCheck(messagesRoot?: string): string[] {
  return listLocales(messagesRoot).filter((locale) => locale !== BASE_LOCALE)
}

/**
 * Loads the `en` base catalog (messages + `@key` metadata) once, for a check to
 * compare every locale against.
 */
export function loadBaseCatalog(messagesRoot?: string): Catalog {
  return loadCatalog(BASE_LOCALE, messagesRoot)
}

/** One per-key issue: `detail` is a short, translator-facing reason. */
export interface Issue {
  key: string
  detail: string
}

/**
 * One locale's accumulated findings: a list of per-key issues, each a `{ key,
 * detail }` pair where `detail` is a short, translator-facing reason. A check adds
 * to it as it walks the locale's keys; `reportFindings` renders it.
 */
export interface LocaleFindings {
  locale: string
  issues: Issue[]
}

/** A `LocaleFindings` plus its `add` recorder, returned by `newFindings`. */
export type FindingsAccumulator = LocaleFindings & { add(key: string, detail: string): void }

/**
 * Starts an empty findings accumulator for one locale.
 */
export function newFindings(locale: string): FindingsAccumulator {
  const issues: Issue[] = []
  return {
    locale,
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
 *   count (default: "N stale key(s)"); the issue lines follow.
 * - `write`: sink for one output line at a time (default `console.log`); tests
 *   pass a collector to assert on the rendered report.
 */
export interface ReportFindingsOptions {
  title: string
  findings: LocaleFindings[]
  summaryLine?: (count: number) => string
  write?: (line: string) => void
}

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

  const summary = summaryLine ?? ((count) => `${String(count)} stale key(s)`)
  let total = 0
  for (const { locale, issues } of findings) {
    if (issues.length === 0) {
      out(`${locale}: clean.`)
      continue
    }
    total += issues.length
    out(`${locale}: ${summary(issues.length)}`)
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
  locale: string
  base: Catalog
  locale_catalog: Catalog
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
  summaryLine?: (count: number) => string
  messagesRoot?: string
  write?: (line: string) => void
}

/**
 * Wires the standard locale-check loop: load `en` once, run `inspectLocale` for
 * every non-`en` locale, then report. `inspectLocale` gets the base catalog, the
 * locale's catalog, and a fresh `findings` accumulator to populate; it returns
 * nothing (it mutates `findings`).
 *
 * @returns the process exit code (`EXIT_CLEAN` / `EXIT_ISSUES`)
 */
export function runLocaleCheck({
  title,
  inspectLocale,
  summaryLine,
  messagesRoot,
  write,
}: RunLocaleCheckOptions): number {
  const locales = localesToCheck(messagesRoot)
  const base = loadBaseCatalog(messagesRoot)
  const findings: LocaleFindings[] = []
  for (const locale of locales) {
    const localeCatalog = loadCatalog(locale, messagesRoot)
    const acc = newFindings(locale)
    inspectLocale({ locale, base, locale_catalog: localeCatalog, findings: acc })
    findings.push(acc)
  }
  return reportFindings({ title, findings, summaryLine, write })
}
