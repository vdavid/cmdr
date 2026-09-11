/**
 * The order a live run carries to the backend, which silences every dialog run older than
 * the one registering. Two starts can reach it in either order, so the order has to say
 * which question came later even when both starts land in the same clock tick.
 */

import { describe, expect, it, vi } from 'vitest'

vi.mock('$lib/tauri-commands', () => ({
  cancelSearch: vi.fn(),
  searchFilesStreaming: vi.fn(),
}))

vi.mock('./live-run-events', () => ({
  liveViewOf: vi.fn(),
  observeSearchRun: vi.fn(),
}))

vi.mock('./walk-handoff.svelte', () => ({
  resumeHandedOffWalk: vi.fn(),
  supersedeHandedOffWalk: vi.fn(),
}))

import { nextRunOrder } from './live-search-source'

describe('nextRunOrder', () => {
  it('gives two starts in the same clock tick different, increasing orders', () => {
    // Far past any clock an earlier test read, so the stamp starts from this tick.
    const tick = 4_000_000_000_000
    const first = nextRunOrder(tick)
    const second = nextRunOrder(tick)

    expect(first).toBe(tick * 1000)
    expect(second).toBe(first + 1)
  })

  it('follows the clock once it moves past the last order handed out', () => {
    const earlier = nextRunOrder(5_000_000_000_000)
    const later = nextRunOrder(5_000_000_000_001)

    expect(later).toBe(5_000_000_000_001 * 1000)
    expect(later).toBeGreaterThan(earlier)
  })
})
