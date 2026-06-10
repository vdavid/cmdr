import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import GlobalShortcutAnimation from './GlobalShortcutAnimation.svelte'

describe('GlobalShortcutAnimation a11y', () => {
  it('is a decorative, aria-hidden SVG with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutAnimation, { target })
    await tick()

    // Decorative only: the toast states the keys in text, so the SVG is hidden
    // from assistive tech and out of the tab order.
    const svg = target.querySelector('svg')
    expect(svg?.getAttribute('aria-hidden')).toBe('true')
    expect(svg?.getAttribute('focusable')).toBe('false')

    await expectNoA11yViolations(target)
  })
})
