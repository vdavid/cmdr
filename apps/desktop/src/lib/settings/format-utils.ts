/**
 * Formatting utilities for settings-based display.
 * These functions are pure and don't need reactive state.
 */

import type { DateTimeFormat, FileSizeFormat } from './types'
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
 * `parts.left` and `parts.right` each carry an ordered list of segments.
 * Concatenating their `text` reproduces the plain string in `text`. Each
 * segment knows whether it should be wrapped in an age-tier span via its
 * `ageClass`, so the renderer doesn't have to call any tier helper itself.
 */
export interface FormattedDate {
  /** Joined `"left right"` (or just `left` when there's no split). Use for
   *  tooltips, MCP responses, clipboard copies, and other plain-text needs. */
  text: string
  /** Structured halves, each an ordered list of colored segments. */
  parts: { left: DateSegment[]; right: DateSegment[] | null }
}

/** Empty result used for `null` / `0` timestamps. */
const EMPTY_DATE: FormattedDate = {
  text: '',
  parts: { left: [], right: null },
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

  const parts = (() => {
    switch (format) {
      case 'system':
        return systemLocaleParts(date, tiers)
      case 'iso':
        return tokenParts(date, 'YYYY-MM-DD | HH:mm', tiers)
      case 'short':
        return tokenParts(date, 'MM/DD | HH:mm', tiers)
      case 'custom':
        return tokenParts(date, customFormat, tiers)
      default:
        return tokenParts(date, 'YYYY-MM-DD | HH:mm', tiers)
    }
  })()

  const text =
    parts.right === null ? joinSegments(parts.left) : `${joinSegments(parts.left)} ${joinSegments(parts.right)}`

  return { text, parts }
}

/** Per-component tier classes for a single timestamp + "now" pair. */
interface ComponentTiers {
  year: AgeTierClass | null
  month: AgeTierClass | null
  day: AgeTierClass | null
  time: AgeTierClass | null
}

/** Split a token format around an optional `|`, then run each side through `applyTokens`. */
function tokenParts(date: Date, format: string, tiers: ComponentTiers): FormattedDate['parts'] {
  const pipeIdx = format.indexOf('|')
  if (pipeIdx < 0) {
    return { left: applyTokens(date, format, tiers), right: null }
  }
  const leftFmt = format.slice(0, pipeIdx).trimEnd()
  const rightFmt = format.slice(pipeIdx + 1).trimStart()
  const left = applyTokens(date, leftFmt, tiers)
  const right = applyTokens(date, rightFmt, tiers)
  // Treat empty right (e.g. `YYYY-MM-DD |`) as no split.
  if (right.length === 0) return { left, right: null }
  return { left, right }
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
 */
let systemLocaleFormatter: Intl.DateTimeFormat | null = null

function getSystemLocaleFormatter(): Intl.DateTimeFormat {
  if (systemLocaleFormatter === null) {
    systemLocaleFormatter = new Intl.DateTimeFormat(undefined, { dateStyle: 'short', timeStyle: 'medium' })
  }
  return systemLocaleFormatter
}

function systemLocaleParts(date: Date, tiers: ComponentTiers): FormattedDate['parts'] {
  const parts = getSystemLocaleFormatter().formatToParts(date)
  const segments: DateSegment[] = []
  for (const p of parts) {
    segments.push({ text: p.value, ageClass: ageClassForIntlPart(p.type, tiers) })
  }
  return { left: segments, right: null }
}

// ============================================================================
// File Size Formatting
// ============================================================================

// Binary units (base 1024) - traditional computing units
const binaryUnits = ['bytes', 'KB', 'MB', 'GB', 'TB', 'PB']

// SI units (base 1000) - International System of Units
const siUnits = ['bytes', 'kB', 'MB', 'GB', 'TB', 'PB']

/**
 * Format bytes as human-readable string based on the format setting.
 * @param bytes Number of bytes
 * @param format 'binary' uses 1024-based (KB/MB/GB), 'si' uses 1000-based (kB/MB/GB)
 */
export function formatFileSizeWithFormat(bytes: number, format: FileSizeFormat): string {
  const base = format === 'binary' ? 1024 : 1000
  const units = format === 'binary' ? binaryUnits : siUnits

  let value = bytes
  let unitIndex = 0
  while (value >= base && unitIndex < units.length - 1) {
    value /= base
    unitIndex++
  }

  const valueStr = unitIndex === 0 ? String(value) : value.toFixed(2)
  return `${valueStr} ${units[unitIndex]}`
}
