/**
 * Tier 3 a11y test for `FieldHistoryHint.svelte`: the chip in a field's label row, beside the
 * field it belongs to.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import FieldHistoryHint from './FieldHistoryHint.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('FieldHistoryHint a11y', () => {
  it('has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FieldHistoryHint, { target, props: { onOpen: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
