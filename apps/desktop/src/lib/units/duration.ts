/**
 * Duration and file-rate formatting. No settings to honor: unlike sizes, these
 * have no binary/SI choice. The numbers still go through `$lib/intl`'s locale
 * formatter, because a decimal mark is the locale's business: a rate built with
 * `toFixed` puts an ASCII dot beside a pane reading "250,00 MB".
 *
 * `$lib/settings/types.ts` has a same-named `formatDuration(ms)` for rendering
 * a duration SETTING's value in the settings UI (milliseconds in, "500ms" /
 * "2m" out). This one takes SECONDS and is the display formatter for elapsed
 * time and ETAs. Don't merge them; they format different things for different
 * surfaces.
 */

import { formatInteger, getNumberFormatter } from '$lib/intl/number-format'

/**
 * A duration in seconds, distinct from a byte count or a plain tally. Same
 * reasoning as `ByteCount` in `byte-size.ts`: the brand is what stops an ETA
 * from being formatted as a size, or vice versa.
 */
declare const secondsBrand: unique symbol
export type Seconds = number & { readonly [secondsBrand]: 'seconds' }

/** Brand a plain number as a count of seconds. Compiles away; no runtime cost. */
export function seconds(count: number): Seconds {
  return count as Seconds
}

/**
 * Format seconds as a human-readable duration ("45s", "2m 30s", "1h 5m").
 *
 * The canonical duration formatter for every ETA and elapsed-time readout, so
 * the copy dialog and the operation queue can't phrase the same number two
 * ways. Takes a branded {@link Seconds}: brand at the edge with `seconds(...)`
 * so a byte count or a file tally can't arrive here by accident.
 */
export function formatDuration(totalSeconds: Seconds): string {
  if (totalSeconds < 60) return `${String(Math.round(totalSeconds))}s`
  if (totalSeconds < 3600) {
    const mins = Math.floor(totalSeconds / 60)
    const secs = Math.round(totalSeconds % 60)
    return secs > 0 ? `${String(mins)}m ${String(secs)}s` : `${String(mins)}m`
  }
  const hours = Math.floor(totalSeconds / 3600)
  const mins = Math.round((totalSeconds % 3600) / 60)
  return mins > 0 ? `${String(hours)}h ${String(mins)}m` : `${String(hours)}h`
}

/**
 * A files-per-second rate ready for the catalog: the number as it will be shown,
 * and the same number for the plural selector.
 *
 * ❌ Not a finished string. "files/s" is user-facing copy, so it lives in
 * `fileOperations.shared.fileRate` where a translator can reach it and give it
 * their language's own plural forms — the same split
 * `fileOperations.shared.byteRate` makes for transfer speed.
 */
export interface FileRateReadout {
  /** Grouped and decimal-separated for the active locale ("0.4", "1,500"). */
  text: string
  /** The SHOWN value, so the words the catalog picks match the digits beside them. */
  value: number
}

/**
 * Round a files-per-second rate to what the progress surfaces show.
 *
 * - `< 3`: 1 decimal (`0.4`, `1.8`). Small values aren't useful as integers.
 * - Rounds to exactly `1`: a bare `1`, no tenth (see below).
 * - `>= 3`: integer (`27`). Decimal precision adds nothing at high rates.
 *
 * Returns `null` for rates that round to `0.0` so the caller can hide the readout
 * entirely. A "0 files/s" display masks the real (sub-1) rates that
 * heterogeneous-size copies produce.
 *
 * The `value` it hands back is the ROUNDED one, never the raw rate: the catalog
 * selects a plural form from it, and a form chosen from 0.97 while the reader
 * sees "1" is how "1 files/s" reaches a screen.
 *
 * ⚠️ Exactly 1 is the one rate where a shown tenth and the plural selector
 * DISAGREE, so it drops the tenth. CLDR reads a visible "1.0" as `other` in
 * en/de/nl/sv ("1.0 files") while the selector for the number 1 is `one`, and
 * `Intl.PluralRules` can't be told about fraction digits through ICU. Printing
 * "1" makes the digits and the noun agree in every locale instead.
 */
export function formatFilesPerSecond(rate: number): FileRateReadout | null {
  if (rate < 3) {
    const oneDecimal = Math.round(rate * 10) / 10
    if (oneDecimal === 0) return null
    if (oneDecimal === 1) return { text: formatInteger(1), value: 1 }
    return { text: formatTenth(oneDecimal), value: oneDecimal }
  }
  const whole = Math.round(rate)
  return { text: formatInteger(whole), value: whole }
}

/**
 * Format a millisecond duration where sub-second precision matters ("847 ms",
 * "1.4 s", then handing off to {@link formatDuration}'s minute/hour shape).
 * For timings and diagnostics; user-facing ETAs use `formatDuration`.
 */
export function formatMilliseconds(ms: number): string {
  if (ms < 1000) return `${formatInteger(Math.round(ms))} ms`
  if (ms < 60_000) return `${formatTenth(ms / 1000)} s`
  return formatDuration(seconds(ms / 1000))
}

/** One fraction digit, with the active locale's decimal mark. */
function formatTenth(value: number): string {
  return getNumberFormatter({ minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(value)
}
