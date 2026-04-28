/**
 * Tier 3 a11y tests for `RepoChip.svelte`.
 *
 * Covers each of the six visual states (clean, ahead, behind, dirty,
 * detached, unborn) so axe sees the chip's structural a11y in every
 * shape it ever renders. Tooltip + screen-reader handoff lives on the
 * pill itself via `aria-label`, which is what makes axe-core happy
 * without needing the tooltip directive to be primed.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import RepoChip from './RepoChip.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { RepoInfo } from './git-store.svelte'

function base(overrides: Partial<RepoInfo> = {}): RepoInfo {
  return {
    repoRoot: '/repo',
    branch: 'main',
    detachedSha: null,
    unborn: false,
    upstream: 'origin/main',
    ahead: 0,
    behind: 0,
    isDirty: false,
    ...overrides,
  }
}

async function mountChip(info: RepoInfo) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(RepoChip, { target, props: { info } })
  await tick()
  return target
}

describe('RepoChip a11y', () => {
  it('clean state has no a11y violations', async () => {
    await expectNoA11yViolations(await mountChip(base()))
  })

  it('ahead state has no a11y violations', async () => {
    await expectNoA11yViolations(await mountChip(base({ ahead: 3 })))
  })

  it('behind state has no a11y violations', async () => {
    await expectNoA11yViolations(await mountChip(base({ behind: 2 })))
  })

  it('dirty state has no a11y violations', async () => {
    await expectNoA11yViolations(await mountChip(base({ isDirty: true })))
  })

  it('detached state has no a11y violations', async () => {
    await expectNoA11yViolations(
      await mountChip(base({ branch: null, detachedSha: 'a1b2c3d', upstream: null, ahead: null, behind: null })),
    )
  })

  it('unborn state has no a11y violations', async () => {
    await expectNoA11yViolations(
      await mountChip(base({ branch: 'main', unborn: true, upstream: null, ahead: null, behind: null })),
    )
  })
})
