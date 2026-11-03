/**
 * "2 days ago", "yesterday", "last month": how long ago something happened, in the UI
 * language, memoized.
 *
 * Written in {@link getCatalogLocale}, like the number and list joins: these are words that sit
 * in the UI's sentences, so they speak the language the catalog does (a `ja-JP` Mac with no `ja`
 * catalog reads "2 days ago"). An absolute date goes through `#lib/settings/format-utils` and
 * the OS region instead.
 *
 * `numeric: 'auto'` gives CLDR's idioms ("yesterday", "last week") where a language has them,
 * which no catalog string could do across languages. An engine without
 * `Intl.RelativeTimeFormat` (Safari before 14) gets `undefined`, and the caller shows a date.
 */

import { getCatalogLocale } from './locale'

const MINUTE = 60
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR
const WEEK = 7 * DAY
const MONTH = 30 * DAY
const YEAR = 365 * DAY

const formatterCache = new Map<string, Intl.RelativeTimeFormat | null>()

function formatter(): Intl.RelativeTimeFormat | null {
  const locale = getCatalogLocale()
  let cached = formatterCache.get(locale)
  if (cached === undefined) {
    // The lib types say it always exists; an old WebKit says otherwise.
    cached =
      typeof (Intl as { RelativeTimeFormat?: unknown }).RelativeTimeFormat === 'function'
        ? new Intl.RelativeTimeFormat(locale, { numeric: 'auto', style: 'long' })
        : null
    formatterCache.set(locale, cached)
  }
  return cached
}

/** The unit and whole count a span of `seconds` reads best in: the largest unit it fills. */
export function relativeTimeParts(seconds: number): { value: number; unit: Intl.RelativeTimeFormatUnit } {
  const span = Math.max(0, seconds)
  if (span < MINUTE) return { value: 0, unit: 'second' }
  if (span < HOUR) return { value: Math.floor(span / MINUTE), unit: 'minute' }
  if (span < DAY) return { value: Math.floor(span / HOUR), unit: 'hour' }
  if (span < WEEK) return { value: Math.floor(span / DAY), unit: 'day' }
  if (span < MONTH) return { value: Math.floor(span / WEEK), unit: 'week' }
  if (span < YEAR) return { value: Math.floor(span / MONTH), unit: 'month' }
  return { value: Math.floor(span / YEAR), unit: 'year' }
}

/**
 * How long before `nowSecs` the moment `thenSecs` was (both unix seconds), as words: "now",
 * "5 minutes ago", "yesterday", "3 weeks ago". A moment in the future (a clock skew) reads as
 * "now". `undefined` where the engine can't say it.
 */
export function formatTimeAgo(thenSecs: number, nowSecs: number): string | undefined {
  const rtf = formatter()
  if (!rtf) return undefined
  const { value, unit } = relativeTimeParts(nowSecs - thenSecs)
  return rtf.format(-value, unit)
}
