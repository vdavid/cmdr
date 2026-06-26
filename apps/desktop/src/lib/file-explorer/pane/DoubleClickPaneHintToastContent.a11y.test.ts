/**
 * Tier 3 a11y tests for `DoubleClickPaneHintToastContent.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DoubleClickPaneHintToastContent from './DoubleClickPaneHintToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({ setSetting: vi.fn(() => Promise.resolve()) }))
vi.mock('$lib/ui/toast', () => ({ dismissToast: vi.fn(() => undefined) }))

describe('DoubleClickPaneHintToastContent a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DoubleClickPaneHintToastContent, { target, props: { toastId: 'hint-1' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
