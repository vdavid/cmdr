/**
 * Tier 3 a11y tests for `DriveIndexBadge.svelte`: the focusable, labeled status
 * dot and its open menu must have no axe violations, in each freshness state.
 * Mirrors `IndexingStatusIndicator.a11y.test.ts`.
 */
import { describe, it, expect } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import DriveIndexBadge from './DriveIndexBadge.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { Freshness, VolumeIndexStatus } from '$lib/ipc/bindings'

function makeStatus(freshness: Freshness | null, enabled = freshness != null): VolumeIndexStatus {
  return {
    volumeId: 'smb-test',
    enabled,
    freshness,
    scanCompletedAt: freshness === 'fresh' ? 1_750_000_000 : null,
    scanDurationMs: freshness === 'fresh' ? 134_000 : null,
  }
}

async function mountBadge(status: VolumeIndexStatus) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DriveIndexBadge, { target, props: { volumeId: status.volumeId, status, onAction: () => {} } })
  await tick()
  return target
}

describe('DriveIndexBadge a11y', () => {
  it('the gray (disabled) dot has no violations', async () => {
    const target = await mountBadge(makeStatus(null, false))
    expect(target.querySelector('.drive-index-badge')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the blue (scanning) dot has no violations', async () => {
    const target = await mountBadge(makeStatus('scanning'))
    await expectNoA11yViolations(target)
  })

  it('the green (fresh) dot has no violations', async () => {
    const target = await mountBadge(makeStatus('fresh'))
    await expectNoA11yViolations(target)
  })

  it('the yellow (stale) dot has no violations', async () => {
    const target = await mountBadge(makeStatus('stale'))
    await expectNoA11yViolations(target)
  })

  it('the open menu has no violations', async () => {
    const target = await mountBadge(makeStatus('stale'))
    const badge = target.querySelector<HTMLButtonElement>('.drive-index-badge')
    expect(badge).not.toBeNull()
    badge?.click()
    flushSync()
    expect(target.querySelector('.drive-index-menu')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
