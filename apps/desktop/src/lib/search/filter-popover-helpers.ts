/**
 * Pure helpers for the round-2 list-style Size and Modified filter popovers (D10 / D11).
 *
 * The popovers replace the old `<select>` + number-input + `<select>` triplet with a multi-
 * column grid: the user reads each column top-to-bottom and clicks one cell to choose. The
 * columns rerender based on the comparator column's selection (`any` greys cols 2 + 3;
 * `between` adds cols 4 + 5).
 *
 * These helpers compute the labels and disabled states so the Svelte template stays a
 * straight list rendering. Tested in isolation.
 */

import type { FileSizeFormat } from '$lib/settings/types'

// ── Size column 2: numeric presets ────────────────────────────────────────────────────────
//
// The brief specifies 0, 1, 5, 10, 20, 50, 100, 200, 500, plus a "Custom..." escape hatch
// that reveals a free-form input. Strings (not numbers) so the legacy `size-state` IPC
// fields, which carry strings, can land them without coercion.

export const SIZE_PRESETS: readonly string[] = ['0', '1', '5', '10', '20', '50', '100', '200', '500']

/** Special value for "Custom...". Selecting it reveals the inline number input. */
export const CUSTOM_VALUE = '__custom__'

/**
 * Returns the unit-column label given the selected numeric preset and the user's
 * `appearance.fileSizeFormat` setting:
 *   - byte vs bytes: singular only when the value is exactly `'1'`.
 *   - KB vs kB: SI uses `kB` (lower-case k); binary uses `KB`.
 * Used for the col-3 "byte(s)" cell. Other unit labels (MB, GB) stay constant.
 */
export function byteUnitLabel(value: string): string {
  return value === '1' ? 'byte' : 'bytes'
}

export function kiloByteLabel(format: FileSizeFormat): 'KB' | 'kB' {
  return format === 'si' ? 'kB' : 'KB'
}

/**
 * Whether the size popover's value + unit columns should render disabled. True only when
 * the comparator column 1 is `any` (no range to apply). Pure: the caller passes the active
 * comparator string.
 */
export function isSizeRangeDisabled(comparator: string): boolean {
  return comparator === 'any'
}

/**
 * Whether the size popover should render the upper-bound columns (4 + 5). True only when
 * the comparator is `between`.
 */
export function showsUpperBound(comparator: string): boolean {
  return comparator === 'between'
}

// ── Date column 2: preset labels ──────────────────────────────────────────────────────────
//
// D11 calls for `today`, `yesterday`, `this week`, `last week`, `this month`, `last month`,
// `this year`, plus `Custom...`. Each preset has a stable string key the dialog stores; the
// resolver turns the key into the ISO date string the search engine wants.

export interface DatePreset {
  /** Stable identifier the dialog stores. */
  key: string
  /** Human-readable label shown in the popover cell. */
  label: string
}

/**
 * Legacy preset list (round 2 D11). Retained for back-compat with consumers
 * that didn't migrate to the round-3 `buildDatePresets` builder. New code
 * should call the builder so labels read like "today 0:00" / "1st of May
 * 0:00" instead of the static "today" / "this month".
 */
export const DATE_PRESETS: readonly DatePreset[] = [
  { key: 'today', label: 'today' },
  { key: 'yesterday', label: 'yesterday' },
  { key: 'thisWeek', label: 'this week' },
  { key: 'lastWeek', label: 'last week' },
  { key: 'thisMonth', label: 'this month' },
  { key: 'lastMonth', label: 'last month' },
  { key: 'thisYear', label: 'this year' },
]

// ── R3 U4: dynamic Modified preset labels ─────────────────────────────────
//
// David asked for friendlier, more specific labels: instead of "this week" /
// "last week" / "this month" / "last month" / "this year" we render the
// concrete date the preset resolves to. Examples:
//
//   today 0:00
//   yesterday 0:00
//   this Monday 0:00   (or "this Sunday 0:00" on US locales)
//   last Monday 0:00
//   1st of May 0:00    (current month, year omitted because it's the current
//                       year — the user reads "1st of May" without the year)
//   1st of April, 2026, 0:00  (last month: always include the year so a
//                              January-now reader can still parse "1st of
//                              December, 2025")
//   1st of January, 2026, 0:00  (year start; omitted when redundant with
//                                 1st of this/last month)
//
// "custom…" (lowercase, plus ellipsis) lives on the popover footer, not in
// this preset list.

/** Day-of-week numbers per Intl.Locale weekInfo: Mon=1 ... Sun=7. */
type WeekStart = 1 | 2 | 3 | 4 | 5 | 6 | 7

/**
 * Returns the user's first-day-of-week per `Intl.Locale.weekInfo`, falling
 * back to Monday (1) when the API isn't available (older WebKit) or the
 * locale doesn't carry a `firstDay`. The fallback is Monday because the rest
 * of the world uses Monday; US locales should be returning 7 explicitly via
 * the WebKit API on modern macOS.
 */
export function resolveFirstDayOfWeek(language: string | undefined): WeekStart {
  try {
    if (!language) return 1
    // The Intl.Locale type doesn't expose `weekInfo` everywhere (TS lib lags
    // behind Safari). Cast through `unknown` to read it without touching the
    // global types.
    const locale = new Intl.Locale(language) as unknown as {
      weekInfo?: { firstDay?: number }
      getWeekInfo?: () => { firstDay?: number }
    }
    const info = locale.weekInfo ?? locale.getWeekInfo?.()
    const firstDay = info?.firstDay
    if (firstDay && firstDay >= 1 && firstDay <= 7) return firstDay as WeekStart
  } catch {
    // Intl.Locale parse failure: fall back to Monday.
  }
  return 1
}

/** Returns the weekday name (e.g. "Monday") in the user's locale. */
export function weekdayName(dayOfWeek: WeekStart, language?: string): string {
  // 1970-01-05 was a Monday (ISO). Pick a known reference for each weekday so
  // we can localize without a separate table.
  const monday = new Date(Date.UTC(1970, 0, 5)) // Monday
  const date = new Date(monday.getTime() + (dayOfWeek - 1) * 24 * 60 * 60 * 1000)
  const fmt = new Intl.DateTimeFormat(language, { weekday: 'long', timeZone: 'UTC' })
  return fmt.format(date)
}

/** Returns the localized full month name for a 0-indexed month. */
export function monthName(month: number, language?: string): string {
  const date = new Date(Date.UTC(2024, month, 1))
  const fmt = new Intl.DateTimeFormat(language, { month: 'long', timeZone: 'UTC' })
  return fmt.format(date)
}

/**
 * Formats a YYYY-MM-DD ISO date string from a Date (using local time).
 */
function isoLocalDate(d: Date): string {
  const yyyy = String(d.getFullYear()).padStart(4, '0')
  const mm = String(d.getMonth() + 1).padStart(2, '0')
  const dd = String(d.getDate()).padStart(2, '0')
  return `${yyyy}-${mm}-${dd}`
}

/**
 * Builds the round-3 Modified preset list relative to `now`. Labels are
 * dynamic (use the current month/year, the user's first-day-of-week, and
 * locale-aware names). The list omits the year-start preset whenever it
 * coincides with one of the per-month presets (current month is January, OR
 * last month is January).
 *
 * Returns a list of `{ key, label, resolved }` where `resolved` is the ISO
 * YYYY-MM-DD string the search engine should use. The popover renders each
 * label as a cell; clicking the cell calls `setDateValue(resolved)`.
 */
export interface DynamicDatePreset {
  key: string
  label: string
  resolved: string
}

export function buildDatePresets(now: Date = new Date(), language?: string): DynamicDatePreset[] {
  const lang = language ?? (typeof navigator !== 'undefined' ? navigator.language : undefined)
  const firstDay = resolveFirstDayOfWeek(lang)
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  const ms = 24 * 60 * 60 * 1000

  // Days back to the first-day-of-week. JS `getDay()` returns Sun=0...Sat=6,
  // we use ISO numbering Mon=1...Sun=7 throughout for arithmetic clarity.
  const jsDay = today.getDay()
  const isoDay: WeekStart = (jsDay === 0 ? 7 : jsDay) as WeekStart
  // Days from `today` back to the most recent occurrence of `firstDay`. When
  // firstDay matches today, this is 0 (this week starts today).
  const daysBack = (isoDay - firstDay + 7) % 7
  const thisWeekStart = new Date(today.getTime() - daysBack * ms)
  const lastWeekStart = new Date(thisWeekStart.getTime() - 7 * ms)

  const thisMonthFirst = new Date(today.getFullYear(), today.getMonth(), 1)
  const lastMonthFirst = new Date(today.getFullYear(), today.getMonth() - 1, 1)
  const yearStartFirst = new Date(today.getFullYear(), 0, 1)

  const weekdayLabel = weekdayName(firstDay, lang)
  const thisMonthMonth = monthName(thisMonthFirst.getMonth(), lang)
  const lastMonthMonth = monthName(lastMonthFirst.getMonth(), lang)

  const presets: DynamicDatePreset[] = [
    { key: 'today', label: 'today 0:00', resolved: isoLocalDate(today) },
    { key: 'yesterday', label: 'yesterday 0:00', resolved: isoLocalDate(new Date(today.getTime() - ms)) },
    { key: 'thisWeek', label: `this ${weekdayLabel} 0:00`, resolved: isoLocalDate(thisWeekStart) },
    { key: 'lastWeek', label: `last ${weekdayLabel} 0:00`, resolved: isoLocalDate(lastWeekStart) },
    {
      key: 'thisMonth',
      label: `1st of ${thisMonthMonth} 0:00`,
      resolved: isoLocalDate(thisMonthFirst),
    },
    {
      key: 'lastMonth',
      // Last month always carries the year, so a "1st of December, 2025" read
      // in January doesn't get confused for "1st of December, 2026".
      label: `1st of ${lastMonthMonth}, ${String(lastMonthFirst.getFullYear())}, 0:00`,
      resolved: isoLocalDate(lastMonthFirst),
    },
  ]

  // Year-start preset: omit when either current month OR last month is
  // January (the year-start collides with one of those presets, so showing
  // it would be redundant).
  const currentIsJan = today.getMonth() === 0
  const lastIsJan = lastMonthFirst.getMonth() === 0
  if (!currentIsJan && !lastIsJan) {
    presets.push({
      key: 'yearStart',
      label: `1st of January, ${String(yearStartFirst.getFullYear())}, 0:00`,
      resolved: isoLocalDate(yearStartFirst),
    })
  }

  return presets
}

/**
 * Turns a preset key into a YYYY-MM-DD string using `now` as the anchor. Returned date is
 * the **inclusive lower bound** (start of the period): today = the day midnight; yesterday
 * = the day before midnight; this week = Monday of the current week; etc.
 *
 * Returns `null` for unknown keys so callers can fall through to free-form input.
 */
export function resolveDatePreset(key: string, now: Date = new Date()): string | null {
  const startOfDay = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  const ms = 24 * 60 * 60 * 1000

  function format(d: Date): string {
    const yyyy = String(d.getFullYear()).padStart(4, '0')
    const mm = String(d.getMonth() + 1).padStart(2, '0')
    const dd = String(d.getDate()).padStart(2, '0')
    return `${yyyy}-${mm}-${dd}`
  }

  /** Days back to Monday (ISO 8601 start-of-week). Sunday is 7 days back. */
  function daysBackToMonday(date: Date): number {
    const day = date.getDay() // 0..6, Sun=0
    return day === 0 ? 6 : day - 1
  }

  switch (key) {
    case 'today':
      return format(startOfDay)
    case 'yesterday':
      return format(new Date(startOfDay.getTime() - ms))
    case 'thisWeek':
      return format(new Date(startOfDay.getTime() - daysBackToMonday(startOfDay) * ms))
    case 'lastWeek': {
      const thisWeekStart = new Date(startOfDay.getTime() - daysBackToMonday(startOfDay) * ms)
      return format(new Date(thisWeekStart.getTime() - 7 * ms))
    }
    case 'thisMonth':
      return format(new Date(now.getFullYear(), now.getMonth(), 1))
    case 'lastMonth':
      return format(new Date(now.getFullYear(), now.getMonth() - 1, 1))
    case 'thisYear':
      return format(new Date(now.getFullYear(), 0, 1))
    default:
      return null
  }
}

/** Whether the date popover's value cells should render disabled (`any` comparator). */
export function isDateRangeDisabled(comparator: string): boolean {
  return comparator === 'any'
}

/** Whether the date popover should show the upper-bound column (`between`). */
export function showsDateUpperBound(comparator: string): boolean {
  return comparator === 'between'
}
