/**
 * Locale-aware compact durations ("8m 12s", "8min 12s", "8 Min., 12 Sek."),
 * memoized.
 *
 * The output is what `Intl.DurationFormat` gives with `style: 'narrow'`, built
 * from a unit-style `Intl.NumberFormat` per unit plus a unit-type
 * `Intl.ListFormat` to join them. Cmdr can't call `Intl.DurationFormat` itself:
 * it arrived in Safari 16.4, and the app's floor is older (the `safari15` build
 * target, macOS 10.15). The two pieces used here are what the standard composes
 * internally, and `duration.test.ts` pins the result against it.
 *
 * The locale is {@link getUiLocale}: "min" and "Sek." are words in the
 * sentence around them, the same reasoning `list-format.ts` follows.
 */

import { getUiLocale } from './locale'

/** The units a compact duration can name. */
export type DurationUnit = 'hour' | 'minute' | 'second'

/** One duration component: a whole number of one unit. */
export interface DurationPart {
  unit: DurationUnit
  value: number
}

const unitFormatterCache = new Map<string, Intl.NumberFormat>()
const joinerCache = new Map<string, Intl.ListFormat>()

function getUnitFormatter(locale: string, unit: DurationUnit): Intl.NumberFormat {
  const key = `${locale} ${unit}`
  let formatter = unitFormatterCache.get(key)
  if (formatter === undefined) {
    formatter = new Intl.NumberFormat(locale, { style: 'unit', unit, unitDisplay: 'narrow' })
    unitFormatterCache.set(key, formatter)
  }
  return formatter
}

function getJoiner(locale: string): Intl.ListFormat {
  let joiner = joinerCache.get(locale)
  if (joiner === undefined) {
    joiner = new Intl.ListFormat(locale, { type: 'unit', style: 'narrow' })
    joinerCache.set(locale, joiner)
  }
  return joiner
}

/**
 * Formats duration components, largest first, in the UI language's narrow
 * style. The caller decides which components to show (dropping a zero tail,
 * say); this only words them.
 */
export function formatNarrowDuration(parts: readonly DurationPart[]): string {
  const locale = getUiLocale()
  return getJoiner(locale).format(parts.map(({ unit, value }) => getUnitFormatter(locale, unit).format(value)))
}
