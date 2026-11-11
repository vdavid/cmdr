import { describe, expect, it } from 'vitest'
import type { FunnelData, FunnelRow } from '#lib/server/sources/funnel.js'
import type { WorkerCountryDay } from './funnel.js'
import {
  bandsLabel,
  countryStackEntries,
  countrySeries,
  installCountryEntries,
  homebrewTint,
  installSplit,
  otherCountries,
  rowsForCountry,
  shareOf,
  splitSeries,
  stackByCountry,
} from './acquisition-by-country.js'
import { COLOR_OTHER_COUNTRIES, countryColors, tint } from './country-colors.js'
import { formatPercent } from './format.js'

describe('series styling', () => {
  it('colors each country by its fixed color and "Other" grey', () => {
    const series = countrySeries([
      { code: 'US', values: [1] },
      { code: otherCountries, values: [2] },
    ])
    expect(series.map((s) => [s.label, s.color])).toEqual([
      ['United States (US)', countryColors.US],
      ['Other', COLOR_OTHER_COUNTRIES],
    ])
  })

  it('tints Homebrew lighter and stripes the unknown channel, in the country’s own hue', () => {
    const series = splitSeries([
      { code: 'DE', channel: 'direct', values: [1] },
      { code: 'DE', channel: 'homebrew', values: [1] },
      { code: 'DE', channel: 'unknown', values: [1] },
    ])
    expect(series.map((s) => [s.color, s.hatch])).toEqual([
      [countryColors.DE, false],
      [tint(countryColors.DE, homebrewTint), false],
      [tint(countryColors.DE, homebrewTint), true],
    ])
  })

  it('names the bands in words', () => {
    expect(bandsLabel({ yellow: 0.05, green: 0.12 }, formatPercent)).toBe('Red under 5%, yellow 5%–12%, green 12%+')
  })
})

function row(date: string, overrides: Partial<FunnelRow> = {}): FunnelRow {
  return {
    date,
    visitors: 10,
    pageviews: 20,
    downloadClicks: 4,
    serverDownloads: 30,
    downloadsBySource: { website: 20, homebrew: 5, other: 5 },
    downloadsByRef: { hn: 30 },
    downloadsByReferer: {},
    downloadsByUaFamily: { human: 20, bot: 5, unknown: 5 },
    humanInstalls: 25,
    newInstalls: 3,
    newInstallsByChannel: { direct: 2, homebrew: 1, unknown: 0 },
    d7Retention: 1 / 3,
    d7Retained: 1,
    retentionSoFar: null,
    d28Retention: null,
    d28Retained: null,
    d28SoFar: null,
    newsletterSignups: 1,
    purchases: 1,
    byCountry: {},
    ...overrides,
  }
}

const se: WorkerCountryDay = {
  serverDownloads: 6,
  humanInstalls: 5,
  newInstalls: 2,
  newInstallsByChannel: { direct: 1, homebrew: 1, unknown: 0 },
  d7Retained: 1,
  d28Retained: null,
  retentionSoFar: null,
  d28SoFar: { day: 10, retained: 1 },
}

function data(rows: FunnelRow[], countries: Partial<FunnelData['countries']> = {}): FunnelData {
  return {
    rows,
    countries: { top: ['SE', 'DE'], extra: null, umami: true, worker: true, ...countries },
    workerStatus: 'ok',
  }
}

describe('rowsForCountry', () => {
  it("takes the country's own numbers, and a country absent that day is a real 0 on both sides", () => {
    const rows = rowsForCountry(
      data([
        row('2026-11-01', { byCountry: { SE: { visitors: 7, pageviews: 9, downloadClicks: 2, worker: se } } }),
        row('2026-11-02'),
      ]),
      'SE',
    )
    expect(rows[0]).toMatchObject({
      visitors: 7,
      pageviews: 9,
      downloadClicks: 2,
      serverDownloads: 6,
      newInstalls: 2,
      newInstallsByChannel: { direct: 1, homebrew: 1, unknown: 0 },
      d7Retained: 1,
      d7Retention: 0.5,
      d28SoFar: { day: 10, retained: 1 },
      // The worker doesn't split these per country, and Paddle and Listmonk have no country.
      downloadsByRef: null,
      downloadsByUaFamily: null,
      newsletterSignups: null,
      purchases: null,
      byCountry: {},
    })
    expect(rows[1]).toMatchObject({
      visitors: 0,
      downloadClicks: 0,
      serverDownloads: 0,
      newInstalls: 0,
      newInstallsByChannel: { direct: 0, homebrew: 0, unknown: 0 },
      d7Retention: null,
    })
  })

  it('leaves a side unknown when it never answered per country, or for a country Umami wasn’t asked about', () => {
    const rows = [row('2026-11-01', { byCountry: { SE: { worker: se } } })]
    expect(rowsForCountry(data(rows, { worker: false }), 'SE')[0]).toMatchObject({
      serverDownloads: null,
      newInstalls: null,
      d7Retention: null,
      visitors: 0,
    })
    expect(rowsForCountry(data(rows, { umami: false }), 'SE')[0]).toMatchObject({ visitors: null, newInstalls: 2 })
    // FR isn't in `top` or `extra`: Umami never fetched it, while the worker's (absent) slice is a real 0.
    expect(rowsForCountry(data(rows), 'FR')[0]).toMatchObject({ visitors: null, newInstalls: 0 })
  })

  it('keeps a day the totals couldn’t answer unknown for the country too', () => {
    const rows = rowsForCountry(
      data([row('2026-11-01', { visitors: null, serverDownloads: null, newInstalls: null })]),
      'SE',
    )
    expect(rows[0]).toMatchObject({ visitors: null, serverDownloads: null, newInstalls: null })
  })
})

describe('stackByCountry', () => {
  it('stacks the countries in order and puts the rest in "Other" as total minus their sum', () => {
    const stack = stackByCountry(
      [10, 5, null],
      [
        { code: 'SE', values: [4, 5, 1] },
        { code: 'DE', values: [3, 0, 1] },
      ],
    )
    expect(stack).toEqual([
      { code: 'SE', values: [4, 5, null] },
      { code: 'DE', values: [3, 0, null] },
      { code: otherCountries, values: [3, 0, null] },
    ])
  })

  it('never lets "Other" go negative, and makes a day with an unknown country unknown', () => {
    const stack = stackByCountry([3, 4], [{ code: 'SE', values: [5, null] }])
    expect(stack.at(-1)).toEqual({ code: otherCountries, values: [0, null] })
    expect(stack[0].values).toEqual([5, null])
  })
})

describe('shareOf', () => {
  it('is the fraction of the day, or null when the day is empty or unknown', () => {
    expect(shareOf(1, 4)).toBe(0.25)
    expect(shareOf(0, 0)).toBeNull()
    expect(shareOf(null, 4)).toBeNull()
  })
})

describe('installSplit', () => {
  it('groups by channel first, then country, so each channel group sums to that channel’s total', () => {
    const split = installSplit(
      [{ direct: 5, homebrew: 2, unknown: 1 }],
      [
        { code: 'SE', channels: [{ direct: 3, homebrew: 1, unknown: 0 }] },
        { code: 'DE', channels: [{ direct: 1, homebrew: 0, unknown: 1 }] },
      ],
    )
    expect(split.map((s) => `${s.channel}:${s.code}`)).toEqual([
      'direct:SE',
      'direct:DE',
      `direct:${otherCountries}`,
      'homebrew:SE',
      'homebrew:DE',
      `homebrew:${otherCountries}`,
      'unknown:SE',
      'unknown:DE',
      `unknown:${otherCountries}`,
    ])
    const byChannel = (channel: string) =>
      split.filter((s) => s.channel === channel).reduce((sum, s) => sum + (s.values[0] ?? 0), 0)
    expect([byChannel('direct'), byChannel('homebrew'), byChannel('unknown')]).toEqual([5, 2, 1])
    expect(split.find((s) => s.channel === 'direct' && s.code === otherCountries)?.values).toEqual([1])
  })

  it('leaves a day unknown when the totals or any country lack the channel split', () => {
    const split = installSplit(
      [null, { direct: 1, homebrew: 0, unknown: 0 }],
      [{ code: 'SE', channels: [{ direct: 1, homebrew: 0, unknown: 0 }, null] }],
    )
    expect(split.every((s) => s.values[0] === null && s.values[1] === null)).toBe(true)
  })
})

describe('countryStackEntries', () => {
  it('reads each layer on day `i`, in its color, with "Other" marked to sort last', () => {
    const stack = countrySeries([
      { code: 'SE', values: [1, 4] },
      { code: otherCountries, values: [9, 2] },
    ])
    expect(countryStackEntries(stack, 1)).toEqual([
      { label: 'Sweden (SE)', color: countryColors.SE, count: 4, last: false },
      { label: 'Other', color: COLOR_OTHER_COUNTRIES, count: 2, last: true },
    ])
  })
})

describe('installCountryEntries', () => {
  const layers = installSplit(
    [{ direct: 5, homebrew: 2, unknown: 1 }],
    [
      { code: 'SE', channels: [{ direct: 3, homebrew: 1, unknown: 0 }] },
      { code: 'DE', channels: [{ direct: 0, homebrew: 0, unknown: 0 }] },
    ],
  )

  it('sums each country across channels, with the bar split by channel in the split bar’s colors', () => {
    const [se] = installCountryEntries(layers, ['SE', 'DE', otherCountries], 0)
    expect(se).toEqual({
      label: 'Sweden (SE)',
      color: countryColors.SE,
      count: 4,
      detail: '3 direct, 1 Homebrew',
      parts: [
        { value: 3, color: countryColors.SE, hatch: false },
        { value: 1, color: tint(countryColors.SE, homebrewTint), hatch: false },
        { value: 0, color: tint(countryColors.SE, homebrewTint), hatch: true },
      ],
      last: false,
    })
  })

  it('keeps empty countries (the rows helper drops them) and marks "Other" last', () => {
    const entries = installCountryEntries(layers, ['SE', 'DE', otherCountries], 0)
    expect(entries.map((e) => [e.label, e.count, e.last])).toEqual([
      ['Sweden (SE)', 4, false],
      ['Germany (DE)', 0, false],
      ['Other', 4, true],
    ])
    expect(entries[2].detail).toBe('2 direct, 1 Homebrew, 1 unknown')
  })
})
