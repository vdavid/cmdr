import { describe, it, expect } from 'vitest'
import type { FunnelRow } from '#lib/server/sources/funnel.js'
import { buildTrendInputs, d7End, d28End } from './acquisition-series.js'

const today = '2026-11-10'

function row(date: string, overrides: Partial<FunnelRow> = {}): FunnelRow {
  return {
    date,
    visitors: 10,
    pageviews: 20,
    downloadClicks: 2,
    serverDownloads: 30,
    downloadsBySource: null,
    downloadsByRef: {},
    downloadsByReferer: {},
    downloadsByUaFamily: null,
    humanInstalls: 3,
    newInstalls: 3,
    newInstallsByChannel: null,
    d7Retention: null,
    d7Retained: null,
    retentionSoFar: null,
    d28Retention: null,
    d28Retained: null,
    d28SoFar: null,
    newsletterSignups: null,
    purchases: null,
    byCountry: {},
    ...overrides,
  }
}

describe('d7End', () => {
  it('is the first cohort still too young for D7', () => {
    // A cohort from 2026-11-02 is 8 days old on 2026-11-10: final. 2026-11-03 isn't yet.
    expect(d7End(today)).toBe('2026-11-03')
  })
})

describe('d28End', () => {
  it('is the first cohort still too young for D28', () => {
    // A cohort from 2026-10-12 is 29 days old on 2026-11-10: final. 2026-10-13 isn't yet.
    expect(d28End(today)).toBe('2026-10-13')
  })
})

describe('buildTrendInputs', () => {
  it('counts a final cohort with installs and its retained users', () => {
    const inputs = buildTrendInputs([row('2026-11-01', { d7Retention: 1 / 3, d7Retained: 1 })], today)
    expect(inputs.d7[0]).toEqual({ date: '2026-11-01', num: 1, den: 3 })
  })

  it('counts a final zero-install cohort as 0 of 0, so it pools without skewing', () => {
    const inputs = buildTrendInputs([row('2026-11-01', { newInstalls: 0 })], today)
    expect(inputs.d7[0]).toEqual({ date: '2026-11-01', num: 0, den: 0 })
  })

  it('leaves a young cohort out of D7 and draws its "so far" point instead', () => {
    const inputs = buildTrendInputs([row('2026-11-07', { retentionSoFar: { day: 3, retained: 2 } })], today)
    expect(inputs.d7[0].num).toBeNull()
    expect(inputs.d7Tentative[0]).toEqual({ value: 2 / 3, label: 'D3 so far' })
    expect(inputs.has.retentionSoFar).toBe(true)
  })

  it('pairs each rate with its own numerator and denominator', () => {
    const inputs = buildTrendInputs([row('2026-11-09')], today)
    expect(inputs.visitorToClick[0]).toMatchObject({ num: 2, den: 10 })
    expect(inputs.clickToInstall[0]).toMatchObject({ num: 3, den: 2 })
    expect(inputs.botMeter[0]).toMatchObject({ num: 30, den: 3 })
    expect(inputs.pageviewsPerVisitor[0]).toMatchObject({ num: 20, den: 10 })
  })

  it('leaves Homebrew installs out of click → install once the channel split is there (they never click)', () => {
    const inputs = buildTrendInputs(
      [row('2026-11-08'), row('2026-11-09', { newInstallsByChannel: { direct: 1, homebrew: 2, unknown: 1 } })],
      today,
    )
    expect(inputs.clickToInstall[0]).toMatchObject({ num: 3, den: 2 })
    expect(inputs.clickToInstall[1]).toMatchObject({ num: 2, den: 2 })
  })

  it('counts final D28 cohorts and draws "so far" points for 8–28-day-old ones', () => {
    const inputs = buildTrendInputs(
      [
        row('2026-10-01', { d28Retention: 1 / 3, d28Retained: 1 }),
        row('2026-10-20', { d28SoFar: { day: 21, retained: 2 } }),
      ],
      today,
    )
    expect(inputs.d28[0]).toEqual({ date: '2026-10-01', num: 1, den: 3 })
    expect(inputs.d28[1].num).toBeNull()
    expect(inputs.d28Tentative[1]).toEqual({ value: 2 / 3, label: 'D21 so far' })
    expect(inputs.has).toMatchObject({ d28: true, d28SoFar: true })
  })

  it('reports no D28 at all on a worker that sends none', () => {
    const inputs = buildTrendInputs([row('2026-10-01', { newInstalls: 0 })], today)
    expect(inputs.has.d28).toBe(false)
  })
})
