/**
 * Duration and file-rate formatting. Pure, no settings: unlike sizes, these
 * have no binary/SI choice to honor.
 *
 * `$lib/settings/types.ts` has a same-named `formatDuration(ms)` for rendering
 * a duration SETTING's value in the settings UI (milliseconds in, "500ms" /
 * "2m" out). This one takes SECONDS and is the display formatter for elapsed
 * time and ETAs. Don't merge them; they format different things for different
 * surfaces.
 */

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
 * Format a files-per-second rate for the progress surfaces.
 *
 * - `< 3`: 1 decimal (`"0.4 files/s"`, `"1.8 files/s"`). Small values aren't useful as integers.
 * - Rounds to exactly `1`: `"1 file/s"` (singular).
 * - `>= 3`: integer (`"27 files/s"`). Decimal precision adds nothing at high rates.
 *
 * Returns `null` for rates that round to `0.0` so the caller can hide the readout
 * entirely. A "0 files/s" display masks the real (sub-1) rates that
 * heterogeneous-size copies produce.
 */
export function formatFilesPerSecond(rate: number): string | null {
  if (rate < 3) {
    const oneDecimal = Math.round(rate * 10) / 10
    if (oneDecimal === 0) return null
    if (oneDecimal === 1) return '1 file/s'
    return `${oneDecimal.toFixed(1)} files/s`
  }
  return `${String(Math.round(rate))} files/s`
}

/**
 * Format a millisecond duration where sub-second precision matters ("847 ms",
 * "1.4 s", then handing off to {@link formatDuration}'s minute/hour shape).
 * For timings and diagnostics; user-facing ETAs use `formatDuration`.
 */
export function formatMilliseconds(ms: number): string {
  if (ms < 1000) return `${String(Math.round(ms))} ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`
  return formatDuration(seconds(ms / 1000))
}
