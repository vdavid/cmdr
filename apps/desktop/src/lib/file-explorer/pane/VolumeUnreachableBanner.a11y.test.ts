/**
 * Tier 3 a11y tests for `VolumeUnreachableBanner.svelte`.
 *
 * Full-pane "couldn't reach X" banner with retry and "Open home folder"
 * actions. Tests cover idle and retrying states.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import VolumeUnreachableBanner from './VolumeUnreachableBanner.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('VolumeUnreachableBanner a11y', () => {
  it('idle state (retry enabled) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(VolumeUnreachableBanner, {
      target,
      props: {
        originalPath: '/Volumes/Backup',
        retrying: false,
        onRetry: () => {},
        onOpenHome: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('retrying state (retry disabled) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(VolumeUnreachableBanner, {
      target,
      props: {
        originalPath: '/Volumes/Backup',
        retrying: true,
        onRetry: () => {},
        onOpenHome: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
