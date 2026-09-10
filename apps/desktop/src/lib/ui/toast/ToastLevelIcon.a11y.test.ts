/**
 * Tier 3 a11y tests for `ToastLevelIcon.svelte`.
 *
 * The level badge at a toast's leading edge: an `aria-hidden` svg with no ARIA of its own, since
 * the toast's role and words carry the level. axe confirms the decorative markup is clean at every
 * level.
 */
import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import ToastLevelIcon from './ToastLevelIcon.svelte'
import type { ToastLevel } from './toast-store.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const levels: ToastLevel[] = ['default', 'info', 'success', 'warn', 'error']

describe('ToastLevelIcon a11y', () => {
  it.each(levels)('%s level badge has no a11y violations', async (level) => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToastLevelIcon, { target, props: { level } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
