/** Client-safe formatting helpers shared across the dashboard's sections. */

const regionNames = new Intl.DisplayNames(['en'], { type: 'region' })

/** A country code as "Name (CC)", or the raw code when the region name is unknown. */
export function formatCountry(code: string): string {
  try {
    const upper = code.toUpperCase()
    const name = regionNames.of(upper)
    return name && name !== upper ? `${name} (${upper})` : code
  } catch {
    return code
  }
}

/** A number with US thousands separators. */
export function formatNumber(n: number): string {
  return n.toLocaleString('en-US')
}

/** Cents (string or number) as a localized currency amount. */
export function formatCurrency(cents: string | number, currency = 'USD'): string {
  const value = Number(cents) / 100
  return new Intl.NumberFormat('en-US', { style: 'currency', currency }).format(value)
}

/** A percent delta between two values, with its sign and whether it's non-negative (for coloring). */
export function formatDelta(current: number, previous: number): { text: string; positive: boolean } {
  if (previous === 0) return { text: 'N/A', positive: true }
  const pct = ((current - previous) / previous) * 100
  const sign = pct >= 0 ? '+' : ''
  return { text: `${sign}${pct.toFixed(1)}%`, positive: pct >= 0 }
}

/** A count that may be fractional (an average), with one decimal under 100 and none above. */
export function formatAverage(n: number): string {
  return n.toLocaleString('en-US', { maximumFractionDigits: Math.abs(n) < 100 ? 1 : 0 })
}

/** A 0..1+ fraction as a percent: one decimal under 10%, none above. Rates can pass 100%. */
export function formatPercent(fraction: number): string {
  const pct = fraction * 100
  return `${pct.toLocaleString('en-US', { maximumFractionDigits: Math.abs(pct) < 10 ? 1 : 0 })}%`
}

/** A ratio as a multiple, like "3.2×". */
export function formatMultiple(ratio: number): string {
  return `${ratio.toLocaleString('en-US', { maximumFractionDigits: ratio < 10 ? 1 : 0 })}×`
}

/** A relative change (0.5 = +50%), signed. Under 1% rounds to "±0%". */
export function formatRelativeChange(change: number): string {
  const pct = Math.round(change * 100)
  return pct === 0 ? '±0%' : `${pct > 0 ? '+' : '−'}${String(Math.abs(pct))}%`
}

/** An absolute change of two rates (0.012 = +1.2 percentage points), signed, in "pp". */
export function formatPointChange(change: number): string {
  const pp = Math.round(change * 1000) / 10
  return pp === 0 ? '±0 pp' : `${pp > 0 ? '+' : '−'}${String(Math.abs(pp))} pp`
}

/** An absolute change of two multiples (+0.4×), signed. */
export function formatMultipleChange(change: number): string {
  const rounded = Math.round(change * 10) / 10
  return rounded === 0 ? '±0×' : `${rounded > 0 ? '+' : '−'}${String(Math.abs(rounded))}×`
}

/** A short local time-of-day (e.g. "09:41 AM") from an ISO timestamp. */
export function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })
}

/**
 * An ISO timestamp as `YYYY-MM-DD HH:MM` in UTC. UTC rather than local time because every other day
 * bucket on the dashboard is UTC, and a ledger row read in two zones must not look like two events.
 * An unparseable timestamp comes back verbatim, so a broken row shows what it actually holds.
 */
export function formatUtcDateTime(iso: string): string {
  const ms = Date.parse(iso)
  if (Number.isNaN(ms)) return iso
  return new Date(ms).toISOString().slice(0, 16).replace('T', ' ')
}

/** Converts daily rows ({day, views/count}) into uPlot's AlignedData format [timestamps[], values[]]. */
export function toChartData(rows: Array<{ day: string; views?: number; count?: number }>): [number[], number[]] {
  const timestamps = rows.map((r) => new Date(r.day).getTime() / 1000)
  const values = rows.map((r) => r.views ?? r.count ?? 0)
  return [timestamps, values]
}
