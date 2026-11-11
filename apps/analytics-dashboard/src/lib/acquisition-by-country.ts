/**
 * Client-safe: the Acquisition trend charts' per-country views, all computed in the browser from the one
 * `FunnelData` the page loads, so switching the country chips never asks the server again.
 *
 * - `rowsForCountry`: the funnel rows as one country saw them, from the sparse per-day slices.
 * - `countryChips`: the filter's chips ranked by visitors in the window in view.
 * - `stackByCountry` / `installSplit`: the country-stacked bars, with the rest as "Other" (total minus the
 *   named countries), so a stack always sums to the day's total.
 */
import type { FunnelData, FunnelRow } from '#lib/server/sources/funnel.js'
import type { DownloadsBySource, InstallsByChannel, WorkerCountryDay } from './funnel.js'
import type { BreakdownEntry, ColumnSeries, RateBands } from './daily-chart.js'
import type { ViewWindow } from './time-range.js'
import { COLOR_OTHER_COUNTRIES, countryColor, tint } from './country-colors.js'
import { formatCountry, formatNumber } from './format.js'

/** How much lighter a country's Homebrew segment is than its direct one (share mixed toward white). */
export const homebrewTint = 0.45

/** A country layer's legend and tooltip name. */
export function countryLabel(code: string): string {
  return code === otherCountries ? 'Other' : formatCountry(code)
}

/** Country layers as chart series, each in its fixed color ("Other" grey). */
export function countrySeries(layers: CountryLayer[]): ColumnSeries[] {
  return layers.map((l) => ({
    key: l.code,
    label: countryLabel(l.code),
    color: l.code === otherCountries ? COLOR_OTHER_COUNTRIES : countryColor(l.code),
    values: l.values,
    ...(l.code === otherCountries ? {} : { foldInto: otherCountries }),
  }))
}

/**
 * The split bar's right half as chart series: a country's direct segment in its color, its Homebrew segment a
 * lighter tint of it, and its unknown-channel segment striped in that tint.
 */
export function splitSeries(layers: SplitLayer[]): ColumnSeries[] {
  return layers.map((l) => ({
    key: `${l.channel}-${l.code}`,
    label: `${countryLabel(l.code)}, ${l.channel}`,
    ...splitStyle(l.code, l.channel),
    values: l.values,
    // A thin country segment folds into its own channel's "Other", so the channel groups stay aligned.
    ...(l.code === otherCountries ? {} : { foldInto: `${l.channel}-${otherCountries}` }),
  }))
}

/** A country's own color, "Other" grey. */
function baseColor(code: string): string {
  return code === otherCountries ? COLOR_OTHER_COUNTRIES : countryColor(code)
}

/** How one country-and-channel segment of the split bar looks, shared by the bar and its tooltip. */
function splitStyle(code: string, channel: Channel): { color: string; hatch: boolean } {
  const base = baseColor(code)
  return { color: channel === 'direct' ? base : tint(base, homebrewTint), hatch: channel === 'unknown' }
}

/** A country stack's layers on day `i`, as tooltip breakdown entries ("Other" sorts last). */
export function countryStackEntries(stack: ColumnSeries[], i: number): BreakdownEntry[] {
  return stack.map((s) => ({ label: s.label, color: s.color, count: s.values[i], last: s.key === otherCountries }))
}

/**
 * The split bar's right half on day `i`, one tooltip entry per country in `codes`: its installs across the
 * three channels, with the bar split by channel in the same colors as the chart segments, and the channels
 * in words ("3 direct, 1 Homebrew").
 */
export function installCountryEntries(layers: SplitLayer[], codes: string[], i: number): BreakdownEntry[] {
  return codes.map((code) => {
    const counts = channels.map((channel) => ({
      channel,
      value: layers.find((l) => l.code === code && l.channel === channel)?.values[i] ?? 0,
    }))
    return {
      label: countryLabel(code),
      color: baseColor(code),
      count: counts.reduce((sum, c) => sum + c.value, 0),
      detail: counts
        .filter((c) => c.value > 0)
        .map((c) => `${formatNumber(c.value)} ${channelWords[c.channel]}`)
        .join(', '),
      parts: counts.map((c) => ({ value: c.value, ...splitStyle(code, c.channel) })),
      last: code === otherCountries,
    }
  })
}

/** A channel as the tooltip says it. */
const channelWords: Record<Channel, string> = { direct: 'direct', homebrew: 'Homebrew', unknown: 'unknown' }

/** The bands in words for the legend: "Red under 5%, yellow 5–12%, green 12%+". */
export function bandsLabel(bands: RateBands, format: (n: number) => string): string {
  return `Red under ${format(bands.yellow)}, yellow ${format(bands.yellow)}–${format(bands.green)}, green ${format(bands.green)}+`
}

/** The stack layer for every country without its own: the day's total minus the named countries. */
export const otherCountries = 'other'

/** The channels in stack order, bottom first: today's direct / Homebrew / unknown. */
export const channels = ['direct', 'homebrew', 'unknown'] as const
export type Channel = (typeof channels)[number]

/**
 * The funnel rows as `code` saw them. A side counts a missing slice as a real 0 only when it answered per
 * country at all (and, for Umami, was asked about `code`); otherwise that side is `null`. A day the totals
 * couldn't answer stays unknown. The download breakdowns (refs, referers, UA families) are `null` on a worker
 * that doesn't split them by country; signups and purchases have no country, so they're always `null`.
 */
export function rowsForCountry(data: FunnelData, code: string): FunnelRow[] {
  const umamiAsked = data.countries.umami && (data.countries.top.includes(code) || data.countries.extra === code)
  const { breakdowns } = data.countries
  return data.rows.map((r): FunnelRow => {
    const slice = r.byCountry[code] ?? {}
    const umami = (total: number | null, value: number | undefined) =>
      umamiAsked && total !== null ? (value ?? 0) : null
    const workerKnown = data.countries.worker && r.serverDownloads !== null
    return {
      date: r.date,
      visitors: umami(r.visitors, slice.visitors),
      pageviews: umami(r.pageviews, slice.pageviews),
      downloadClicks: umami(r.downloadClicks, slice.downloadClicks),
      ...(workerKnown ? workerColumns(r, slice.worker) : unknownWorker),
      ...(workerKnown && breakdowns ? breakdownColumns(slice.worker) : unknownBreakdowns),
      newsletterSignups: null,
      purchases: null,
      byCountry: {},
    }
  })
}

type WorkerColumns = Pick<
  FunnelRow,
  | 'serverDownloads'
  | 'downloadsBySource'
  | 'humanInstalls'
  | 'newInstalls'
  | 'newInstallsByChannel'
  | 'd7Retention'
  | 'd7Retained'
  | 'retentionSoFar'
  | 'd28Retention'
  | 'd28Retained'
  | 'd28SoFar'
>

const unknownWorker: WorkerColumns = {
  serverDownloads: null,
  downloadsBySource: null,
  humanInstalls: null,
  newInstalls: null,
  newInstallsByChannel: null,
  d7Retention: null,
  d7Retained: null,
  retentionSoFar: null,
  d28Retention: null,
  d28Retained: null,
  d28SoFar: null,
}

/** One country's worker columns on a day the worker covered: an absent slice is a real empty day. */
function workerColumns(total: FunnelRow, slice: WorkerCountryDay | undefined): WorkerColumns {
  const w = { ...emptyCountryDay, ...slice }
  const retention = (retained: number | null) =>
    retained !== null && w.newInstalls > 0 ? retained / w.newInstalls : null
  return {
    serverDownloads: w.serverDownloads,
    // The splits exist for a country only when the totals carry them (the worker sends both or neither).
    downloadsBySource: total.downloadsBySource ? w.downloadsBySource : null,
    humanInstalls: w.humanInstalls,
    newInstalls: w.newInstalls,
    newInstallsByChannel: total.newInstallsByChannel ? w.newInstallsByChannel : null,
    d7Retention: retention(w.d7Retained),
    d7Retained: w.d7Retained,
    retentionSoFar: w.retentionSoFar,
    d28Retention: retention(w.d28Retained),
    d28Retained: w.d28Retained,
    d28SoFar: w.d28SoFar,
  }
}

type BreakdownColumns = Pick<FunnelRow, 'downloadsByRef' | 'downloadsByReferer' | 'downloadsByUaFamily'>

const unknownBreakdowns: BreakdownColumns = {
  downloadsByRef: null,
  downloadsByReferer: null,
  downloadsByUaFamily: null,
}

/** One country's download breakdowns: the worker sends them sparse, and drops them on a day with no downloads. */
function breakdownColumns(slice: WorkerCountryDay | undefined): BreakdownColumns {
  const ua = slice?.downloadsByUaFamily ?? {}
  return {
    downloadsByRef: slice?.downloadsByRef ?? {},
    downloadsByReferer: slice?.downloadsByReferer ?? {},
    downloadsByUaFamily: { human: ua.human ?? 0, bot: ua.bot ?? 0, unknown: ua.unknown ?? 0 },
  }
}

/** A chip of the country filter: a country and its visitors in the window in view (`null`: Umami couldn't say). */
export interface CountryChip {
  code: string
  visitors: number | null
}

/**
 * The country chips (`top`, then `extra`), ranked by visitors over `window`, biggest first; a tie keeps the
 * 90-day order. Without per-country Umami numbers, the chips keep that order and carry no count.
 */
export function countryChips(data: FunnelData, window: ViewWindow): CountryChip[] {
  const { top, extra, umami } = data.countries
  const codes = extra ? [...top, extra] : top
  if (!umami) return codes.map((code) => ({ code, visitors: null }))
  const inView = data.rows.filter((r) => r.date >= window.start && r.date <= window.end && r.visitors !== null)
  const chips = codes.map((code) => ({
    code,
    visitors: inView.length > 0 ? inView.reduce((sum, r) => sum + ((r.byCountry[code] ?? {}).visitors ?? 0), 0) : null,
  }))
  // `toSorted` is stable, so equal counts keep the 90-day order.
  return chips.toSorted((a, b) => (b.visitors ?? 0) - (a.visitors ?? 0))
}

/** A country with nothing that day (the worker omits it). The breakdowns have their own defaults (`breakdownColumns`). */
const emptyCountryDay: {
  [K in Exclude<keyof WorkerCountryDay, keyof BreakdownColumns>]-?: Exclude<WorkerCountryDay[K], undefined>
} & { downloadsBySource: DownloadsBySource; newInstallsByChannel: InstallsByChannel } = {
  serverDownloads: 0,
  humanInstalls: 0,
  downloadsBySource: { website: 0, homebrew: 0, other: 0 },
  newInstalls: 0,
  newInstallsByChannel: { direct: 0, homebrew: 0, unknown: 0 },
  d7Retained: null,
  d28Retained: null,
  retentionSoFar: null,
  d28SoFar: null,
}

/** One stack layer: a country code (or `otherCountries`) and its value per day. */
export interface CountryLayer {
  code: string
  values: (number | null)[]
}

/**
 * The named countries' layers in the given order, then "Other" = total minus their sum (never below 0). A
 * day where the total or any named country is unknown is unknown in every layer, so a stack never draws a
 * partial column that looks like a real total.
 */
export function stackByCountry(totals: (number | null)[], countries: CountryLayer[]): CountryLayer[] {
  const known = totals.map((t, i) => t !== null && countries.every((c) => c.values[i] !== null))
  const named = countries.map((c) => ({ code: c.code, values: c.values.map((v, i) => (known[i] ? v : null)) }))
  const other = totals.map((t, i) =>
    known[i] && t !== null ? Math.max(0, t - countries.reduce((sum, c) => sum + (c.values[i] ?? 0), 0)) : null,
  )
  return [...named, { code: otherCountries, values: other }]
}

/** `value` as a fraction of the day's `total`, or `null` when either is unknown or the day is empty. */
export function shareOf(value: number | null, total: number | null): number | null {
  return value === null || total === null || total === 0 ? null : value / total
}

/** One layer of the split bar's right half: a channel and a country (or `otherCountries`). */
export interface SplitLayer extends CountryLayer {
  channel: Channel
}

/**
 * The right half of the new-installs split bar: grouped by channel first (direct, Homebrew, unknown), then by
 * country within each, so each channel group is exactly as tall as that channel's segment in the left half.
 */
export function installSplit(
  totals: (InstallsByChannel | null)[],
  countries: { code: string; channels: (InstallsByChannel | null)[] }[],
): SplitLayer[] {
  return channels.flatMap((channel) =>
    stackByCountry(
      totals.map((t, i) => (t && countries.every((c) => c.channels[i]) ? t[channel] : null)),
      countries.map((c) => ({ code: c.code, values: c.channels.map((v) => (v ? v[channel] : null)) })),
    ).map((layer) => ({ ...layer, channel })),
  )
}
