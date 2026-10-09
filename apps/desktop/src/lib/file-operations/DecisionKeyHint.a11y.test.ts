/** Tier 3 a11y test for `DecisionKeyHint.svelte`: the key chip inside a decision button stays out of its name. */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import DecisionKeyHint from './DecisionKeyHint.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('DecisionKeyHint a11y', () => {
  it('has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DecisionKeyHint, { target, props: { key: 'S' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
