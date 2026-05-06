/**
 * Tier 3 a11y tests for `LinkButton.svelte`.
 *
 * Runs axe-core in jsdom against each meaningful state. Covers structural
 * a11y (ARIA, labels, focusable-when-enabled). Color contrast is handled
 * by the design-time checker (tier 1). Focus traps / keyboard integration
 * across a full page live in the E2E tier.
 */

import { describe, it } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import LinkButton from './LinkButton.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function snip(text: string) {
  return createRawSnippet(() => ({ render: () => `<span>${text}</span>` }))
}

describe('LinkButton a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LinkButton, { target, props: { children: snip('Open settings') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LinkButton, { target, props: { disabled: true, children: snip('Open settings') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('submit type has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LinkButton, { target, props: { type: 'submit', children: snip('Submit') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('aria-label override has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LinkButton, {
      target,
      props: { 'aria-label': 'Open system appearance settings', children: snip('System Settings > Appearance') },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
