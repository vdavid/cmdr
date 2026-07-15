/**
 * Tier 3 a11y + visibility tests for `MediaIndexReclaim.svelte` (the "delete the extra
 * indexed entries" line + button under the image-index slider).
 *
 * The line renders only once counts settle AND the leftover clears the `shouldOfferReclaim`
 * floor, so each state is driven off a mocked reclaim-preview. The prune round-trip
 * (confirm → prune → toast) is covered by the reclaim E2E; here we pin that the offered
 * state is accessible and the blocked / below-floor states render nothing.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { ReclaimPreview } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

const reclaimPreview = vi.fn<(threshold: number, volumeIds: string[]) => Promise<ReclaimPreview>>()

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexReclaimPreview: (t: number, ids: string[]) => reclaimPreview(t, ids),
  mediaIndexPruneBelowThreshold: vi.fn(),
}))

vi.mock('$lib/media-index/enabled-volumes', () => ({
  getEnabledMediaIndexVolumeIds: () => ['root'],
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

const { default: MediaIndexReclaim } = await import('./MediaIndexReclaim.svelte')

function preview(overrides: Partial<ReclaimPreview> = {}): ReclaimPreview {
  return {
    totalStored: 200_000,
    coveredStored: 150,
    doomedCount: 199_850,
    estimatedBytes: 1_900_000_000,
    pending: false,
    ...overrides,
  }
}

async function mountReclaim(props: Record<string, unknown>): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MediaIndexReclaim, { target, props: { threshold: 0.0, blocked: false, ...props } })
  flushSync()
  await vi.waitFor(() => {
    // Let the effect-driven preview fetch resolve.
    expect(reclaimPreview).toHaveBeenCalled()
  })
  await tick()
  return target
}

describe('MediaIndexReclaim', () => {
  beforeEach(() => {
    reclaimPreview.mockResolvedValue(preview())
  })
  afterEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('offers the reclaim line + button and is accessible when leftover is large', async () => {
    const target = await mountReclaim({})
    const line = target.querySelector('.mi-reclaim-line')?.textContent ?? ''
    expect(line).toContain('200,000')
    expect(line).toContain('199,850')
    expect(target.querySelector('button')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('renders nothing while blocked (waiting on importance / a scan)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MediaIndexReclaim, { target, props: { threshold: 0.0, blocked: true } })
    flushSync()
    await tick()
    expect(target.querySelector('.mi-reclaim')).toBeNull()
    expect(reclaimPreview).not.toHaveBeenCalled()
  })

  it('renders nothing when the leftover is below the offer floor', async () => {
    reclaimPreview.mockResolvedValue(preview({ totalStored: 1000, coveredStored: 990, doomedCount: 10 }))
    const target = await mountReclaim({})
    expect(target.querySelector('.mi-reclaim')).toBeNull()
  })

  it('renders nothing while the backend reports pending', async () => {
    reclaimPreview.mockResolvedValue(preview({ pending: true }))
    const target = await mountReclaim({})
    expect(target.querySelector('.mi-reclaim')).toBeNull()
  })
})
