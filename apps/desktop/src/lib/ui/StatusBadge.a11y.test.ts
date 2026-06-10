import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import StatusBadge from './StatusBadge.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('StatusBadge a11y', () => {
  it('alpha badge has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(StatusBadge, { target, props: { status: 'alpha' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('beta badge has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(StatusBadge, { target, props: { status: 'beta' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
