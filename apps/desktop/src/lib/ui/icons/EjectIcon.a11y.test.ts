/**
 * Tier 3 a11y tests for `EjectIcon.svelte`.
 *
 * The custom macOS eject glyph (Lucide ships none). It's a bare inline svg with no ARIA of its
 * own; callers reach it through `Icon` (`name="eject"`) and supply `aria-hidden` on the wrapping
 * button. axe just confirms the decorative svg produces clean markup.
 */
import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import EjectIcon from './EjectIcon.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('EjectIcon a11y', () => {
  it('decorative eject glyph has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EjectIcon, { target, props: { width: 14, height: 14, 'aria-hidden': 'true' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
