/**
 * Tier 3 a11y tests for `LoadingIcon.svelte`.
 *
 * Covers each of the four progressive-status states (default, opening,
 * loadedCount, finalizingCount) plus the optional cancel hint.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import LoadingIcon from './LoadingIcon.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('LoadingIcon a11y', () => {
  it('default "Loading..." state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LoadingIcon, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('"Opening folder..." state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LoadingIcon, { target, props: { openingFolder: true } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('loadedCount (plural) state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LoadingIcon, { target, props: { loadedCount: 1200 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('finalizingCount state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LoadingIcon, { target, props: { finalizingCount: 42000 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with showCancelHint has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LoadingIcon, { target, props: { loadedCount: 500, showCancelHint: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
