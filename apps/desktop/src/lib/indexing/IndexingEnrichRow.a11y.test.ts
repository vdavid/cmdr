/**
 * Tier 3 a11y tests for `IndexingEnrichRow.svelte`.
 *
 * The "Image indexing" block in the multi-drive indexing tooltip. A pure
 * props-driven presentational component (no store / Tauri deps), so each state is a
 * `mount` with the right props: actively enriching with the images + bytes double bar,
 * paused (both reasons), and with the drive heading. `tString` resolves the real `en`
 * catalog. Mirrors `IndexingDriveRow.a11y.test.ts`.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import IndexingEnrichRow from './IndexingEnrichRow.svelte'
import type { VolumeEnrichActivity } from './media-enrich-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function enrichActivity(overrides: Partial<VolumeEnrichActivity> = {}): VolumeEnrichActivity {
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

const baseProps = {
  activity: enrichActivity(),
  driveName: 'Macintosh HD',
  showHeading: true,
}

async function mountRow(props: Record<string, unknown>): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(IndexingEnrichRow, { target, props: { ...baseProps, ...props } })
  await tick()
  return target
}

describe('IndexingEnrichRow a11y', () => {
  it('actively enriching with the images + bytes double bar has no violations', async () => {
    const target = await mountRow({ activity: enrichActivity() })
    // Two labeled progress bars (images + bytes).
    expect(target.querySelectorAll('[role="progressbar"]').length).toBe(2)
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('paused waiting for idle has no violations (no bars, just the status)', async () => {
    const target = await mountRow({ activity: enrichActivity({ paused: 'waitingForIdle' }) })
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('paused disconnected has no violations', async () => {
    const target = await mountRow({ activity: enrichActivity({ paused: 'disconnected' }) })
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('without a total (indeterminate) renders no bar and has no violations', async () => {
    const target = await mountRow({ activity: enrichActivity({ total: 0, bytesTotal: 0 }) })
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('with the heading hidden has no violations', async () => {
    const target = await mountRow({ activity: enrichActivity(), showHeading: false })
    expect(target.querySelector('.enrich-heading')).toBeNull()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
