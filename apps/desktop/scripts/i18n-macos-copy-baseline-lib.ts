/**
 * The frozen macOS copy baseline: what a Mac reader of each shipped locale sees
 * for every key the platform-copy work touches, captured once before any value
 * learned a `{platform, select, …}` block. `src/lib/intl/macos-copy-unchanged.test.ts`
 * holds every later catalog edit to it; `i18n-macos-copy-baseline.ts` wrote it.
 *
 * "What a reader sees" is the RESOLVED value: the locale's own catalog, then the
 * catalogs it inherits from (`inheritableAncestors`, the runtime's rule), then
 * `en`. So an overlay (`en-GB`) contributes every key, forked or inherited, and a
 * fork that vanishes or a base edit that leaks through both show up.
 */
import { join } from 'node:path'
import { inheritableAncestors } from '../src/lib/intl/locale-inheritance.ts'
import { BASE_LOCALE, GENERATED_LOCALES, listLocales, loadCatalog } from './i18n-catalog-lib.ts'

/** The baseline file, relative to `apps/desktop/`. */
export const BASELINE_RELATIVE_PATH = 'src/lib/intl/__fixtures__/macos-copy-baseline.json'

/** The baseline's on-disk shape: locale → key → resolved value. */
export interface MacosCopyBaseline {
  readme: string
  locales: Record<string, Record<string, string>>
}

/** One resolved value and the catalog it came from (`en` when nothing more specific defines it). */
export interface ResolvedValue {
  value: string
  from: string
}

/** The shipped locales the baseline covers: every catalog dir except generated ones (`en-XA`). */
export function baselineLocales(messagesRoot?: string): string[] {
  return listLocales(messagesRoot).filter((locale) => !GENERATED_LOCALES.has(locale))
}

/** The catalogs `locale` reads, most specific first, ending in `en`. */
export function resolutionChain(locale: string, available: readonly string[]): string[] {
  const chain = [locale, ...inheritableAncestors(locale, available), BASE_LOCALE]
  return chain.filter((tag, index) => chain.indexOf(tag) === index)
}

/**
 * Resolves `keys` for every baseline locale from the catalogs on disk.
 * @returns locale → key → resolved value; a key no catalog in the chain defines is absent
 */
export function resolveLocales(
  keys: readonly string[],
  messagesRoot?: string,
): Partial<Record<string, Partial<Record<string, ResolvedValue>>>> {
  const available = baselineLocales(messagesRoot)
  const catalogs = new Map(available.map((locale) => [locale, loadCatalog(locale, messagesRoot).messages]))
  const result: Partial<Record<string, Partial<Record<string, ResolvedValue>>>> = {}
  for (const locale of available) {
    const chain = resolutionChain(locale, available)
    const resolved: Partial<Record<string, ResolvedValue>> = {}
    for (const key of keys) {
      for (const tag of chain) {
        const value = catalogs.get(tag)?.[key]
        if (value !== undefined) {
          resolved[key] = { value, from: tag }
          break
        }
      }
    }
    result[locale] = resolved
  }
  return result
}

/** Whether a catalog value is already a `{platform, select, …}` block (the baseline must predate them). */
function isPlatformBlock(value: string): boolean {
  return value.startsWith('{platform, select,')
}

/**
 * Freezes `keys` that `baseline` lacks, from `resolved`, and leaves every entry it
 * already holds exactly as it was, even where the catalog has moved since. Each
 * locale comes back sorted by key, like the first write, so the file diff is pure
 * additions. Only the baseline's own locales are touched.
 * @returns the grown baseline and how many (locale, key) values it gained
 * @throws when a missing key resolves nowhere or already holds a platform block
 */
export function addMissingEntries(
  baseline: MacosCopyBaseline,
  resolved: Partial<Record<string, Partial<Record<string, ResolvedValue>>>>,
  keys: readonly string[],
  readme: string,
): { baseline: MacosCopyBaseline; added: number } {
  const locales: Record<string, Record<string, string>> = {}
  let added = 0
  for (const [locale, existing] of Object.entries(baseline.locales)) {
    const values: Record<string, string> = { ...existing }
    for (const key of keys) {
      if (key in existing) continue
      const entry = resolved[locale]?.[key]
      if (!entry) throw new Error(`${locale}: no catalog in the chain defines ${key}`)
      if (isPlatformBlock(entry.value)) {
        throw new Error(`${locale} ${key} already holds a platform block; the baseline must predate them`)
      }
      values[key] = entry.value
      added++
    }
    locales[locale] = Object.fromEntries(
      Object.keys(values)
        .sort()
        .map((key) => [key, values[key]]),
    )
  }
  return { baseline: { readme, locales }, added }
}

/** The absolute path of a file under `apps/desktop/`, resolved from this script's dir. */
export function desktopPath(relative: string): string {
  return join(import.meta.dirname, '..', relative)
}
