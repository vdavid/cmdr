import { describe, expect, it } from 'vitest'
import {
  incrementTotalBytes,
  recomputeTotal,
  tryEvict,
  ERROR_REPORT_PREFIX,
  TOTAL_BYTES_KEY,
  EVICTION_LOCK_KEY,
} from './error-report-eviction'

/** In-memory KV stub matching the subset of KVNamespace we use. */
function createKv(initial: Record<string, string> = {}): KVNamespace {
  const store = new Map<string, string>(Object.entries(initial))
  return {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
    delete: (key: string) => {
      store.delete(key)
      return Promise.resolve()
    },
    // used only for inspection in tests
    _store: store,
  } as unknown as KVNamespace & { _store: Map<string, string> }
}

/** In-memory R2 stub. Keys are stored with size + uploaded Date. */
interface StubObj {
  key: string
  size: number
  uploaded: Date
}

function createR2(objects: StubObj[] = []): R2Bucket {
  const store = new Map<string, StubObj>(objects.map((o) => [o.key, o]))
  return {
    list: ({ prefix, cursor, limit }: { prefix?: string; cursor?: string; limit?: number } = {}) => {
      const all = [...store.values()]
        .filter((o) => !prefix || o.key.startsWith(prefix))
        .sort((a, b) => (a.key < b.key ? -1 : 1))
      const pageSize = limit ?? 1000
      const startIdx = cursor ? parseInt(cursor, 10) : 0
      const slice = all.slice(startIdx, startIdx + pageSize)
      const truncated = startIdx + pageSize < all.length
      return Promise.resolve({
        objects: slice.map((o) => ({ key: o.key, size: o.size, uploaded: o.uploaded })),
        truncated,
        cursor: truncated ? String(startIdx + pageSize) : undefined,
      })
    },
    delete: (key: string) => {
      store.delete(key)
      return Promise.resolve()
    },
  } as unknown as R2Bucket
}

const GB = 1024 ** 3

describe('incrementTotalBytes', () => {
  it('adds to an empty counter', async () => {
    const kv = createKv()
    const next = await incrementTotalBytes(kv, 1234)
    expect(next).toBe(1234)
    expect(await kv.get(TOTAL_BYTES_KEY)).toBe('1234')
  })

  it('adds to an existing counter', async () => {
    const kv = createKv({ [TOTAL_BYTES_KEY]: '500' })
    const next = await incrementTotalBytes(kv, 300)
    expect(next).toBe(800)
  })
})

describe('recomputeTotal', () => {
  it('sums object sizes across paginated R2 list', async () => {
    // 2500 objects, each 100 bytes → 250,000 total. R2 page = 1000 max.
    const objs: StubObj[] = Array.from({ length: 2500 }, (_, i) => ({
      key: `${ERROR_REPORT_PREFIX}2026-04-23/ERR-${String(i).padStart(5, '0')}-uuid.zip`,
      size: 100,
      uploaded: new Date(2_000_000_000_000 + i),
    }))
    const bucket = createR2(objs)
    const kv = createKv()
    const total = await recomputeTotal({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv })
    expect(total).toBe(250_000)
    expect(await kv.get(TOTAL_BYTES_KEY)).toBe('250000')
  })

  it('ignores objects outside the prefix', async () => {
    const bucket = createR2([
      { key: 'error-reports/2026-04-23/a.zip', size: 1000, uploaded: new Date() },
      { key: 'other/b.zip', size: 500, uploaded: new Date() },
    ])
    const kv = createKv()
    const total = await recomputeTotal({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv })
    expect(total).toBe(1000)
  })
})

describe('tryEvict', () => {
  it('skips when under the high watermark', async () => {
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(5 * GB) })
    const bucket = createR2()
    const result = await tryEvict({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv })
    expect(result).toEqual({ skipped: 'under_threshold' })
  })

  it('skips when the lock is held', async () => {
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(10 * GB), [EVICTION_LOCK_KEY]: '1' })
    const bucket = createR2()
    const result = await tryEvict({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv })
    expect(result).toEqual({ skipped: 'lock_held' })
  })

  it('evicts oldest first until under the low watermark', async () => {
    const objs: StubObj[] = [
      // Keys sort oldest-first lexically by the date prefix
      {
        key: `${ERROR_REPORT_PREFIX}2026-01-01/ERR-AAAAA-u.zip`,
        size: 2 * GB,
        uploaded: new Date('2026-01-01'),
      },
      {
        key: `${ERROR_REPORT_PREFIX}2026-02-01/ERR-BBBBB-u.zip`,
        size: 2 * GB,
        uploaded: new Date('2026-02-01'),
      },
      {
        key: `${ERROR_REPORT_PREFIX}2026-03-01/ERR-CCCCC-u.zip`,
        size: 2 * GB,
        uploaded: new Date('2026-03-01'),
      },
      {
        key: `${ERROR_REPORT_PREFIX}2026-04-01/ERR-DDDDD-u.zip`,
        size: 2 * GB,
        uploaded: new Date('2026-04-01'),
      },
      {
        key: `${ERROR_REPORT_PREFIX}2026-04-23/ERR-EEEEE-u.zip`,
        size: 1 * GB,
        uploaded: new Date('2026-04-23'),
      },
    ]
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(9 * GB) })

    // Use custom thresholds so we don't need tens of GB of fixtures
    const result = await tryEvict(
      { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv },
      { highWatermark: 8 * GB, lowWatermark: 6 * GB },
    )

    expect('evictedCount' in result).toBe(true)
    if (!('evictedCount' in result)) throw new Error('unreachable')
    // 9 GB → need to drop at least 3 GB → delete oldest (2 GB, 2 GB = 4 GB) to reach 5 GB ≤ 6 GB
    expect(result.evictedCount).toBe(2)
    expect(result.freedBytes).toBe(4 * GB)
    expect(result.newTotal).toBe(5 * GB)

    // Verify the oldest two are gone, newest three remain
    const remaining = await bucket.list({ prefix: ERROR_REPORT_PREFIX })
    expect(remaining.objects.map((o) => o.key).sort()).toEqual([
      `${ERROR_REPORT_PREFIX}2026-03-01/ERR-CCCCC-u.zip`,
      `${ERROR_REPORT_PREFIX}2026-04-01/ERR-DDDDD-u.zip`,
      `${ERROR_REPORT_PREFIX}2026-04-23/ERR-EEEEE-u.zip`,
    ])

    // Lock released after eviction
    expect(await kv.get(EVICTION_LOCK_KEY)).toBeNull()
  })

  it('stops exactly at the low watermark', async () => {
    const objs: StubObj[] = Array.from({ length: 5 }, (_, i) => ({
      key: `${ERROR_REPORT_PREFIX}2026-04-2${String(i)}/ERR-${String(i).padStart(5, '0')}-u.zip`,
      size: 1 * GB,
      uploaded: new Date(`2026-04-0${String(i + 1)}`),
    }))
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(5 * GB) })

    const result = await tryEvict(
      { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv },
      { highWatermark: 4 * GB, lowWatermark: 3 * GB },
    )

    if (!('evictedCount' in result)) throw new Error('expected eviction')
    // 5 GB → 3 GB = delete 2 oldest (2 GB)
    expect(result.evictedCount).toBe(2)
    expect(result.newTotal).toBe(3 * GB)
  })

  it('evicts oldest by key date prefix, then upload time for ties', async () => {
    const objs: StubObj[] = [
      // Same day — two uploads with different upload times
      {
        key: `${ERROR_REPORT_PREFIX}2026-04-01/ERR-AAAAA-u.zip`,
        size: 2 * GB,
        uploaded: new Date('2026-04-01T02:00:00Z'),
      },
      {
        key: `${ERROR_REPORT_PREFIX}2026-04-01/ERR-BBBBB-u.zip`,
        size: 2 * GB,
        uploaded: new Date('2026-04-01T01:00:00Z'),
      },
      {
        key: `${ERROR_REPORT_PREFIX}2026-04-10/ERR-CCCCC-u.zip`,
        size: 2 * GB,
        uploaded: new Date('2026-04-10T00:00:00Z'),
      },
    ]
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(6 * GB) })

    await tryEvict(
      { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv },
      { highWatermark: 5 * GB, lowWatermark: 3 * GB },
    )

    // Both 2026-04-01 entries deleted (same day, sorted by key ascending).
    // Since AAAAA < BBBBB lexically, AAAAA goes first. Then BBBBB. That drops us to 2 GB.
    const remaining = await bucket.list({ prefix: ERROR_REPORT_PREFIX })
    expect(remaining.objects).toHaveLength(1)
    expect(remaining.objects[0].key).toContain('ERR-CCCCC')
  })
})
