/**
 * Tier 3 a11y tests for `Button.svelte`.
 *
 * Runs axe-core in jsdom against each meaningful variant/state. Covers
 * structural a11y (ARIA, labels, focusable-when-enabled). Color contrast
 * is handled by the design-time checker (tier 1). Focus traps / keyboard
 * integration across a full page live in the E2E tier.
 */

import { describe, it } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import Button from './Button.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function snip(text: string) {
  return createRawSnippet(() => ({ render: () => `<span>${text}</span>` }))
}

describe('Button a11y', () => {
  it('default (secondary, regular) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Button, { target, props: { children: snip('Action') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('primary variant has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Button, { target, props: { variant: 'primary', children: snip('Save') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('danger variant has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Button, { target, props: { variant: 'danger', children: snip('Delete') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('mini size has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Button, { target, props: { size: 'mini', children: snip('More') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Button, { target, props: { disabled: true, children: snip('Action') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('submit type has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Button, { target, props: { type: 'submit', children: snip('Submit') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('aria-label override has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Button, { target, props: { 'aria-label': 'Save the file', children: snip('Save') } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
