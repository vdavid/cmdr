/**
 * Tier 3 a11y test for `ListingStalledView.svelte`.
 *
 * One deterministic state: a live `status` region with a spinner, the stalled
 * folder's path, and the Try again / Go back buttons.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'

import ListingStalledView from './ListingStalledView.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('ListingStalledView a11y', () => {
  it('stalled view has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ListingStalledView, {
      target,
      props: { folderPath: '/Volumes/nas/photos', stalledOn: 'server', onRetry: vi.fn(), onGoBack: vi.fn() },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
