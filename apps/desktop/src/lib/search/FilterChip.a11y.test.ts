/**
 * Tier-3 a11y tests for `FilterChip.svelte`.
 *
 * Covers default, configured, disabled, and open states. The chip is a single `<button>` with
 * `aria-haspopup="dialog"` and `aria-expanded`; the `×` clear control is decorative (the keyboard
 * path is Backspace on the chip), so axe shouldn't flag the nested role-button-tabindex-minus-one
 * pattern.
 */

import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import FilterChip from './FilterChip.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof FilterChip>

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    label: 'Size',
    configured: false,
    isOpen: false,
    onActivate: () => {},
    onClear: () => {},
    ...overrides,
  }
}

describe('FilterChip a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FilterChip, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('configured state (label + value + clear) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FilterChip, { target, props: baseProps({ configured: true, value: '> 100 MB' }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open state (aria-expanded=true) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FilterChip, { target, props: baseProps({ isOpen: true }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FilterChip, { target, props: baseProps({ disabled: true }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
