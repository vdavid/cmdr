import { describe, it, expect } from 'vitest'
import {
  aggregateChannels,
  aggregateReferers,
  aggregateUaFamilies,
  assembleFunnelRows,
  buildFunnelDateList,
  indexWorkerFunnel,
  workerStatusOf,
} from './funnel.js'
import type { UaFamilyCounts } from './funnel.js'

interface WorkerDay {
  date: string
  downloads: number
  downloadsByRef: Record<string, number>
  newInstalls: number
  d7Retention: number | null
  d7Retained: number | null
  newsletterSignups: number | null
}

function workerMap(days: WorkerDay[]) {
  return indexWorkerFunnel(days, '2026-06-11')
}

describe('buildFunnelDateList', () => {
  it('returns N consecutive UTC days, oldest first, ending today', () => {
    expect(buildFunnelDateList(3, new Date('2026-06-12T18:00:00Z'))).toEqual(['2026-06-10', '2026-06-11', '2026-06-12'])
  })
})

describe('assembleFunnelRows', () => {
  const dates = ['2026-06-11', '2026-06-12']

  it('passes worker columns through, including per-column nulls (young D7, Listmonk down)', () => {
    const worker = workerMap([
      {
        date: '2026-06-11',
        downloads: 10,
        downloadsByRef: { hn: 6, '(none)': 4 },
        newInstalls: 4,
        d7Retention: 0.5,
        d7Retained: 2,
        newsletterSignups: 3,
      },
      {
        date: '2026-06-12',
        downloads: 5,
        downloadsByRef: { reddit: 5 },
        newInstalls: 1,
        d7Retention: null,
        d7Retained: null,
        newsletterSignups: null,
      },
    ])
    const rows = assembleFunnelRows(dates, { worker, visitors: null, pageviews: null, clicks: null, purchases: null })
    expect(rows[0]).toMatchObject({
      date: '2026-06-11',
      serverDownloads: 10,
      downloadsByRef: { hn: 6, '(none)': 4 },
      newInstalls: 4,
      d7Retention: 0.5,
      d7Retained: 2,
      newsletterSignups: 3,
      // Umami and Paddle sources were null -> those columns are null (dashes), not 0.
      visitors: null,
      downloadClicks: null,
      purchases: null,
    })
    expect(rows[1].d7Retention).toBeNull()
    expect(rows[1].newsletterSignups).toBeNull()
  })

  it('makes the whole worker-derived columns null when the worker source failed', () => {
    const rows = assembleFunnelRows(dates, {
      worker: null,
      visitors: new Map([['2026-06-11', 7]]),
      pageviews: null,
      clicks: null,
      purchases: null,
    })
    expect(rows[0].serverDownloads).toBeNull()
    expect(rows[0].newInstalls).toBeNull()
    expect(rows[0].d7Retention).toBeNull()
    expect(rows[0].newsletterSignups).toBeNull()
    // The ref breakdown is also unknown when the worker source failed (null, not an empty object).
    expect(rows[0].downloadsByRef).toBeNull()
    // Visitors source IS present, so a present day is its real number and a missing day is a real 0.
    expect(rows[0].visitors).toBe(7)
    expect(rows[1].visitors).toBe(0)
  })

  it('treats a missing day inside a present source as a real 0, not a dash', () => {
    const worker = workerMap([
      {
        date: '2026-06-11',
        downloads: 2,
        downloadsByRef: { hn: 2 },
        newInstalls: 0,
        d7Retention: null,
        d7Retained: null,
        newsletterSignups: 0,
      },
    ])
    const rows = assembleFunnelRows(dates, {
      worker,
      visitors: null,
      pageviews: null,
      clicks: null,
      purchases: new Map([['2026-06-11', 1]]),
    })
    // 2026-06-12 has no worker row but the worker source is present -> downloads 0 (real), not null.
    expect(rows[1].serverDownloads).toBe(0)
    // A day with no worker row but a present worker source has an empty ref breakdown, not null.
    expect(rows[1].downloadsByRef).toEqual({})
    // Purchases source present, no row for 2026-06-12 -> real 0.
    expect(rows[1].purchases).toBe(0)
    expect(rows[0].purchases).toBe(1)
  })
})

describe('trend-chart worker fields', () => {
  const dates = ['2026-06-10', '2026-06-11', '2026-06-12']
  const base = { downloads: 1, downloadsByRef: {}, newInstalls: 1, d7Retention: null, d7Retained: null }
  const none = { visitors: null, pageviews: null, clicks: null, purchases: null }

  it('leaves the new fields null on an older worker, never zero', () => {
    const worker = indexWorkerFunnel([{ date: '2026-06-11', ...base, newsletterSignups: null }], '2026-06-10')
    expect(worker.hasTrendFields).toBe(false)
    const rows = assembleFunnelRows(dates, { worker, ...none })
    expect(rows[1].newInstallsByChannel).toBeNull()
    expect(rows[2].newInstallsByChannel).toBeNull() // an empty day on an older worker stays unknown too
  })

  it('passes the new fields through and zero-fills an empty day on a new worker', () => {
    const worker = indexWorkerFunnel(
      [
        {
          date: '2026-06-11',
          ...base,
          newsletterSignups: null,
          newInstallsByChannel: { homebrew: 1, direct: 0, unknown: 0 },
          downloadsBySource: { website: 0, homebrew: 1, other: 0 },
          retentionSoFar: { day: 1, retained: 1 },
        },
      ],
      '2026-06-10',
    )
    const rows = assembleFunnelRows(dates, { worker, ...none })
    expect(rows[1].newInstallsByChannel).toEqual({ homebrew: 1, direct: 0, unknown: 0 })
    expect(rows[1].retentionSoFar).toEqual({ day: 1, retained: 1 })
    expect(rows[2].newInstallsByChannel).toEqual({ homebrew: 0, direct: 0, unknown: 0 })
  })

  it('treats days before a capped worker window as unknown, not zero', () => {
    // Asked from 2026-06-10, but the worker only answered from 2026-06-11 (an older 90-day cap).
    const worker = indexWorkerFunnel([{ date: '2026-06-11', ...base, newsletterSignups: null }], '2026-06-10')
    expect(worker.since).toBe('2026-06-11')
    const rows = assembleFunnelRows(dates, { worker, ...none })
    expect(rows[0].newInstalls).toBeNull()
    expect(rows[1].newInstalls).toBe(1)
  })
})

describe('workerStatusOf', () => {
  const base = {
    downloads: 1,
    downloadsByRef: {},
    newInstalls: 1,
    d7Retention: null,
    d7Retained: null,
    newsletterSignups: null,
  }
  const oldWorker = indexWorkerFunnel([{ date: '2026-06-11', ...base }], '2026-06-11')
  const newWorker = indexWorkerFunnel(
    [{ date: '2026-06-11', ...base, newInstallsByChannel: { homebrew: 0, direct: 1, unknown: 0 } }],
    '2026-06-11',
  )

  it("refuses an older worker's numbers under a country filter: they'd be global numbers", () => {
    expect(workerStatusOf(oldWorker, 'SE')).toBe('country-unsupported')
    expect(workerStatusOf(newWorker, 'SE')).toBe('ok')
  })

  it("keeps an older worker's numbers for all countries", () => {
    expect(workerStatusOf(oldWorker, null)).toBe('ok')
    expect(workerStatusOf(null, null)).toBe('unavailable')
  })
})

describe('aggregateChannels', () => {
  it('sums refs across days into a ranked list, biggest first, with (none) labeled', () => {
    const rows: { downloadsByRef: Record<string, number> | null }[] = [
      { downloadsByRef: { hn: 6, '(none)': 4 } },
      { downloadsByRef: { reddit: 5, hn: 1 } },
      { downloadsByRef: null },
      { downloadsByRef: {} },
    ]
    expect(aggregateChannels(rows)).toEqual([
      { ref: 'hn', count: 7 },
      { ref: 'reddit', count: 5 },
      { ref: '(none)', count: 4 },
    ])
  })

  it('returns an empty list when no day has any ref data', () => {
    const rows: { downloadsByRef: Record<string, number> | null }[] = [{ downloadsByRef: null }, { downloadsByRef: {} }]
    expect(aggregateChannels(rows)).toEqual([])
  })
})

describe('aggregateReferers', () => {
  it('sums referer hosts across days into a ranked list, biggest first, with (none) labeled', () => {
    const rows: { downloadsByReferer: Record<string, number> | null }[] = [
      { downloadsByReferer: { 'alternativeto.net': 6, '(none)': 4 } },
      { downloadsByReferer: { 'github.com': 5, 'alternativeto.net': 1 } },
      { downloadsByReferer: null },
      { downloadsByReferer: {} },
    ]
    expect(aggregateReferers(rows)).toEqual([
      { ref: 'alternativeto.net', count: 7 },
      { ref: 'github.com', count: 5 },
      { ref: '(none)', count: 4 },
    ])
  })
})

describe('aggregateUaFamilies', () => {
  it('sums the per-day UA-family split and derives humanInstalls = human + unknown (bots excluded)', () => {
    const rows: { downloadsByUaFamily: UaFamilyCounts | null }[] = [
      { downloadsByUaFamily: { human: 4, bot: 6, unknown: 2 } },
      { downloadsByUaFamily: { human: 3, bot: 5, unknown: 1 } },
      { downloadsByUaFamily: null }, // worker unavailable that day -> contributes nothing
    ]
    expect(aggregateUaFamilies(rows)).toEqual({
      human: 7,
      bot: 11,
      unknown: 3,
      total: 21,
      humanInstalls: 10, // 7 human + 3 unknown; the 11 non-mac-UA bots are excluded
    })
  })

  it('returns all-zero totals when no day has UA data', () => {
    const rows: { downloadsByUaFamily: UaFamilyCounts | null }[] = [{ downloadsByUaFamily: null }]
    expect(aggregateUaFamilies(rows)).toEqual({ human: 0, bot: 0, unknown: 0, total: 0, humanInstalls: 0 })
  })
})
