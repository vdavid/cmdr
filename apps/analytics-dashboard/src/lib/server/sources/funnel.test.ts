import { describe, it, expect } from 'vitest'
import { assembleFunnelRows, buildFunnelDateList } from './funnel.js'

interface WorkerDay {
  date: string
  downloads: number
  newInstalls: number
  d7Retention: number | null
  d7Retained: number | null
  newsletterSignups: number | null
}

function workerMap(days: WorkerDay[]): Map<string, WorkerDay> {
  return new Map(days.map((d) => [d.date, d]))
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
      { date: '2026-06-11', downloads: 10, newInstalls: 4, d7Retention: 0.5, d7Retained: 2, newsletterSignups: 3 },
      {
        date: '2026-06-12',
        downloads: 5,
        newInstalls: 1,
        d7Retention: null,
        d7Retained: null,
        newsletterSignups: null,
      },
    ])
    const rows = assembleFunnelRows(dates, worker, null, null, null)
    expect(rows[0]).toMatchObject({
      date: '2026-06-11',
      serverDownloads: 10,
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
    const rows = assembleFunnelRows(dates, null, new Map([['2026-06-11', 7]]), null, null)
    expect(rows[0].serverDownloads).toBeNull()
    expect(rows[0].newInstalls).toBeNull()
    expect(rows[0].d7Retention).toBeNull()
    expect(rows[0].newsletterSignups).toBeNull()
    // Visitors source IS present, so a present day is its real number and a missing day is a real 0.
    expect(rows[0].visitors).toBe(7)
    expect(rows[1].visitors).toBe(0)
  })

  it('treats a missing day inside a present source as a real 0, not a dash', () => {
    const worker = workerMap([
      { date: '2026-06-11', downloads: 2, newInstalls: 0, d7Retention: null, d7Retained: null, newsletterSignups: 0 },
    ])
    const rows = assembleFunnelRows(dates, worker, null, null, new Map([['2026-06-11', 1]]))
    // 2026-06-12 has no worker row but the worker source is present -> downloads 0 (real), not null.
    expect(rows[1].serverDownloads).toBe(0)
    // Purchases source present, no row for 2026-06-12 -> real 0.
    expect(rows[1].purchases).toBe(0)
    expect(rows[0].purchases).toBe(1)
  })
})
