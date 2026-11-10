/**
 * Client-safe stats for the Acquisition page's trend charts: the 90/30/7-day stats grid, rolling rates, and
 * axis ticks. Pure, so the page, tests, and any future report share one definition.
 *
 * Two rules hold everywhere here:
 * - **Today is a partial UTC day**, so every stat takes an exclusive `end` day (today, for most metrics) and
 *   ignores it and anything after.
 * - **Every rate pools as a ratio of sums** (Σ numerator ÷ Σ denominator), never an average of daily ratios:
 *   a 1-visitor day at 100% would otherwise weigh as much as a 200-visitor day. The one per-day rate stat is
 *   the median, which is by definition a statistic of the daily values.
 */

/** One day of a count metric. `null` means "couldn't get this", never zero. */
export interface CountPoint {
  date: string
  value: number | null
}

/** One day of a rate metric, as its two raw counts. Either side `null` makes the day unknown. */
export interface RatioPoint {
  date: string
  num: number | null
  den: number | null
}

/** The stats grid's rows for one window. Any cell is `null` when there isn't enough data to say. */
export interface WindowStats {
  average: number | null
  median: number | null
  weekdayAverage: number | null
  weekendAverage: number | null
  /**
   * Versus the previous same-length window. For counts, the relative change (0.5 = +50%); for rates, the
   * absolute difference of the pooled rates (0.01 = +1 percentage point). `null` unless the previous window
   * is fully covered by data.
   */
  change: number | null
}

/** The stats grid's columns, in display order. */
export const statWindows = [90, 30, 7] as const

const msPerDay = 86_400_000

/** `date` (`YYYY-MM-DD`, UTC) moved by `delta` days. */
export function addDays(date: string, delta: number): string {
  return new Date(Date.parse(`${date}T00:00:00Z`) + delta * msPerDay).toISOString().slice(0, 10)
}

/** True for a Saturday or Sunday, read in UTC (every day bucket on the dashboard is UTC). */
export function isWeekend(date: string): boolean {
  const weekday = new Date(`${date}T00:00:00Z`).getUTCDay()
  return weekday === 0 || weekday === 6
}

export function median(values: number[]): number | null {
  if (values.length === 0) return null
  const sorted = [...values].sort((a, b) => a - b)
  const mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2
}

/**
 * The `windowDays` points ending the day before `end`, or `null` when the data doesn't reach back that far
 * (a window the data only half-covers would silently report a different window).
 */
function windowOf<T extends { date: string }>(points: T[], windowDays: number, end: string): T[] | null {
  const start = addDays(end, -windowDays)
  const inWindow = points.filter((p) => p.date >= start && p.date < end)
  return inWindow.length === windowDays ? inWindow : null
}

function mean(values: number[]): number | null {
  return values.length === 0 ? null : values.reduce((a, b) => a + b, 0) / values.length
}

function knownValues(points: CountPoint[]): number[] {
  return points.flatMap((p) => (p.value === null ? [] : [p.value]))
}

const emptyStats: WindowStats = {
  average: null,
  median: null,
  weekdayAverage: null,
  weekendAverage: null,
  change: null,
}

/** The stats grid for a count metric (visitors, clicks, installs) over the `windowDays` before `end`. */
export function countStats(points: CountPoint[], windowDays: number, end: string): WindowStats {
  const current = windowOf(points, windowDays, end)
  if (!current) return emptyStats
  const values = knownValues(current)
  if (values.length === 0) return emptyStats

  const previous = windowOf(points, windowDays, addDays(end, -windowDays))
  const previousValues = previous ? knownValues(previous) : []
  const average = mean(values)
  const previousAverage = previous && previousValues.length === windowDays ? mean(previousValues) : null

  return {
    average,
    median: median(values),
    weekdayAverage: mean(knownValues(current.filter((p) => !isWeekend(p.date)))),
    weekendAverage: mean(knownValues(current.filter((p) => isWeekend(p.date)))),
    change: average !== null && previousAverage ? average / previousAverage - 1 : null,
  }
}

/** Σ num ÷ Σ den over the days where both are known, or `null` when the denominator sums to zero. */
function pooled(points: RatioPoint[]): number | null {
  let num = 0
  let den = 0
  for (const p of points) {
    if (p.num === null || p.den === null) continue
    num += p.num
    den += p.den
  }
  return den > 0 ? num / den : null
}

/**
 * The stats grid for a rate metric over the `windowDays` before `end`. Pass a non-today `end` to anchor on
 * something other than complete days: D7 retention anchors on the newest final cohort (`today - 7`).
 */
export function ratioStats(points: RatioPoint[], windowDays: number, end: string): WindowStats {
  const current = windowOf(points, windowDays, end)
  if (!current) return emptyStats
  const average = pooled(current)
  const previous = windowOf(points, windowDays, addDays(end, -windowDays))
  const previousAverage = previous ? pooled(previous) : null
  const dailyRates = current.flatMap((p) => (p.num !== null && p.den !== null && p.den > 0 ? [p.num / p.den] : []))

  return {
    average,
    median: median(dailyRates),
    weekdayAverage: pooled(current.filter((p) => !isWeekend(p.date))),
    weekendAverage: pooled(current.filter((p) => isWeekend(p.date))),
    change: average !== null && previousAverage !== null ? average - previousAverage : null,
  }
}

/**
 * The trailing `windowDays`-day rate at each point, pooled as a ratio of sums. `null` until a full window of
 * history exists, across any unknown day inside the window, or when the window's denominator is zero.
 */
export function rollingRatio(points: RatioPoint[], windowDays: number): (number | null)[] {
  return points.map((_, i) => {
    if (i + 1 < windowDays) return null
    const slice = points.slice(i + 1 - windowDays, i + 1)
    if (slice.some((p) => p.num === null || p.den === null)) return null
    return pooled(slice)
  })
}

/**
 * Clean axis ticks from 0 to just above `max`: about `target` steps of 1, 2, 2.5, or 5 times a power of ten,
 * never a fractional step on an integer axis (no "37.5 visitors").
 */
export function niceTicks(max: number, options: { target?: number; integer?: boolean } = {}): number[] {
  const target = options.target ?? 4
  const top = max > 0 ? max : 1
  const rough = top / target
  const magnitude = 10 ** Math.floor(Math.log10(rough))
  const multiple = [1, 2, 2.5, 5, 10].find((m) => m * magnitude >= rough - 1e-12) ?? 10
  let step = multiple * magnitude
  if (options.integer) step = Math.max(1, Math.ceil(step))
  const count = Math.ceil(top / step - 1e-9)
  return Array.from({ length: count + 1 }, (_, i) => Math.round(i * step * 1e9) / 1e9)
}
