/**
 * Client-safe: the Acquisition trend charts' per-country views, all computed in the browser from the one
 * `FunnelData` the page loads, so switching the country chips never asks the server again.
 *
 * - `rowsForCountry`: the funnel rows as one country saw them, from the sparse per-day slices.
 * - `stackByCountry` / `installSplit`: the country-stacked bars, with the rest as "Other" (total minus the
 *   named countries), so a stack always sums to the day's total.
 */
import type { FunnelData, FunnelRow } from '#lib/server/sources/funnel.js'
import type { DownloadsBySource, InstallsByChannel, WorkerCountryDay } from './funnel.js'
import type { ColumnSeries, RateBands } from './daily-chart.js'
import { COLOR_OTHER_COUNTRIES, countryColor, tint } from './country-colors.js'
import { formatCountry } from './format.js'

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
  return layers.map((l) => {
    const base = l.code === otherCountries ? COLOR_OTHER_COUNTRIES : countryColor(l.code)
    return {
      key: `${l.channel}-${l.code}`,
      label: `${countryLabel(l.code)}, ${l.channel}`,
      color: l.channel === 'direct' ? base : tint(base, homebrewTint),
      values: l.values,
      hatch: l.channel === 'unknown',
      // A thin country segment folds into its own channel's "Other", so the channel groups stay aligned.
      ...(l.code === otherCountries ? {} : { foldInto: `${l.channel}-${otherCountries}` }),
    }
  })
}

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
 * couldn't answer stays unknown. Fields nothing splits by country (refs, UA families, signups, purchases)
 * are `null`.
 */
export function rowsForCountry(data: FunnelData, code: string): FunnelRow[] {
  const umamiAsked = data.countries.umami && (data.countries.top.includes(code) || data.countries.extra === code)
  return data.rows.map((r): FunnelRow => {
    const slice = r.byCountry[code] ?? {}
    const umami = (total: number | null, value: number | undefined) =>
      umamiAsked && total !== null ? (value ?? 0) : null
    return {
      date: r.date,
      visitors: umami(r.visitors, slice.visitors),
      pageviews: umami(r.pageviews, slice.pageviews),
      downloadClicks: umami(r.downloadClicks, slice.downloadClicks),
      ...(data.countries.worker && r.serverDownloads !== null ? workerColumns(r, slice.worker) : unknownWorker),
      downloadsByRef: null,
      downloadsByReferer: null,
      downloadsByUaFamily: null,
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

/** A country with nothing that day (the worker omits it). */
const emptyCountryDay: {
  [K in keyof WorkerCountryDay]-?: Exclude<WorkerCountryDay[K], undefined>
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
