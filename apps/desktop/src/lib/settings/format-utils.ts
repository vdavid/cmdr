/**
 * Formatting utilities for settings-based display.
 * These functions are pure and don't need reactive state.
 */

import type { DateTimeFormat, FileSizeFormat, FileSizeUnit } from './types'
import { tierForYear, tierForMonth, tierForDay, tierForTime, type AgeTierClass } from './age-tier-utils'

/**
 * One rendered fragment of a formatted date. Components carry the age-tier
 * class for the moment they represent (year/month/day/time), or `null` when
 * the segment should render in the default text color (literals like `-`, `:`
 * separators always render plain). Renderers wrap any segment whose
 * `ageClass !== null` in `<span class={ageClass}>` and pass the rest through.
 */
export interface DateSegment {
  text: string
  ageClass: AgeTierClass | null
}

/**
 * The full result of formatting a timestamp for display in the UI.
 *
 * This is the single source of truth: every site that shows a modified date
 * to the user should obtain it via {@link formatDateForDisplay} (or its
 * reactive wrapper `formattedDate` in `reactive-settings.svelte.ts`). New
 * date-touching code should never reach for `Date#toLocaleString` or
 * hand-rolled formatters; that's how locale, format, and age-coloring drift
 * across components.
 *
 * `segments` is the ordered list of fragments; concatenating their `text`
 * reproduces the plain string in `text`. Each segment knows whether it should
 * be wrapped in an age-tier span via its `ageClass`, so the renderer doesn't
 * have to call any tier helper itself.
 */
export interface FormattedDate {
  /** The joined plain string. Use for tooltips, MCP responses, clipboard
   *  copies, and other plain-text needs. */
  text: string
  /** Ordered list of colored segments. */
  segments: DateSegment[]
}

/** Empty result used for `null` / `0` timestamps. */
const EMPTY_DATE: FormattedDate = {
  text: '',
  segments: [],
}

/**
 * Concatenate segments back to their plain string form. Useful for callers
 * that need a half as text (column-width measurement, etc.).
 */
export function joinSegments(segments: DateSegment[]): string {
  let s = ''
  for (const seg of segments) s += seg.text
  return s
}

/**
 * Format a timestamp for display. The single entry point for everything the
 * UI shows about a date: locale-aware text plus per-component age tiers
 * (year, month, day, time), so the renderer can color each segment
 * independently.
 *
 * Returns the empty result for `null`, `undefined`, or `0` timestamps.
 * virtual git entries arrive as `null` over the wire and a few legacy code
 * paths use `0` as a sentinel; treat both as absent.
 *
 * @param timestamp Unix timestamp in seconds
 * @param format Date/time format mode
 * @param customFormat Format string used when `format === 'custom'`
 * @param nowMs Override for "now" in milliseconds (for tests/snapshots). Defaults to `Date.now()`.
 */
export function formatDateForDisplay(
  timestamp: number | null | undefined,
  format: DateTimeFormat,
  customFormat: string,
  nowMs: number = Date.now(),
): FormattedDate {
  if (timestamp == null || timestamp === 0) return EMPTY_DATE

  const date = new Date(timestamp * 1000)

  // Precompute per-component tiers once; every segment of the same type
  // shares the same age class within a single formatted date.
  const tiers: ComponentTiers = {
    year: tierForYear(timestamp, nowMs),
    month: tierForMonth(timestamp, nowMs),
    day: tierForDay(timestamp, nowMs),
    time: tierForTime(timestamp, nowMs),
  }

  const segments = (() => {
    switch (format) {
      case 'system':
        return systemLocaleSegments(date, tiers)
      case 'iso':
        return applyTokens(date, 'YYYY-MM-DD HH:mm', tiers)
      case 'short':
        return applyTokens(date, 'MM/DD HH:mm', tiers)
      case 'custom':
        return applyTokens(date, customFormat, tiers)
      default:
        return applyTokens(date, 'YYYY-MM-DD HH:mm', tiers)
    }
  })()

  return { text: joinSegments(segments), segments }
}

/** Per-component tier classes for a single timestamp + "now" pair. */
interface ComponentTiers {
  year: AgeTierClass | null
  month: AgeTierClass | null
  day: AgeTierClass | null
  time: AgeTierClass | null
}

/**
 * Token pattern matching the supported components. Order matters: `YYYY`
 * comes before `MM`/`DD` so the longer pattern wins, and `mm`/`HH`/`ss` are
 * separate so we can keep `MM` (month) distinct from `mm` (minutes).
 */
const TOKEN_RE = /YYYY|MM|DD|HH|mm|ss/g

type ComponentKey = 'year' | 'month' | 'day' | 'time'

/**
 * Apply our supported tokens (YYYY/MM/DD/HH/mm/ss) to a format string,
 * emitting one segment per matched token plus one literal segment per
 * inter-token gap. Each token segment carries the precomputed age class for
 * its component category (year/month/day/time); literal segments and tokens
 * with no tier (e.g. month tokens when the file is in a different year)
 * render plain.
 */
function applyTokens(date: Date, format: string, tiers: ComponentTiers): DateSegment[] {
  const pad = (n: number) => String(n).padStart(2, '0')
  const segments: DateSegment[] = []
  let lastIndex = 0

  const pushLiteral = (text: string) => {
    if (text.length > 0) segments.push({ text, ageClass: null })
  }

  for (const match of format.matchAll(TOKEN_RE)) {
    const token = match[0]
    const start = match.index
    pushLiteral(format.slice(lastIndex, start))
    lastIndex = start + token.length

    let text: string
    let component: ComponentKey
    switch (token) {
      case 'YYYY':
        text = String(date.getFullYear())
        component = 'year'
        break
      case 'MM':
        text = pad(date.getMonth() + 1)
        component = 'month'
        break
      case 'DD':
        text = pad(date.getDate())
        component = 'day'
        break
      case 'HH':
        text = pad(date.getHours())
        component = 'time'
        break
      case 'mm':
        text = pad(date.getMinutes())
        component = 'time'
        break
      case 'ss':
        text = pad(date.getSeconds())
        component = 'time'
        break
      default:
        // Unreachable: TOKEN_RE only matches the above.
        text = token
        component = 'time'
    }
    segments.push({ text, ageClass: tiers[component] })
  }
  pushLiteral(format.slice(lastIndex))

  return segments
}

/**
 * Build the parts for the `'system'` format using
 * `Intl.DateTimeFormat#formatToParts`. The component type is identified
 * structurally (`type: 'year' | 'month' | 'day' | 'hour' | 'minute' | 'second'`
 * → our four buckets, no string parsing, no locale assumptions.
 *
 * We mirror `Date#toLocaleString`'s shape (short date + medium time) so
 * existing dev/user expectations don't shift visibly with this refactor.
 */
function ageClassForIntlPart(type: Intl.DateTimeFormatPartTypes, tiers: ComponentTiers): AgeTierClass | null {
  switch (type) {
    case 'year':
      return tiers.year
    case 'month':
      return tiers.month
    case 'day':
      return tiers.day
    case 'hour':
    case 'minute':
    case 'second':
      return tiers.time
    default:
      return null
  }
}

/**
 * Lazily constructed `Intl.DateTimeFormat` for the `'system'` format. The
 * instance depends only on the runtime locale + options, both of which are
 * stable for the life of the page, so one formatter serves every call.
 * Constructing one per call is ~10× the cost of `formatToParts` itself, which
 * adds up across virtualized file-list re-renders.
 *
 * We request fixed-width components (`2-digit` month/day/hour/minute, numeric
 * year) rather than `dateStyle: 'short'`, which in many locales drops the
 * zero-padding (en-US `2/3/25`). Fixed widths let the file-list date column
 * line up across rows under `font-variant-numeric: tabular-nums`. The locale
 * still owns field order, separators, and the 12-/24-hour choice, so this stays
 * the native format, just padded. The hour-cycle is left to the locale.
 */
let systemLocaleFormatter: Intl.DateTimeFormat | null = null

function getSystemLocaleFormatter(): Intl.DateTimeFormat {
  if (systemLocaleFormatter === null) {
    systemLocaleFormatter = new Intl.DateTimeFormat(undefined, {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    })
  }
  return systemLocaleFormatter
}

function systemLocaleSegments(date: Date, tiers: ComponentTiers): DateSegment[] {
  const parts = getSystemLocaleFormatter().formatToParts(date)
  const segments: DateSegment[] = []
  for (const p of parts) {
    segments.push({ text: p.value, ageClass: ageClassForIntlPart(p.type, tiers) })
  }
  return segments
}

// ============================================================================
// File Size Formatting
// ============================================================================

// Binary units (base 1024) - traditional computing units
const binaryUnits = ['bytes', 'KB', 'MB', 'GB', 'TB', 'PB']

// SI units (base 1000) - International System of Units
const siUnits = ['bytes', 'kB', 'MB', 'GB', 'TB', 'PB']

/**
 * The user-facing label for `kB`/`MB`/`GB` under the current
 * binary/SI base. Binary mode shows `KB` (uppercase), SI shows `kB`. `MB` and
 * `GB` are the same in both, but we route them through one helper so callers
 * never hand-pick the casing.
 */
export function unitLabel(unit: 'kB' | 'MB' | 'GB', format: FileSizeFormat): string {
  if (unit === 'kB') return format === 'binary' ? 'KB' : 'kB'
  return unit
}

/**
 * Format bytes as a human-readable string.
 *
 * Without `forceUnit`, picks the friendliest unit per value (the "dynamic"
 * behavior). With `forceUnit` (`'kB'`/`'MB'`/`'GB'`), always renders in that
 * unit so sizes are apples-to-apples across a directory. The base (1024 vs
 * 1000) and the kilobyte label casing both come from `format`.
 *
 * `bytes` mode is not handled here — callers route raw-byte rendering through
 * `formatSizeTriads` for the colored triad treatment.
 *
 * @param bytes Number of bytes
 * @param format 'binary' uses 1024-based (KB/MB/GB), 'si' uses 1000-based (kB/MB/GB)
 * @param forceUnit Optional fixed unit to render in
 */
export function formatFileSizeWithFormat(
  bytes: number,
  format: FileSizeFormat,
  forceUnit?: 'kB' | 'MB' | 'GB',
): string {
  const base = format === 'binary' ? 1024 : 1000
  const units = format === 'binary' ? binaryUnits : siUnits

  if (forceUnit) {
    const power = forceUnit === 'kB' ? 1 : forceUnit === 'MB' ? 2 : 3
    const value = bytes / base ** power
    return `${value.toFixed(2)} ${unitLabel(forceUnit, format)}`
  }

  let value = bytes
  let unitIndex = 0
  while (value >= base && unitIndex < units.length - 1) {
    value /= base
    unitIndex++
  }

  const valueStr = unitIndex === 0 ? String(value) : value.toFixed(2)
  return `${valueStr} ${units[unitIndex]}`
}

/**
 * Resolve a `FileSizeUnit` to the fixed unit token (or `null` for the dynamic
 * mode). Bytes mode also returns `null` here because the raw-byte path is not
 * a "human-friendly with forced unit" case; it goes through `formatSizeTriads`
 * upstream.
 */
export function fixedUnitFor(unit: FileSizeUnit): 'kB' | 'MB' | 'GB' | null {
  if (unit === 'kB' || unit === 'MB' || unit === 'GB') return unit
  return null
}

/**
 * Magnitude tier of `bytes` under the chosen base — the tier dynamic mode
 * would settle on for this value. Returns an index into the canonical tier
 * order: 0=bytes, 1=kB/KB, 2=MB, 3=GB, 4=TB+ (TB and PB share the top tier).
 *
 * Forced-unit display modes use this so the tier color still tracks the
 * file's real size, even though the rendered label is fixed (a 349-byte file
 * shown as `"0.00 MB"` still gets the bytes-tier color, the same green a user
 * would expect from dynamic mode).
 */
export function dynamicTierIndex(bytes: number, format: FileSizeFormat): number {
  const base = format === 'binary' ? 1024 : 1000
  let value = bytes
  let tier = 0
  while (value >= base && tier < 4) {
    value /= base
    tier++
  }
  return tier
}
