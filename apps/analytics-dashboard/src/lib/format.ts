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

/** A short local time-of-day (e.g. "09:41 AM") from an ISO timestamp. */
export function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })
}

/** Converts daily rows ({day, views/count}) into uPlot's AlignedData format [timestamps[], values[]]. */
export function toChartData(rows: Array<{ day: string; views?: number; count?: number }>): [number[], number[]] {
  const timestamps = rows.map((r) => new Date(r.day).getTime() / 1000)
  const values = rows.map((r) => r.views ?? r.count ?? 0)
  return [timestamps, values]
}
