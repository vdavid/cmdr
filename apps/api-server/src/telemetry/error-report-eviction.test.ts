import { describe, expect, it } from 'vitest'
import {
  incrementTotalBytes,
  recomputeTotal,
  tryEvict,
  ERROR_REPORT_PREFIX,
  EVICTION_MIN_AGE_DAYS,
  TOTAL_BYTES_KEY,
  EVICTION_LOCK_KEY,
} from './error-report-eviction'
import { INTAKE_PAUSED_KEY } from './error-report-intake'

/** Fixed clock for the age-floor tests, so eligibility never depends on the wall clock. */
const NOW = new Date('2026-08-10T12:00:00Z')

function daysBefore(days: number): Date {
  return new Date(NOW.getTime() - days * 24 * 60 * 60 * 1000)
}

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
    expect(result).toEqual({ outcome: 'skipped', reason: 'under_threshold' })
  })

  it('skips when the lock is held', async () => {
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(10 * GB), [EVICTION_LOCK_KEY]: '1' })
    const bucket = createR2()
    const result = await tryEvict({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv })
    expect(result).toEqual({ outcome: 'skipped', reason: 'lock_held' })
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

    expect(result.outcome).toBe('evicted')
    if (result.outcome !== 'evicted') throw new Error('unreachable')
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

    if (result.outcome !== 'evicted') throw new Error('expected eviction')
    // 5 GB → 3 GB = delete 2 oldest (2 GB)
    expect(result.evictedCount).toBe(2)
    expect(result.newTotal).toBe(3 * GB)
  })

  it('sorts both legacy and new key shapes by the embedded date', async () => {
    // Mixed shapes: legacy (no env segment) vs new (`prod/`). The legacy 2026-01-01
    // entry should evict before the newer `prod/2026-03-01` entry, even though the
    // raw key strings compare differently.
    const objs: StubObj[] = [
      {
        key: `${ERROR_REPORT_PREFIX}prod/2026-03-01/ERR-NEWER-u.zip`,
        size: 2 * GB,
        uploaded: new Date('2026-03-01'),
      },
      {
        key: `${ERROR_REPORT_PREFIX}2026-01-01/ERR-OLDER-u.zip`,
        size: 2 * GB,
        uploaded: new Date('2026-01-01'),
      },
      {
        key: `${ERROR_REPORT_PREFIX}dev/2026-04-01/ERR-EVEN-NEWER-u.zip`,
        size: 1 * GB,
        uploaded: new Date('2026-04-01'),
      },
    ]
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(5 * GB) })

    await tryEvict(
      { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv },
      { highWatermark: 4 * GB, lowWatermark: 3 * GB },
    )

    const remaining = await bucket.list({ prefix: ERROR_REPORT_PREFIX })
    // Oldest by embedded date (2026-01-01) should be the one gone.
    expect(remaining.objects.map((o) => o.key).sort()).toEqual([
      `${ERROR_REPORT_PREFIX}dev/2026-04-01/ERR-EVEN-NEWER-u.zip`,
      `${ERROR_REPORT_PREFIX}prod/2026-03-01/ERR-NEWER-u.zip`,
    ])
  })

  it('evicts oldest by key date prefix, then upload time for ties', async () => {
    const objs: StubObj[] = [
      // Same day: two uploads with different upload times
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

/**
 * The age floor is what stops an upload flood from turning eviction into a delete primitive
 * against real reports. Without it, anyone who can push the bucket past the high watermark makes
 * the oldest (most likely genuine) bundles disappear.
 */
describe('tryEvict age floor', () => {
  const evictionOptions = { highWatermark: 4 * GB, lowWatermark: 3 * GB, now: NOW }

  function freshBundle(index: number, ageDays: number, size: number): StubObj {
    const uploaded = daysBefore(ageDays)
    return {
      key: `${ERROR_REPORT_PREFIX}prod/${uploaded.toISOString().slice(0, 10)}/ERR-${String(index).padStart(5, '0')}-u.zip`,
      size,
      uploaded,
    }
  }

  it('deletes nothing and pauses intake when every bundle is too young to evict', async () => {
    // A flood: 6 GB of bundles uploaded today, well over the 4 GB high watermark.
    const objs = Array.from({ length: 6 }, (_, i) => freshBundle(i, 0, 1 * GB))
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(6 * GB) })

    const result = await tryEvict({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv }, evictionOptions)

    expect(result.outcome).toBe('paused')
    const remaining = await bucket.list({ prefix: ERROR_REPORT_PREFIX })
    expect(remaining.objects).toHaveLength(6)
    expect(await kv.get(INTAKE_PAUSED_KEY)).not.toBeNull()
  })

  it('leaves bundles just under the age floor alone', async () => {
    const objs = [freshBundle(1, EVICTION_MIN_AGE_DAYS - 1, 3 * GB), freshBundle(2, EVICTION_MIN_AGE_DAYS - 2, 3 * GB)]
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(6 * GB) })

    const result = await tryEvict({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv }, evictionOptions)

    expect(result.outcome).toBe('paused')
    const remaining = await bucket.list({ prefix: ERROR_REPORT_PREFIX })
    expect(remaining.objects).toHaveLength(2)
  })

  it('evicts bundles past the age floor and leaves the young ones', async () => {
    const objs = [
      freshBundle(1, EVICTION_MIN_AGE_DAYS + 20, 2 * GB), // eligible, oldest
      freshBundle(2, EVICTION_MIN_AGE_DAYS + 1, 1 * GB), // eligible
      freshBundle(3, 1, 3 * GB), // too young to touch
    ]
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(6 * GB) })

    const result = await tryEvict({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv }, evictionOptions)

    // 6 GB total, need ≤ 3 GB: the two eligible bundles (3 GB) are exactly enough.
    if (result.outcome !== 'evicted') throw new Error('expected eviction')
    expect(result.evictedCount).toBe(2)
    expect(result.freedBytes).toBe(3 * GB)

    const remaining = await bucket.list({ prefix: ERROR_REPORT_PREFIX })
    expect(remaining.objects).toHaveLength(1)
    expect(remaining.objects[0].key).toContain('ERR-00003')
    // Eviction succeeded, so intake keeps running.
    expect(await kv.get(INTAKE_PAUSED_KEY)).toBeNull()
  })

  it('pauses rather than half-evicting when the eligible bundles are not enough', async () => {
    // 2 GB of old bundles cannot bring 6 GB down to 3 GB. Deleting them anyway would destroy real
    // reports and still leave the bucket over its watermark, which is the worst of both.
    const objs = [freshBundle(1, EVICTION_MIN_AGE_DAYS + 5, 2 * GB), freshBundle(2, 0, 4 * GB)]
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(6 * GB) })

    const result = await tryEvict({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv }, evictionOptions)

    expect(result.outcome).toBe('paused')
    if (result.outcome !== 'paused') throw new Error('unreachable')
    expect(result.evictableBytes).toBe(2 * GB)
    expect(result.neededBytes).toBe(3 * GB)

    const remaining = await bucket.list({ prefix: ERROR_REPORT_PREFIX })
    expect(remaining.objects).toHaveLength(2)
  })

  it('is the age floor, not anything else, that spares the fresh bundles', async () => {
    // Same fixture as the pause case above, with the floor dropped to zero: every bundle becomes
    // eligible and eviction proceeds. Pins the pause outcome to the floor rather than to some
    // other guard that would keep passing if the floor were removed.
    const objs = Array.from({ length: 6 }, (_, i) => freshBundle(i, 0, 1 * GB))
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(6 * GB) })

    const result = await tryEvict(
      { ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv },
      { ...evictionOptions, minAgeDays: 0 },
    )

    if (result.outcome !== 'evicted') throw new Error('expected eviction')
    expect(result.evictedCount).toBe(3)
    expect(await kv.get(INTAKE_PAUSED_KEY)).toBeNull()
  })

  it('releases the lock after pausing', async () => {
    const objs = [freshBundle(1, 0, 6 * GB)]
    const bucket = createR2(objs)
    const kv = createKv({ [TOTAL_BYTES_KEY]: String(6 * GB) })

    await tryEvict({ ERROR_REPORTS_BUCKET: bucket, ERROR_REPORT_META: kv }, evictionOptions)

    expect(await kv.get(EVICTION_LOCK_KEY)).toBeNull()
  })
})
