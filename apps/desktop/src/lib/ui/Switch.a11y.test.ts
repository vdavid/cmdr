/**
 * Tier-3 a11y tests for `Switch.svelte`.
 *
 * Ark renders the semantic control as a visually-hidden native `<input type="checkbox">` with
 * `role="switch"`, wrapped in a `<label>` that carries the accessible name; the styled track is an
 * `aria-hidden` `.switch-control`. These tests audit the on, off, and disabled states, and confirm
 * the native input is present and toggles.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import Switch from './Switch.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof Switch>

async function mountSwitch(props: Props): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(Switch, { target, props })
  await tick()
  return target
}

describe('Switch a11y', () => {
  it('off state has no a11y violations', async () => {
    const target = await mountSwitch({ ariaLabel: 'Search subfolders' })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('on state has no a11y violations', async () => {
    const target = await mountSwitch({ ariaLabel: 'Search subfolders', checked: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = await mountSwitch({ ariaLabel: 'Search subfolders', disabled: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('exposes a native switch input that toggles and fires onCheckedChange', async () => {
    const onCheckedChange = vi.fn()
    const target = await mountSwitch({ ariaLabel: 'Search subfolders', onCheckedChange })

    const input = target.querySelector<HTMLInputElement>('input[type="checkbox"]')
    if (!input) throw new Error('expected a native input backing the switch')

    const control = target.querySelector('.switch-control')
    expect(control?.getAttribute('data-state')).toBe('unchecked')

    input.click()
    await tick()

    expect(onCheckedChange).toHaveBeenCalledWith(true)
    expect(control?.getAttribute('data-state')).toBe('checked')

    target.remove()
  })
})
