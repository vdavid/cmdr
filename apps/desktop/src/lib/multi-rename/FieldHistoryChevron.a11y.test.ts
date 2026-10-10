/**
 * Tier 3 a11y test for `FieldHistoryChevron.svelte`: the chevron with a history to open, and
 * disabled with none.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import FieldHistoryChevron from './FieldHistoryChevron.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('FieldHistoryChevron a11y', () => {
  it.each([false, true])('with disabled %s has no violations', async (disabled) => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FieldHistoryChevron, { target, props: { disabled, expanded: false, onOpen: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
