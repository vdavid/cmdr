/**
 * Locale-aware number formatting, memoized.
 *
 * `Intl.NumberFormat` construction costs ~10× a format call, and the count and
 * size formatters run per-visible-entry in render AND again in the
 * column-measurement fold over the prefetch buffer. Constructing per call would
 * regress scroll/measure performance on large directories, so we cache each
 * `Intl.NumberFormat` by (locale, options) and rebuild only when the active
 * locale changes. Mirrors the lazy-singleton shape of
 * `getSystemLocaleFormatter()` in `$lib/settings/format-utils`.
 *
 * The locale always comes from {@link getFormatLocale}, which follows the OS
 * rather than the UI-language setting; this module never resolves a locale
 * itself.
 */

import { getFormatLocale } from './locale'

/** Cache keyed by `${locale} ${JSON.stringify(options)}`. */
const formatterCache = new Map<string, Intl.NumberFormat>()

/**
 * A memoized `Intl.NumberFormat` for the active locale and the given options.
 * Callers pass the options they need (fraction digits, grouping); identical
 * (locale, options) pairs share one instance.
 */
export function getNumberFormatter(options: Intl.NumberFormatOptions): Intl.NumberFormat {
  const locale = getFormatLocale()
  const key = `${locale} ${JSON.stringify(options)}`
  let formatter = formatterCache.get(key)
  if (formatter === undefined) {
    formatter = new Intl.NumberFormat(locale, options)
    formatterCache.set(key, formatter)
  }
  return formatter
}

/**
 * Format an integer count with locale-aware thousands grouping (e.g. `1,234`
 * in en-US, `1.234` in de-DE). The single helper behind `formatNumber` and
 * every other user-facing count readout.
 */
export function formatInteger(n: number): string {
  return getNumberFormatter({ maximumFractionDigits: 0 }).format(n)
}

/** Cache for the per-locale grouping separator. */
const groupSeparatorCache = new Map<string, string>()

/**
 * The active locale's thousands-group separator (e.g. `,` in en-US, `.` in
 * de-DE, a thin/narrow no-break space in fr-FR). Derived from the same
 * `Intl.NumberFormat` the counts use, so byte-triad grouping agrees with
 * counts. Memoized per locale.
 */
export function getGroupSeparator(): string {
  const locale = getFormatLocale()
  let separator = groupSeparatorCache.get(locale)
  if (separator === undefined) {
    // 11111 is large enough to force one group boundary in every locale.
    separator = new Intl.NumberFormat(locale).formatToParts(11111).find((p) => p.type === 'group')?.value ?? ''
    groupSeparatorCache.set(locale, separator)
  }
  return separator
}

/** Test seam: drop both memoization caches so a memoization assertion starts clean. */
export function _clearCachesForTests(): void {
  formatterCache.clear()
  groupSeparatorCache.clear()
}
