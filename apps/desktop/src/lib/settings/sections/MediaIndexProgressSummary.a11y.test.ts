/**
 * Tier 3 a11y tests for `MediaIndexProgressSummary.svelte`: the live per-volume
 * image-indexing progress summary shown in the "Enable indexing" settings card. It wraps
 * the shared `IndexingEnrichRow`, so the only local logic is the enriching-volumes gate
 * (renders nothing when idle) and the drive-name resolution. `getEnrichingVolumes` and the
 * volume store are mocked; `tString` resolves the real `en` catalog.
 */
import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import type { VolumeEnrichActivity } from '$lib/indexing/media-enrich-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let enriching: VolumeEnrichActivity[] = []
vi.mock('$lib/indexing', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getEnrichingVolumes: () => enriching,
}))
vi.mock('$lib/stores/volume-store.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD' }],
}))

import MediaIndexProgressSummary from './MediaIndexProgressSummary.svelte'

function activity(overrides: Partial<VolumeEnrichActivity> = {}): VolumeEnrichActivity {
  return {
    volumeId: 'root',
    done: 1_200,
    total: 5_000,
    bytesDone: 2_000_000,
    bytesTotal: 9_000_000,
    paused: null,
    startedAt: Date.now() - 4000,
    ...overrides,
  }
}

async function mountSummary(): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MediaIndexProgressSummary, { target, props: {} })
  await tick()
  return target
}

describe('MediaIndexProgressSummary a11y', () => {
  it('renders nothing while no volume is enriching', async () => {
    enriching = []
    const target = await mountSummary()
    expect(target.querySelector('.mi-progress')).toBeNull()
    target.remove()
  })

  it('a single enriching drive (images + bytes bars) has no violations', async () => {
    enriching = [activity()]
    const target = await mountSummary()
    expect(target.querySelector('.mi-progress')).not.toBeNull()
    expect(target.querySelectorAll('[role="progressbar"]').length).toBe(2)
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('multiple enriching drives have no violations', async () => {
    enriching = [activity(), activity({ volumeId: 'smb-nas', done: 40, total: 900 })]
    const target = await mountSummary()
    expect(target.querySelectorAll('.enrich-row').length).toBeGreaterThanOrEqual(2)
    await expectNoA11yViolations(target)
    target.remove()
  })
})
