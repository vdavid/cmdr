import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

import RevealEmptyToastContent from './RevealEmptyToastContent.svelte'

describe('RevealEmptyToastContent a11y', () => {
  it('renders with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RevealEmptyToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
