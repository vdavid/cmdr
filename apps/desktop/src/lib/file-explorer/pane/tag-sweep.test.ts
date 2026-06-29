import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/tauri-commands', () => ({
  getFileRange: vi.fn(),
}))
vi.mock('$lib/ipc/bindings', () => ({
  commands: { enrichTags: vi.fn().mockResolvedValue({ status: 'ok', data: null }) },
}))

import { getFileRange } from '$lib/tauri-commands'
import { commands } from '$lib/ipc/bindings'
import type { FileEntry } from '../types'
import { sweepListingTags, TAG_SWEEP_CHUNK } from './tag-sweep'

/** Fake a backend range response of `count` entries starting at `start`. The
 *  sweep only reads `.path`, so a partial shape cast to `FileEntry` is enough. */
function fakeRange(start: number, count: number): FileEntry[] {
  return Array.from({ length: count }, (_, i) => ({ path: `/dir/file-${String(start + i)}` })) as unknown as FileEntry[]
}

beforeEach(() => {
  vi.mocked(getFileRange).mockReset()
  vi.mocked(commands.enrichTags).mockClear()
})

describe('sweepListingTags', () => {
  it('enriches the whole listing in chunks', async () => {
    const total = TAG_SWEEP_CHUNK * 2 + 30
    vi.mocked(getFileRange).mockImplementation((_id, start: number, count: number) =>
      Promise.resolve(fakeRange(start, Math.min(count, total - start))),
    )

    await sweepListingTags({ listingId: 'L1', totalCount: total, includeHidden: false, isStale: () => false })

    // Three chunks: 500, 500, 30.
    expect(vi.mocked(getFileRange)).toHaveBeenCalledTimes(3)
    expect(vi.mocked(commands.enrichTags)).toHaveBeenCalledTimes(3)
    expect(vi.mocked(commands.enrichTags).mock.calls[0][1]).toHaveLength(TAG_SWEEP_CHUNK)
    expect(vi.mocked(commands.enrichTags).mock.calls[2][1]).toHaveLength(30)
  })

  it('stops immediately when already stale (no IPC at all)', async () => {
    await sweepListingTags({ listingId: 'L1', totalCount: 5000, includeHidden: false, isStale: () => true })
    expect(vi.mocked(getFileRange)).not.toHaveBeenCalled()
    expect(vi.mocked(commands.enrichTags)).not.toHaveBeenCalled()
  })

  it('stops mid-sweep once isStale flips (e.g. navigation away)', async () => {
    vi.mocked(getFileRange).mockImplementation((_id, start: number, count: number) =>
      Promise.resolve(fakeRange(start, count)),
    )
    // Become stale after the first chunk's enrich.
    let enriched = 0
    vi.mocked(commands.enrichTags).mockImplementation(() => {
      enriched++
      return Promise.resolve({ status: 'ok', data: null } as never)
    })
    const isStale = () => enriched >= 1

    await sweepListingTags({ listingId: 'L1', totalCount: 10_000, includeHidden: false, isStale })

    // One chunk enriched, then the post-await check stops the loop.
    expect(vi.mocked(commands.enrichTags)).toHaveBeenCalledTimes(1)
  })

  it('aborts the sweep if a chunk fetch throws', async () => {
    vi.mocked(getFileRange).mockRejectedValue(new Error('listing gone'))
    await sweepListingTags({ listingId: 'L1', totalCount: 5000, includeHidden: false, isStale: () => false })
    expect(vi.mocked(commands.enrichTags)).not.toHaveBeenCalled()
  })

  it('passes includeHidden through to getFileRange', async () => {
    vi.mocked(getFileRange).mockResolvedValue([])
    await sweepListingTags({ listingId: 'L1', totalCount: 10, includeHidden: true, isStale: () => false })
    expect(vi.mocked(getFileRange)).toHaveBeenCalledWith('L1', 0, TAG_SWEEP_CHUNK, true)
  })
})
