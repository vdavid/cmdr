import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('./quick-filter-intro-actions', () => ({
  QUICK_FILTER_INTRO_TOAST_ID: 'quick-filter-intro',
  switchToJumpMode: vi.fn(),
  openTypingModeSettings: vi.fn(() => Promise.resolve()),
}))

import QuickFilterIntroToastContent from './QuickFilterIntroToastContent.svelte'
import QuickFilterTypingHintToastContent from './QuickFilterTypingHintToastContent.svelte'
import TypeToJumpIndicator from './TypeToJumpIndicator.svelte'

describe('the quick filter surfaces', () => {
  it('the first-filter toast has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(QuickFilterIntroToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the typing hint toast, with its key chip, has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(QuickFilterTypingHintToastContent, { target, props: { keyLabel: 'Space', seconds: 1.5, secondsText: '1.5' } })
    await tick()
    await expectNoA11yViolations(target)
    // The chip carries the key, so the sentence never renders an empty `<code>`.
    expect(target.querySelector('code')?.textContent).toBe('Space')
  })

  it('the filter badge with its clear button has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TypeToJumpIndicator, {
      target,
      props: { buffer: 'rep', visible: true, stale: false, kind: 'filter', onClear: vi.fn() },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
