/**
 * Tier-3 a11y tests for `Popover.svelte`.
 *
 * Covers the closed state (renders nothing) and the open state (renders a `role="dialog"` with
 * the slot content focusable). The anchor is a real button in the test DOM so the popover has
 * something to position against.
 */

import { describe, it } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import Popover from './Popover.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('Popover a11y', () => {
  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const anchor = document.createElement('button')
    anchor.textContent = 'Anchor'
    target.appendChild(anchor)
    mount(Popover, {
      target,
      props: {
        anchor,
        open: false,
        onClose: () => {},
        children: createRawSnippet(() => ({
          render: () => '<input type="text" aria-label="Test input" />',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open state with a labeled input has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const anchor = document.createElement('button')
    anchor.textContent = 'Anchor'
    target.appendChild(anchor)
    mount(Popover, {
      target,
      props: {
        anchor,
        open: true,
        onClose: () => {},
        ariaLabel: 'Test popover',
        children: createRawSnippet(() => ({
          render: () => '<label for="popover-test">Test field</label><input id="popover-test" type="text" />',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })
})
